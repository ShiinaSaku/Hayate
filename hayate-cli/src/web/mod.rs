//! `hayate web` — a loopback dashboard backed by a SvelteKit SPA.
//!
//! The UI lives in `web/` at the repo root and is built with Bun + Vite into
//! `web/build`, which is embedded at compile time. Rust builds therefore need
//! no JavaScript toolchain; refresh the bundle with `mise run web` and commit
//! the result (CI checks it is up to date).
//!
//! Transfers:
//!
//! * **Send** — the browser uploads a file body to `POST /api/send`, which
//!   spools it to a temp file and hands it to the same engine the CLI uses.
//! * **Receive** — `POST /api/receive/start` runs a listener on a dedicated
//!   thread with its own compio runtime, auto-accepting into a fixed directory
//!   (default `~/Downloads/Hayate`) so the UI never blocks a consent callback.
//!
//! Security model:
//!
//! * binds `127.0.0.1` unless `--allow-lan` is passed;
//! * every request must carry the per-launch token (cookie or `?t=`);
//! * state-changing POSTs additionally require `X-Hayate-Request: 1`, which a
//!   cross-site form cannot set (and a cross-origin fetch would need CORS
//!   preflight, which is never granted);
//! * a `Host` header that is not this machine's loopback address is rejected
//!   (DNS-rebinding defense);
//! * uploads are capped, filenames are reduced to a basename, and receive
//!   writes only inside the configured directory;
//! * responses set a strict CSP, `X-Content-Type-Options: nosniff`, and a
//!   connection cap. The SPA bootstrap script is inline, hence `script-src
//!   'unsafe-inline'` — the page renders no untrusted HTML.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use compio::io::{AsyncRead, AsyncWriteAtExt, AsyncWriteExt};
use compio::net::{TcpListener, TcpStream};
use include_dir::{Dir, include_dir};
use rand::RngExt;
use serde::Serialize;

use crate::cli::WebArgs;
use crate::{history, output};

/// Embedded SvelteKit build output (`mise run web` regenerates it).
static UI: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../web/build");

const MAX_REQUEST_BYTES: usize = 16 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const BODY_STALL_TIMEOUT: Duration = Duration::from_secs(60);
const PEER_REFRESH: Duration = Duration::from_secs(5);
const SSE_INTERVAL: Duration = Duration::from_secs(1);
/// Run an mDNS browse every Nth refresh (UDP query covers the rest).
const MDNS_EVERY: u32 = 6;

/// Concurrent connection cap. SSE connections are long-lived, so an unbounded
/// accept loop would let a local process exhaust file descriptors.
const MAX_CONNECTIONS: usize = 64;

/// Largest accepted upload (the body is spooled to disk, not memory).
const MAX_UPLOAD_BYTES: u64 = 16 * 1024 * 1024 * 1024;

/// Default receive port shown in the UI.
const DEFAULT_RECEIVE_PORT: u16 = 50001;

/// Cookie name carrying the dashboard token.
const TOKEN_COOKIE: &str = "hayate_web";

const CACHE_IMMUTABLE: &str = "public, max-age=31536000, immutable";
const CACHE_NONE: &str = "no-store";

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// One peer as rendered by the dashboard.
#[derive(Clone, Serialize)]
struct PeerView {
    name: String,
    address: String,
    os: String,
    version: String,
    rtt_ms: Option<f64>,
}

#[derive(Default)]
struct PeerState {
    peers: Vec<PeerView>,
    updated: Option<Instant>,
    rounds: u32,
}

/// One transfer, live or finished, as rendered by the dashboard.
#[derive(Clone, Serialize)]
struct TransferView {
    id: u64,
    direction: &'static str,
    filename: String,
    peer: String,
    total: u64,
    transferred: u64,
    /// `uploading` | `active` | `done` | `failed` | `rejected`
    status: &'static str,
    error: Option<String>,
    checksum: Option<String>,
    started: u64,
    finished: Option<u64>,
}

/// State of the browser-controlled receiver.
struct ReceiveState {
    running: bool,
    stopping: bool,
    port: u16,
    dir: String,
    error: Option<String>,
    stop: Option<Arc<AtomicBool>>,
}

impl Default for ReceiveState {
    fn default() -> Self {
        Self {
            running: false,
            stopping: false,
            port: DEFAULT_RECEIVE_PORT,
            dir: String::new(),
            error: None,
            stop: None,
        }
    }
}

struct Ctx {
    token: String,
    started: Instant,
    listen: SocketAddr,
    hostname: String,
    peers: Arc<Mutex<PeerState>>,
    transfers: Arc<Mutex<Vec<TransferView>>>,
    receive: Arc<Mutex<ReceiveState>>,
    next_transfer: AtomicU64,
    upload_dir: PathBuf,
    download_dir: PathBuf,
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

pub async fn run(args: WebArgs, cancelled: Arc<AtomicBool>) -> Result<()> {
    if !args.bind.is_loopback() && !args.allow_lan {
        bail!(
            "refusing to bind the dashboard to {} — pass --allow-lan to expose it beyond this machine",
            args.bind
        );
    }

    let bind = SocketAddr::new(args.bind, args.port);
    let listener = TcpListener::bind(bind).await.context("could not bind the web dashboard")?;
    let listen = listener.local_addr().context("could not read the dashboard address")?;

    let token = generate_token();
    let upload_dir = std::env::temp_dir().join("hayate-web");
    let download_dir = dirs::download_dir().unwrap_or_else(|| PathBuf::from(".")).join("Hayate");
    let _ = std::fs::create_dir_all(&upload_dir);
    let _ = std::fs::create_dir_all(&download_dir);
    clean_stale_uploads(&upload_dir);

    let ctx = Arc::new(Ctx {
        token: token.clone(),
        started: Instant::now(),
        listen,
        hostname: hayate::discovery::local_hostname(),
        peers: Arc::new(Mutex::new(PeerState::default())),
        transfers: Arc::new(Mutex::new(Vec::new())),
        receive: Arc::new(Mutex::new(ReceiveState {
            dir: download_dir.display().to_string(),
            ..ReceiveState::default()
        })),
        next_transfer: AtomicU64::new(1),
        upload_dir,
        download_dir,
    });

    refresh_peers_soon(Arc::clone(&ctx));
    spawn_refresher(Arc::clone(&ctx));

    let shown_ip =
        if listen.ip().is_unspecified() { IpAddr::V4(Ipv4Addr::LOCALHOST) } else { listen.ip() };
    output::ok(&format!("Dashboard: http://{shown_ip}:{}/?t={token}", listen.port()));
    if !args.bind.is_loopback() {
        if let Some(ip) = hayate::local_addr::primary_local_ipv4() {
            output::info(&format!("On the LAN: http://{ip}:{}/?t={token}", listen.port()));
        }
        output::warn(
            "--allow-lan is active: anyone who obtains this URL can send and receive files.",
        );
    }
    output::info("Press Ctrl+C to stop.");

    let active = Arc::new(AtomicUsize::new(0));

    while !cancelled.load(Ordering::SeqCst) {
        match compio::time::timeout(Duration::from_millis(500), listener.accept()).await {
            Ok(Ok((stream, _peer))) => {
                if active.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                    drop(stream);
                    continue;
                }
                active.fetch_add(1, Ordering::Relaxed);
                let ctx = Arc::clone(&ctx);
                let active = Arc::clone(&active);
                compio::runtime::spawn(async move {
                    let _ = handle(stream, ctx).await;
                    active.fetch_sub(1, Ordering::Relaxed);
                })
                .detach();
            },
            Ok(Err(_)) => continue,
            Err(_) => {},
        }
    }

    if let Some(stop) = ctx.receive.lock().unwrap_or_else(|e| e.into_inner()).stop.as_ref() {
        stop.store(true, Ordering::SeqCst);
    }

    Ok(())
}

/// Removes upload directories left behind by a crash (older than a day).
fn clean_stale_uploads(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let cutoff = SystemTime::now() - Duration::from_secs(24 * 60 * 60);
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else { continue };
        let Ok(modified) = metadata.modified() else { continue };
        if modified < cutoff {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// 128-bit hex token; regenerated every launch so a stale URL stops working.
fn generate_token() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 16] = rng.random();
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Peer discovery
// ---------------------------------------------------------------------------

fn spawn_refresher(ctx: Arc<Ctx>) {
    compio::runtime::spawn(async move {
        loop {
            compio::time::sleep(PEER_REFRESH).await;
            refresh_peers(&ctx).await;
        }
    })
    .detach();
}

/// Runs the first peer scan without delaying the printed dashboard URL.
fn refresh_peers_soon(ctx: Arc<Ctx>) {
    compio::runtime::spawn(async move {
        refresh_peers(&ctx).await;
    })
    .detach();
}

async fn refresh_peers(ctx: &Arc<Ctx>) {
    let use_mdns = {
        let state = ctx.peers.lock().unwrap_or_else(|e| e.into_inner());
        state.rounds.is_multiple_of(MDNS_EVERY) || state.peers.is_empty()
    };

    let found = compio::runtime::spawn_blocking(move || {
        let mut peers = hayate::discovery::query(Duration::from_millis(600)).unwrap_or_default();
        if use_mdns {
            peers.extend(hayate::discovery::scan(Duration::from_millis(900)).unwrap_or_default());
        }
        peers
    })
    .await;

    let found = found.unwrap_or_default();

    let mut state = ctx.peers.lock().unwrap_or_else(|e| e.into_inner());
    state.rounds = state.rounds.wrapping_add(1);
    state.updated = Some(Instant::now());

    for peer in found {
        let view = PeerView {
            name: peer.name,
            address: peer.addr.to_string(),
            os: peer.os,
            version: peer.version,
            rtt_ms: peer.rtt_ms,
        };
        // Same peer can answer the loopback and broadcast queries; keep one row
        // and prefer the routable address.
        if let Some(existing) = state.peers.iter_mut().find(|known| {
            known.address.rsplit(':').next() == view.address.rsplit(':').next()
                && known.name == view.name
        }) {
            let existing_loopback = existing.address.starts_with("127.");
            let new_loopback = view.address.starts_with("127.");
            if existing_loopback && !new_loopback {
                *existing = view;
            }
            continue;
        }
        state.peers.push(view);
    }
    state.peers.sort_by(|a, b| a.name.cmp(&b.name).then(a.address.cmp(&b.address)));
}

// ---------------------------------------------------------------------------
// Transfer bookkeeping
// ---------------------------------------------------------------------------

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Ctx {
    fn push_transfer(&self, mut transfer: TransferView) -> u64 {
        let id = self.next_transfer.fetch_add(1, Ordering::Relaxed);
        transfer.id = id;
        transfer.started = now_secs();
        let mut list = self.transfers.lock().unwrap_or_else(|e| e.into_inner());
        list.insert(0, transfer);
        list.truncate(50);
        id
    }

    fn update_transfer(&self, id: u64, update: impl FnOnce(&mut TransferView)) {
        let mut list = self.transfers.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(transfer) = list.iter_mut().find(|transfer| transfer.id == id) {
            update(transfer);
        }
    }

    fn finish_transfer(&self, id: u64, checksum: Option<String>) {
        self.update_transfer(id, |transfer| {
            transfer.transferred = transfer.total;
            transfer.status = "done";
            transfer.checksum = checksum;
            transfer.finished = Some(now_secs());
        });
    }

    fn fail_transfer(&self, id: u64, error: impl Into<String>) {
        let error = error.into();
        self.update_transfer(id, |transfer| {
            transfer.status = "failed";
            transfer.error = Some(error);
            transfer.finished = Some(now_secs());
        });
    }
}

// ---------------------------------------------------------------------------
// HTTP handling
// ---------------------------------------------------------------------------

struct Request {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
}

async fn handle(mut stream: TcpStream, ctx: Arc<Ctx>) -> std::io::Result<()> {
    let Some((request, body_prefix)) = read_request(&mut stream).await? else {
        return plain(&mut stream, 400, "bad request").await;
    };

    let (path, query) = split_target(&request.target);

    if !host_allowed(&request.headers, ctx.listen) {
        return plain(&mut stream, 403, "forbidden host").await;
    }
    if !authorized(&request.headers, query, &ctx.token) {
        return plain(&mut stream, 401, UNAUTHORIZED_TEXT).await;
    }

    if request.method == "POST" {
        // A cross-site form can send a POST, but it cannot set this header; a
        // cross-origin fetch would need a CORS preflight we never grant.
        if !header_is(&request.headers, "x-hayate-request", "1") {
            return json_error(&mut stream, 403, "missing X-Hayate-Request header").await;
        }
        return match path {
            "/api/send" => {
                handle_send(stream, Arc::clone(&ctx), query, body_prefix, &request.headers).await
            },
            "/api/receive/start" => {
                handle_receive_start(stream, Arc::clone(&ctx), query, body_prefix).await
            },
            "/api/receive/stop" => handle_receive_stop(stream, Arc::clone(&ctx)).await,
            _ => json_error(&mut stream, 404, "not found").await,
        };
    }

    if request.method != "GET" {
        return plain(&mut stream, 405, "method not allowed").await;
    }

    match path {
        "/api/status" => {
            let body = serde_json::to_vec(&status_json(&ctx)).unwrap_or_default();
            respond(&mut stream, 200, "application/json", &body, CACHE_NONE, &[]).await
        },
        "/api/peers" => {
            let body = serde_json::to_vec(&peers_json(&ctx)).unwrap_or_default();
            respond(&mut stream, 200, "application/json", &body, CACHE_NONE, &[]).await
        },
        "/api/transfers" => {
            let body = serde_json::to_vec(&transfers_json(&ctx)).unwrap_or_default();
            respond(&mut stream, 200, "application/json", &body, CACHE_NONE, &[]).await
        },
        "/api/history" => {
            let limit = query_param(query, "limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(25)
                .min(500);
            let entries = history::recent(limit).unwrap_or_default();
            let body = serde_json::to_vec(&entries).unwrap_or_default();
            respond(&mut stream, 200, "application/json", &body, CACHE_NONE, &[]).await
        },
        "/api/events" => sse(stream, ctx).await,
        _ => serve_asset(&mut stream, path, &ctx).await,
    }
}

const UNAUTHORIZED_TEXT: &str =
    "Unauthorized.\nOpen the URL printed by `hayate web` (it contains the access token).\n";

// ---------------------------------------------------------------------------
// Send: upload → engine
// ---------------------------------------------------------------------------

async fn handle_send(
    mut stream: TcpStream,
    ctx: Arc<Ctx>,
    query: Option<&str>,
    body_prefix: Vec<u8>,
    headers: &[(String, String)],
) -> std::io::Result<()> {
    let Some(to) = query_param(query, "to") else {
        return json_error(&mut stream, 400, "missing ?to=<ip:port>").await;
    };
    let Some(raw_name) = query_param(query, "name") else {
        return json_error(&mut stream, 400, "missing ?name=<filename>").await;
    };
    let Some(filename) = sanitize_filename(&percent_decode(raw_name)) else {
        return json_error(&mut stream, 400, "invalid filename").await;
    };

    let peer = match resolve_target(&percent_decode(to)) {
        Ok(peer) => peer,
        Err(error) => return json_error(&mut stream, 400, &error.to_string()).await,
    };

    let Some(length) = content_length(headers) else {
        return json_error(&mut stream, 411, "Content-Length required").await;
    };
    if length == 0 {
        return json_error(&mut stream, 400, "empty upload").await;
    }
    if length > MAX_UPLOAD_BYTES {
        return json_error(&mut stream, 413, "upload exceeds the dashboard limit").await;
    }

    let id = ctx.push_transfer(TransferView {
        id: 0,
        direction: "send",
        filename: filename.clone(),
        peer: peer.to_string(),
        total: length,
        transferred: 0,
        status: "uploading",
        error: None,
        checksum: None,
        started: 0,
        finished: None,
    });

    // Keep the original basename inside a unique directory: the engine derives
    // the transfer's display filename from the path it is handed.
    let temp_dir = ctx.upload_dir.join(id.to_string());
    let temp_path = temp_dir.join(&filename);
    if let Err(error) = std::fs::create_dir_all(&temp_dir) {
        ctx.fail_transfer(id, error.to_string());
        return json_error(&mut stream, 500, "could not create upload directory").await;
    }

    let result = spool_body(&mut stream, body_prefix, &temp_path, &ctx, id, length).await;
    if let Err(error) = result {
        let _ = compio::fs::remove_file(&temp_path).await;
        ctx.fail_transfer(id, error.to_string());
        return json_error(&mut stream, 400, &error.to_string()).await;
    }

    let total = ctx
        .transfers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|transfer| transfer.id == id)
        .map(|transfer| transfer.total)
        .unwrap_or(0);

    if total == 0 {
        let _ = compio::fs::remove_file(&temp_path).await;
        ctx.fail_transfer(id, "empty upload");
        return json_error(&mut stream, 400, "empty upload").await;
    }

    let ctx_task = Arc::clone(&ctx);
    let peer_for_task = peer;
    let filename_for_task = filename.clone();
    compio::runtime::spawn(async move {
        run_send(ctx_task, id, temp_path, filename_for_task, peer_for_task).await;
    })
    .detach();

    let body = serde_json::json!({ "ok": true, "id": id }).to_string();
    respond(&mut stream, 202, "application/json", body.as_bytes(), CACHE_NONE, &[]).await
}

/// Streams the request body into `path`, updating the transfer's progress.
async fn spool_body(
    stream: &mut TcpStream,
    prefix: Vec<u8>,
    path: &Path,
    ctx: &Ctx,
    id: u64,
    declared: u64,
) -> std::io::Result<()> {
    // `read_request` consumed the headers and may have buffered body bytes.
    let mut file = compio::fs::File::create(path).await?;

    if prefix.len() as u64 > declared {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "upload exceeded Content-Length",
        ));
    }

    let mut written = 0u64;
    if !prefix.is_empty() {
        let take = prefix.len() as u64;
        file.write_all_at(prefix, 0).await.0?;
        written += take;
        ctx.update_transfer(id, |transfer| transfer.transferred = written);
    }

    let mut buffer = vec![0u8; 64 * 1024];
    while written < declared {
        let read = compio::time::timeout(BODY_STALL_TIMEOUT, stream.read(buffer)).await;
        let Ok(compio::BufResult(result, returned)) = read else {
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "upload stalled"));
        };
        let n = result?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "upload ended before Content-Length",
            ));
        }
        buffer = returned;
        let chunk = buffer[..n].to_vec();
        file.write_all_at(chunk, written).await.0?;
        written += n as u64;
        if written > declared {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "upload exceeded Content-Length",
            ));
        }
        ctx.update_transfer(id, |transfer| transfer.transferred = written);
    }

    ctx.update_transfer(id, |transfer| {
        transfer.total = written;
        transfer.transferred = written;
    });
    Ok(())
}

async fn run_send(ctx: Arc<Ctx>, id: u64, path: PathBuf, filename: String, peer: SocketAddr) {
    ctx.update_transfer(id, |transfer| transfer.status = "active");
    let progress_ctx = Arc::clone(&ctx);
    let sender = hayate::HayateSender::new().target(peer).compress(true);
    let result = sender
        .send_with(
            &path,
            |_stage| Ok(()),
            move |bytes| {
                progress_ctx.update_transfer(id, |transfer| transfer.transferred = bytes);
                Ok(())
            },
        )
        .await;

    match result {
        Ok(outcome) => {
            ctx.finish_transfer(id, Some(outcome.checksum.clone()));
            let elapsed = ctx
                .transfers
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .find(|transfer| transfer.id == id)
                .map_or(0.0, |transfer| {
                    transfer.finished.unwrap_or_else(now_secs).saturating_sub(transfer.started)
                        as f64
                });
            history::record_transfer(
                "send",
                &filename,
                outcome.total_size,
                elapsed,
                &outcome.checksum,
                crate::output::cipher_name(outcome.cipher_id),
                &outcome.peer.to_string(),
                None,
            );
        },
        Err(error) => ctx.fail_transfer(id, error.to_string()),
    }

    let _ = compio::fs::remove_file(&path).await;
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir(parent);
    }
}

// ---------------------------------------------------------------------------
// Receive: dedicated listener thread → shared state
// ---------------------------------------------------------------------------

async fn handle_receive_start(
    mut stream: TcpStream,
    ctx: Arc<Ctx>,
    query: Option<&str>,
    _body: Vec<u8>,
) -> std::io::Result<()> {
    let port = query_param(query, "port")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_RECEIVE_PORT);
    let dir = query_param(query, "dir")
        .map(percent_decode)
        .filter(|value| !value.is_empty())
        .map(|value| expand_tilde(&value))
        .unwrap_or_else(|| ctx.download_dir.clone());

    if std::fs::create_dir_all(&dir).is_err() {
        return json_error(&mut stream, 400, "could not create receive directory").await;
    }

    let stop = {
        let mut receive = ctx.receive.lock().unwrap_or_else(|e| e.into_inner());
        if receive.running {
            None
        } else {
            let stop = Arc::new(AtomicBool::new(false));
            receive.running = true;
            receive.stopping = false;
            receive.port = port;
            receive.dir = dir.display().to_string();
            receive.error = None;
            receive.stop = Some(Arc::clone(&stop));
            Some(stop)
        }
    };
    let Some(stop) = stop else {
        return json_error(&mut stream, 409, "receiver already running").await;
    };

    let thread_ctx = Arc::clone(&ctx);
    std::thread::Builder::new()
        .name("hayate-web-receive".to_owned())
        .spawn(move || receive_thread(thread_ctx, port, dir, stop))
        .map_err(std::io::Error::other)?;

    respond(&mut stream, 202, "application/json", br#"{"ok":true}"#, CACHE_NONE, &[]).await
}

async fn handle_receive_stop(mut stream: TcpStream, ctx: Arc<Ctx>) -> std::io::Result<()> {
    {
        let mut receive = ctx.receive.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(stop) = &receive.stop {
            stop.store(true, Ordering::SeqCst);
        }
        receive.stopping = receive.running;
    }
    respond(&mut stream, 202, "application/json", br#"{"ok":true}"#, CACHE_NONE, &[]).await
}

/// Runs the receive engine on its own thread/runtime, auto-accepting into
/// `dir`.
///
/// A dedicated runtime keeps the dashboard's event loop free: the engine's
/// consent path is synchronous and this listener never needs a prompt.
fn receive_thread(ctx: Arc<Ctx>, port: u16, dir: PathBuf, stop: Arc<AtomicBool>) {
    let runtime = match compio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            mark_receive_failed(&ctx, &format!("runtime: {error}"));
            return;
        },
    };

    runtime.block_on(async move {
        let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
        let builder = hayate::HayateReceiver::new().bind(bind).resume(true).auto_accept(true);
        let listener = match builder.listen().await {
            Ok(listener) => listener,
            Err(error) => {
                mark_receive_failed(&ctx, &format!("bind {bind}: {error}"));
                return;
            },
        };
        let _announcer = listener
            .local_addr()
            .ok()
            .and_then(|addr| hayate::discovery::start_announcer(addr).ok());

        while !stop.load(Ordering::SeqCst) {
            let current: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));
            let stage_ctx = Arc::clone(&ctx);
            let stage_current = Arc::clone(&current);
            let progress_ctx = Arc::clone(&ctx);
            let progress_current = Arc::clone(&current);

            let result = listener
                .try_accept_one(
                    Duration::from_millis(500),
                    &dir,
                    move |stage| {
                        if let hayate::TransferStage::Offer { meta, peer, .. } = &stage {
                            let id = stage_ctx.push_transfer(TransferView {
                                id: 0,
                                direction: "receive",
                                filename: meta.filename.clone(),
                                peer: peer.to_string(),
                                total: meta.total_size,
                                transferred: 0,
                                status: "active",
                                error: None,
                                checksum: None,
                                started: 0,
                                finished: None,
                            });
                            *stage_current.lock().unwrap_or_else(|e| e.into_inner()) = Some(id);
                        }
                        Ok(())
                    },
                    |_meta, _peer| None,
                    move |bytes| {
                        if let Some(id) =
                            *progress_current.lock().unwrap_or_else(|e| e.into_inner())
                        {
                            progress_ctx.update_transfer(id, |transfer| {
                                transfer.transferred = bytes;
                            });
                        }
                        Ok(())
                    },
                )
                .await;

            let id = *current.lock().unwrap_or_else(|e| e.into_inner());
            match result {
                Ok(Some(outcome)) => {
                    if let Some(id) = id {
                        ctx.finish_transfer(id, Some(outcome.checksum.clone()));
                        let elapsed = ctx
                            .transfers
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .iter()
                            .find(|transfer| transfer.id == id)
                            .map_or(0.0, |transfer| {
                                transfer
                                    .finished
                                    .unwrap_or_else(now_secs)
                                    .saturating_sub(transfer.started)
                                    as f64
                            });
                        history::record_transfer(
                            "receive",
                            &outcome.meta.filename,
                            outcome.meta.total_size,
                            elapsed,
                            &outcome.checksum,
                            crate::output::cipher_name(outcome.cipher_id),
                            &outcome.peer.to_string(),
                            Some(outcome.path.display().to_string()),
                        );
                    }
                },
                Ok(None) => {},
                Err(error) if hayate::is_benign_peer_close(&error) => {},
                Err(error) => {
                    if let Some(id) = id {
                        ctx.fail_transfer(id, error.to_string());
                    }
                    if matches!(error, hayate::EngineError::Handshake(_)) {
                        mark_receive_failed(&ctx, &error.to_string());
                        return;
                    }
                },
            }
        }

        let mut receive = ctx.receive.lock().unwrap_or_else(|e| e.into_inner());
        receive.running = false;
        receive.stopping = false;
        receive.stop = None;
    });
}

fn mark_receive_failed(ctx: &Arc<Ctx>, message: &str) {
    let mut receive = ctx.receive.lock().unwrap_or_else(|e| e.into_inner());
    receive.running = false;
    receive.stopping = false;
    receive.error = Some(message.to_owned());
    receive.stop = None;
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

fn header_is(headers: &[(String, String)], name: &str, value: &str) -> bool {
    headers.iter().any(|(key, candidate)| key == name && candidate == value)
}

fn content_length(headers: &[(String, String)]) -> Option<u64> {
    headers.iter().find(|(name, _)| name == "content-length")?.1.parse().ok()
}

fn resolve_target(raw: &str) -> Result<SocketAddr> {
    let mut candidates: Vec<SocketAddr> =
        raw.to_socket_addrs().with_context(|| format!("invalid target \"{raw}\""))?.collect();
    if candidates.is_empty() {
        bail!("could not resolve \"{raw}\"");
    }
    candidates.sort_by_key(|addr| !addr.is_ipv4());
    Ok(candidates[0])
}

/// Reduces a client-supplied name to a safe basename.
fn sanitize_filename(name: &str) -> Option<String> {
    // Treat both separators as path separators on every platform. On Unix a
    // backslash is a legal filename byte, but the same bundle is served by
    // Windows builds where it would escape the intended basename.
    let base = name.rsplit(['/', '\\']).find(|part| !part.is_empty())?;
    let cleaned: String = base.chars().filter(|ch| !ch.is_control()).take(200).collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return None;
    }
    Some(cleaned)
}

/// Minimal percent-decoder for query values (`%20` and `+`).
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            },
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
                if let Some(byte) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                    out.push(byte);
                    index += 3;
                } else {
                    out.push(b'%');
                    index += 1;
                }
            },
            byte => {
                out.push(byte);
                index += 1;
            },
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---------------------------------------------------------------------------
// Static assets + request parsing
// ---------------------------------------------------------------------------

async fn serve_asset(stream: &mut TcpStream, path: &str, ctx: &Ctx) -> std::io::Result<()> {
    let rel = path.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };

    if let Some(file) = UI.get_file(rel) {
        let cache = if rel.starts_with("_app/immutable/") { CACHE_IMMUTABLE } else { CACHE_NONE };
        return respond_asset(stream, rel, file.contents(), cache, ctx).await;
    }

    // SPA fallback: extension-less routes get the shell; hashed assets 404.
    if !rel.contains('.')
        && let Some(file) = UI.get_file("index.html")
    {
        return respond_asset(stream, "index.html", file.contents(), CACHE_NONE, ctx).await;
    }

    respond(stream, 404, "text/plain; charset=utf-8", b"not found\n", CACHE_NONE, &[]).await
}

async fn respond_asset(
    stream: &mut TcpStream,
    rel: &str,
    body: &[u8],
    cache: &str,
    ctx: &Ctx,
) -> std::io::Result<()> {
    if rel == "index.html" {
        let cookie = format!("{TOKEN_COOKIE}={}; HttpOnly; SameSite=Strict; Path=/", ctx.token);
        return respond(
            stream,
            200,
            "text/html; charset=utf-8",
            body,
            cache,
            &[("Set-Cookie", cookie.as_str())],
        )
        .await;
    }
    respond(stream, 200, mime_for(rel), body, cache, &[]).await
}

fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "woff2" => "font/woff2",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

async fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<(Request, Vec<u8>)>> {
    let mut buf = Vec::new();
    let header_end = loop {
        let chunk = vec![0u8; 4096];
        let read = compio::time::timeout(REQUEST_TIMEOUT, stream.read(chunk)).await;
        let Ok(compio::BufResult(result, chunk)) = read else {
            return Ok(None);
        };
        let n = result?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(index) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
        if buf.len() > MAX_REQUEST_BYTES {
            return Ok(None);
        }
    };
    let body_prefix = buf[header_end..].to_vec();

    let text = String::from_utf8_lossy(&buf[..header_end]);
    let mut lines = text.split("\r\n");
    let Some(request_line) = lines.next() else {
        return Ok(None);
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    if method.is_empty() || target.is_empty() {
        return Ok(None);
    }

    let headers = lines
        .take_while(|line| !line.is_empty())
        .filter_map(|line| {
            line.split_once(':')
                .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        })
        .collect();

    Ok(Some((Request { method, target, headers }, body_prefix)))
}

fn split_target(target: &str) -> (&str, Option<&str>) {
    match target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (target, None),
    }
}

/// Renders a path relative to the user's home as `~/…` for display.
fn tildify(path: &Path) -> String {
    let Some(home) = dirs::home_dir() else {
        return path.display().to_string();
    };
    match path.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Expands a leading `~/` in user-supplied paths.
fn expand_tilde(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(value)
}

fn query_param<'a>(query: Option<&'a str>, name: &str) -> Option<&'a str> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(key, value)| (key == name).then_some(value))
}

/// Accepts the token from `?t=`, the session cookie, or a bearer header.
fn authorized(headers: &[(String, String)], query: Option<&str>, token: &str) -> bool {
    if query_param(query, "t").is_some_and(|value| value == token) {
        return true;
    }
    headers.iter().any(|(name, value)| match name.as_str() {
        "cookie" => {
            value.split(';').any(|cookie| cookie.trim() == format!("{TOKEN_COOKIE}={token}"))
        },
        "authorization" => value == &format!("Bearer {token}"),
        _ => false,
    })
}

/// Rejects cross-origin probes that reuse this port via DNS rebinding.
fn host_allowed(headers: &[(String, String)], listen: SocketAddr) -> bool {
    let Some((_, host)) = headers.iter().find(|(name, _)| name == "host") else {
        // HTTP/1.0 clients may omit Host; the token still gates access.
        return true;
    };
    let port = listen.port();
    let allowed =
        [format!("127.0.0.1:{port}"), format!("localhost:{port}"), format!("[::1]:{port}")];
    allowed.iter().any(|candidate| host.eq_ignore_ascii_case(candidate))
}

async fn plain(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    respond(stream, status, "text/plain; charset=utf-8", body.as_bytes(), CACHE_NONE, &[]).await
}

async fn json_error(stream: &mut TcpStream, status: u16, message: &str) -> std::io::Result<()> {
    let body = serde_json::json!({ "ok": false, "error": message }).to_string();
    respond(stream, status, "application/json", body.as_bytes(), CACHE_NONE, &[]).await
}

async fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    cache: &str,
    extra: &[(&str, &str)],
) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: {cache}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: no-referrer\r\n\
         Content-Security-Policy: default-src 'self'; script-src 'self' 'unsafe-inline'; \
         style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; \
         connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'none'\r\n",
        reason(status),
        body.len()
    );
    for (name, value) in extra {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");

    stream.write_all(head.into_bytes()).await.0.map_err(std::io::Error::other)?;
    stream.write_all(body.to_vec()).await.0.map_err(std::io::Error::other)?;
    Ok(())
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Payload Too Large",
        _ => "OK",
    }
}

async fn sse(mut stream: TcpStream, ctx: Arc<Ctx>) -> std::io::Result<()> {
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\
                Connection: keep-alive\r\nX-Content-Type-Options: nosniff\r\n\r\n";
    stream.write_all(head.as_bytes().to_vec()).await.0.map_err(std::io::Error::other)?;

    loop {
        let payload = serde_json::json!({
            "status": status_json(&ctx),
            "peers": peers_json(&ctx),
            "transfers": transfers_json(&ctx),
            "receive": receive_json(&ctx),
        })
        .to_string();
        let frame = format!("event: state\ndata: {payload}\n\n");
        if stream.write_all(frame.into_bytes()).await.0.is_err() {
            break;
        }
        compio::time::sleep(SSE_INTERVAL).await;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// JSON payloads
// ---------------------------------------------------------------------------

fn status_json(ctx: &Ctx) -> serde_json::Value {
    let peers = ctx.peers.lock().unwrap_or_else(|e| e.into_inner()).peers.len();
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "hostname": ctx.hostname,
        "os": std::env::consts::OS,
        "address": ctx.listen.to_string(),
        "uptime_secs": ctx.started.elapsed().as_secs(),
        "peers": peers,
        "mode": "full",
    })
}

fn peers_json(ctx: &Ctx) -> serde_json::Value {
    let state = ctx.peers.lock().unwrap_or_else(|e| e.into_inner());
    serde_json::json!({
        "updated_secs_ago": state.updated.map(|at| at.elapsed().as_secs()),
        "peers": state.peers,
    })
}

fn transfers_json(ctx: &Ctx) -> serde_json::Value {
    let transfers = ctx.transfers.lock().unwrap_or_else(|e| e.into_inner());
    serde_json::json!({ "transfers": *transfers })
}

fn receive_json(ctx: &Ctx) -> ReceiveView {
    let receive = ctx.receive.lock().unwrap_or_else(|e| e.into_inner());
    ReceiveView {
        running: receive.running,
        stopping: receive.stopping,
        port: receive.port,
        dir: tildify(Path::new(&receive.dir)),
        error: receive.error.clone(),
    }
}

#[derive(Serialize)]
struct ReceiveView {
    running: bool,
    stopping: bool,
    port: u16,
    dir: String,
    error: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::{
        authorized,
        host_allowed,
        mime_for,
        percent_decode,
        query_param,
        sanitize_filename,
        split_target,
    };

    const LISTEN: &str = "127.0.0.1:8787";

    #[test]
    fn token_accepted_from_query_cookie_and_bearer() {
        let query = Some("t=abc123");
        assert!(authorized(&[], query, "abc123"));
        assert!(!authorized(&[], Some("t=wrong"), "abc123"));

        let cookie = vec![("cookie".to_owned(), "a=1; hayate_web=abc123".to_owned())];
        assert!(authorized(&cookie, None, "abc123"));

        let bearer = vec![("authorization".to_owned(), "Bearer abc123".to_owned())];
        assert!(authorized(&bearer, None, "abc123"));
        assert!(!authorized(&bearer, None, "other"));
    }

    #[test]
    fn host_header_must_match_loopback() {
        let listen: SocketAddr = LISTEN.parse().unwrap();
        assert!(host_allowed(&[], listen));
        let ok = vec![("host".to_owned(), "localhost:8787".to_owned())];
        assert!(host_allowed(&ok, listen));
        let bad = vec![("host".to_owned(), "evil.example:8787".to_owned())];
        assert!(!host_allowed(&bad, listen));
    }

    #[test]
    fn query_and_path_parsing() {
        assert_eq!(split_target("/api/peers?timeout=3"), ("/api/peers", Some("timeout=3")));
        assert_eq!(query_param(Some("a=1&limit=5"), "limit"), Some("5"));
        assert_eq!(query_param(Some("a=1"), "limit"), None);
        assert_eq!(percent_decode("hello%20world+again"), "hello world again");
    }

    #[test]
    fn filenames_are_reduced_to_a_safe_basename() {
        assert_eq!(sanitize_filename("../../etc/passwd").as_deref(), Some("passwd"));
        assert_eq!(sanitize_filename("notes.txt").as_deref(), Some("notes.txt"));
        assert_eq!(sanitize_filename(".."), None);
        assert_eq!(sanitize_filename(""), None);
        assert_eq!(sanitize_filename("a\\b.txt").as_deref(), Some("b.txt"));
        assert_eq!(sanitize_filename("a/b\\c.txt").as_deref(), Some("c.txt"));
        assert_eq!(sanitize_filename("dir/").as_deref(), Some("dir"));
        assert_eq!(sanitize_filename("C:\\Users\\me\\file.txt").as_deref(), Some("file.txt"));
    }

    #[test]
    fn mime_types_cover_sveltekit_assets() {
        assert_eq!(mime_for("index.html"), "text/html; charset=utf-8");
        assert_eq!(mime_for("_app/immutable/entry/start.abc.js"), "text/javascript; charset=utf-8");
        assert_eq!(mime_for("_app/immutable/assets/0.abc.css"), "text/css; charset=utf-8");
        assert_eq!(mime_for("favicon.svg"), "image/svg+xml");
        assert_eq!(mime_for("font.woff2"), "font/woff2");
        assert_eq!(mime_for("unknown.bin"), "application/octet-stream");
    }
}
