//! LAN peer discovery with mDNS (RFC 6762) primary + UDP broadcast fallback.
//!
//! ## Architecture
//!
//! The sender **broadcasts** its presence via two channels simultaneously:
//! 1. **mDNS** — registers a `_hayate._udp.local.` service. Works on Android,
//!    iOS, macOS, Linux, and Windows. No admin privileges needed.
//! 2. **UDP broadcast** — sends `HAYATE_PEER:v2:…` to `255.255.255.255:50002`
//!    and `127.0.0.1:50002` every 800ms. Fast-path for legacy / restricted
//!    networks.
//!
//! The receiver **listens** on both channels and returns whichever peer
//! announces first.
//!
//! ## Security note
//!
//! Discovery announcements are **unauthenticated**: any host on the local
//! network can send a valid UDP broadcast or mDNS response. The pairing
//! passphrase is what protects the actual transfer; discovery only tells the
//! receiver where to initiate the QUIC connection. Treat discovered addresses
//! as untrusted until the X25519 + passphrase handshake succeeds.

use std::collections::HashSet;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// mDNS service type for Hayate discovery.
const MDNS_SERVICE_TYPE: &str = "_hayate._udp.local.";
/// UDP discovery port.
const UDP_DISCOVERY_PORT: u16 = 50002;

/// UDP port used for active "who is here?" discovery queries.
///
/// Kept separate from `UDP_DISCOVERY_PORT` so a receiver can answer queries
/// while a pairing session is listening for sender broadcasts on the other
/// port.
pub const DISCOVERY_QUERY_PORT: u16 = 50003;

/// Query packet sent by `hayate discover`.
const QUERY_MAGIC: &str = "HAYATE_DISCOVER:v1";

/// Reply packet sent by an announcing receiver.
const REPLY_MAGIC: &str = "HAYATE_HERE";

/// Result of a discovery probe containing peer metadata.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DiscoveredPeer {
    /// Human-readable name of the peer.
    pub name: String,
    /// Socket address of the peer's QUIC listener.
    pub addr: SocketAddr,
    /// Operating system reported by the peer.
    pub os: String,
    /// Round-trip time of the probe in milliseconds.
    pub rtt_ms: Option<f64>,
    /// Hayate version string reported by the peer, when known.
    pub version: String,
}

/// Computes SHA-256 of the phrase and returns the first 4 bytes as a hex
/// string.
///
/// Used to derive a short, phrase-bound channel ID for mDNS TXT records and
/// UDP broadcast packets. `ring` is already in the dep graph for AEAD, so no
/// extra crate is needed.
#[must_use]
pub fn derive_channel_id(phrase: &str) -> String {
    let result = ring::digest::digest(&ring::digest::SHA256, phrase.as_bytes());
    crate::hex_encode(&result.as_ref()[..4])
}

// ─────────────────────────────────────────────────────────────────────────────
// Broadcaster (sender side)
// ─────────────────────────────────────────────────────────────────────────────

/// RAII guard that shuts down both the mDNS service and the UDP broadcast
/// loop when dropped.
pub struct BroadcasterGuard {
    mdns_handle: Option<mdns_sd::ServiceDaemon>,
    cancel_tx: Option<flume::Sender<()>>,
}

impl std::fmt::Debug for BroadcasterGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BroadcasterGuard")
            .field("mdns_handle", &self.mdns_handle.is_some())
            .field("cancel_tx", &self.cancel_tx.is_some())
            .finish()
    }
}

impl BroadcasterGuard {
    /// Creates a new guard from a cancel sender and mDNS daemon.
    ///
    /// Prefer [`start_broadcaster_hybrid`] — it creates the channel pair
    /// and manages the lifecycle automatically.
    #[must_use]
    pub(crate) fn new(cancel_tx: flume::Sender<()>, mdns: mdns_sd::ServiceDaemon) -> Self {
        Self { cancel_tx: Some(cancel_tx), mdns_handle: Some(mdns) }
    }
}

impl Drop for BroadcasterGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
        if let Some(mdns) = self.mdns_handle.take() {
            let _ = mdns.shutdown();
        }
    }
}

/// Starts both mDNS and UDP broadcast advertisement.
///
/// Creates an mDNS daemon, registers a `_hayate._udp.local.` service with
/// TXT records encoding the channel ID, OS, and QUIC port, then spawns the
/// UDP broadcast loop in a detached compio task.
///
/// ## Security
///
/// UDP broadcast packets are sent in the clear and can be observed or spoofed
/// by any host on the local network. They carry no secret material; the
/// channel ID is public and the actual transfer is authenticated by the
/// X25519 + passphrase handshake. Do not rely on discovery alone for peer
/// identity.
///
/// Returns a [`BroadcasterGuard`] that shuts down both when dropped.
pub fn start_broadcaster_hybrid(
    channel_id: &str,
    port: u16,
    os_name: &str,
) -> Result<BroadcasterGuard, io::Error> {
    let (cancel_tx, cancel_rx) = flume::bounded::<()>(1);

    let mdns = mdns_sd::ServiceDaemon::new().map_err(io::Error::other)?;

    let instance_name = format!("hayate-{channel_id}");
    let host_name = format!("hayate-{channel_id}.local.");

    let port_str = port.to_string();
    let name = local_hostname();
    let ip_str = crate::local_addr::primary_local_ipv4()
        .map_or_else(|| "127.0.0.1".to_owned(), |ip| ip.to_string());
    let txt_props: &[(&str, &str)] = &[
        ("chid", channel_id),
        ("role", "sender"),
        ("name", name.as_str()),
        ("os", os_name),
        ("port", port_str.as_str()),
        ("ip", ip_str.as_str()),
        ("version", env!("CARGO_PKG_VERSION")),
    ];

    let service = mdns_sd::ServiceInfo::new(
        MDNS_SERVICE_TYPE,
        &instance_name,
        &host_name,
        ip_str.as_str(),
        port,
        txt_props,
    )
    .map_err(io::Error::other)?;

    mdns.register(service).map_err(io::Error::other)?;

    let cid = channel_id.to_owned();
    let os = os_name.to_owned();
    let mdns_clone = mdns.clone();
    compio::runtime::spawn(async move {
        let _ = udp_broadcast_loop(&cid, port, &os, cancel_rx).await;
        let _ = mdns_clone.shutdown();
    })
    .detach();

    Ok(BroadcasterGuard::new(cancel_tx, mdns))
}

/// Internal UDP broadcast loop.
async fn udp_broadcast_loop(
    channel_id: &str,
    port: u16,
    os: &str,
    cancel_rx: flume::Receiver<()>,
) -> Result<(), io::Error> {
    let socket = compio::net::UdpSocket::bind("0.0.0.0:0").await?;
    socket.set_broadcast(true)?;

    let msg = format!("HAYATE_PEER:v2:{channel_id}:{os}:{port}");
    let msg_bytes = msg.into_bytes();
    let target = SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), UDP_DISCOVERY_PORT);
    let loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), UDP_DISCOVERY_PORT);

    loop {
        let _ = socket.send_to(msg_bytes.clone(), target).await;
        let _ = socket.send_to(msg_bytes.clone(), loopback).await;

        let sleep_fut = compio::time::sleep(Duration::from_millis(800));
        let cancel_fut = cancel_rx.recv_async();
        let sleep_pinned = std::pin::pin!(sleep_fut);
        let cancel_pinned = std::pin::pin!(cancel_fut);

        if let futures_util::future::Either::Right(_) =
            futures_util::future::select(sleep_pinned, cancel_pinned).await
        {
            break;
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Receiver announcer + active discovery query
// ─────────────────────────────────────────────────────────────────────────────

/// Best-effort human-readable host name for discovery announcements.
///
/// `HAYATE_NAME` overrides everything; otherwise `HOSTNAME`, `COMPUTERNAME`,
/// and `/etc/hostname` are consulted in that order.
#[must_use]
pub fn local_hostname() -> String {
    let raw = std::env::var("HAYATE_NAME")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .or_else(|| {
            // macOS has no /etc/hostname; the `hostname` utility exists on
            // every supported platform and returns immediately.
            std::process::Command::new("hostname")
                .output()
                .ok()
                .filter(|out| out.status.success())
                .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        })
        .unwrap_or_default();
    let cleaned: String =
        raw.trim().chars().filter(|c| !matches!(c, ':' | '\n' | '\r' | '\t')).take(64).collect();
    if cleaned.is_empty() { "Hayate Peer".to_owned() } else { cleaned }
}

/// Keeps a receiver visible to `hayate discover` while it listens.
///
/// Registers an mDNS `_hayate._udp.local.` service (with `role=receiver`) and
/// answers UDP discovery queries on [`DISCOVERY_QUERY_PORT`]. Both channels are
/// best-effort: a failure to advertise never prevents receiving, and dropping
/// the guard tears both down.
pub struct Announcer {
    cancel: Arc<AtomicBool>,
    mdns: Option<mdns_sd::ServiceDaemon>,
    responder: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for Announcer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Announcer")
            .field("cancel", &self.cancel)
            .field("mdns", &self.mdns.is_some())
            .field("responder", &self.responder.is_some())
            .finish()
    }
}

impl Drop for Announcer {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(mdns) = self.mdns.take() {
            let _ = mdns.shutdown();
        }
        if let Some(handle) = self.responder.take() {
            let _ = handle.join();
        }
    }
}

/// Starts advertising a receiver that listens on `listen`.
///
/// The mDNS record advertises `listen`'s address when it is specific, or the
/// primary LAN address when bound to a wildcard.
///
/// # Errors
///
/// Returns [`io::Error`] only when the announcement could not be started at
/// all; the mDNS and UDP channels degrade independently and silently.
pub fn start_announcer(listen: SocketAddr) -> Result<Announcer, io::Error> {
    let name = local_hostname();
    let os = std::env::consts::OS.to_owned();
    let cancel = Arc::new(AtomicBool::new(false));

    let quic_port = listen.port();
    let port_str = quic_port.to_string();
    // Advertise an address the peer can actually reach: the bound address when
    // it is specific, otherwise the primary LAN address.
    let ip_str = if listen.ip().is_unspecified() {
        crate::local_addr::primary_local_ipv4()
            .map_or_else(|| "127.0.0.1".to_owned(), |ip| ip.to_string())
    } else {
        listen.ip().to_string()
    };
    let instance = format!("hayate-{quic_port}");
    let host_name = format!("hayate-{quic_port}.local.");
    let txt_props: &[(&str, &str)] = &[
        ("role", "receiver"),
        ("name", name.as_str()),
        ("os", os.as_str()),
        ("port", port_str.as_str()),
        ("ip", ip_str.as_str()),
        ("version", env!("CARGO_PKG_VERSION")),
    ];

    let mdns = mdns_sd::ServiceDaemon::new().ok().and_then(|daemon| {
        let service = mdns_sd::ServiceInfo::new(
            MDNS_SERVICE_TYPE,
            &instance,
            &host_name,
            ip_str.as_str(),
            quic_port,
            txt_props,
        )
        .ok()?;
        daemon.register(service).ok()?;
        Some(daemon)
    });

    let responder = {
        let cancel = Arc::clone(&cancel);
        let name = name.clone();
        let os = os.clone();
        let ip = ip_str.clone();
        std::thread::spawn(move || responder_loop(&name, &os, &ip, quic_port, &cancel))
    };

    Ok(Announcer { cancel, mdns, responder: Some(responder) })
}

/// Answers `HAYATE_DISCOVER:v1` queries with a unicast `HAYATE_HERE` reply.
fn responder_loop(name: &str, os: &str, ip: &str, quic_port: u16, cancel: &AtomicBool) {
    let Ok(socket) = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DISCOVERY_QUERY_PORT))
    else {
        return;
    };
    let _ = socket.set_read_timeout(Some(Duration::from_millis(300)));
    // v2 carries the address the receiver is actually bound to, so a client
    // that reached us by broadcast does not mistake our LAN source address for
    // a reachable listener when we are bound to loopback.
    let reply =
        format!("{REPLY_MAGIC}:v2:{name}:{os}:{quic_port}:{ip}:{}", env!("CARGO_PKG_VERSION"));
    let mut buf = [0u8; 512];

    while !cancel.load(Ordering::Relaxed) {
        match socket.recv_from(&mut buf) {
            Ok((n, src)) => {
                let is_query =
                    std::str::from_utf8(&buf[..n]).is_ok_and(|text| text.trim() == QUERY_MAGIC);
                if is_query {
                    let _ = socket.send_to(reply.as_bytes(), src);
                }
            },
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
            },
            Err(_) => break,
        }
    }
}

/// Actively asks every local network for Hayate receivers.
///
/// Broadcasts `QUERY_MAGIC` to the all-ones broadcast address of every local
/// interface (plus loopback for same-machine peers) and collects
/// `REPLY_MAGIC` replies until `timeout` elapses. This costs two packets
/// instead of a full subnet sweep and works on networks that block mDNS.
///
/// Discovery is unauthenticated: treat replies as hints and let the pairing
/// handshake authenticate the peer.
///
/// # Errors
///
/// Returns [`io::Error`] when the local UDP socket cannot be created.
pub fn query(timeout: Duration) -> Result<Vec<DiscoveredPeer>, io::Error> {
    let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.set_broadcast(true)?;
    socket.set_read_timeout(Some(Duration::from_millis(150)))?;

    let mut targets = vec![
        SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), DISCOVERY_QUERY_PORT),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DISCOVERY_QUERY_PORT),
    ];
    for net in crate::local_addr::local_networks() {
        let target = SocketAddr::new(IpAddr::V4(net.broadcast()), DISCOVERY_QUERY_PORT);
        if !targets.contains(&target) {
            targets.push(target);
        }
    }

    // Two rounds: UDP is lossy, and the first packet may wake a radio.
    for _ in 0..2 {
        for target in &targets {
            let _ = socket.send_to(QUERY_MAGIC.as_bytes(), target);
        }
    }

    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 1024];
    let mut seen = HashSet::new();
    let mut peers = Vec::new();

    while Instant::now() < deadline {
        match socket.recv_from(&mut buf) {
            Ok((n, src)) => {
                if let Ok(text) = std::str::from_utf8(&buf[..n])
                    && let Some(peer) = parse_reply(text, src)
                    && seen.insert(peer.addr)
                {
                    peers.push(peer);
                }
            },
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
            },
            Err(_) => break,
        }
    }

    Ok(peers)
}

/// Parses `HAYATE_HERE:v2:<name>:<os>:<port>:<ip>:<version>` into a peer.
///
/// v1 replies (no `ip` field) are still accepted and fall back to the packet's
/// source address.
fn parse_reply(text: &str, src: SocketAddr) -> Option<DiscoveredPeer> {
    let mut parts = text.trim().splitn(7, ':');
    if parts.next()? != "HAYATE_HERE" {
        return None;
    }
    let tag = parts.next()?;
    let name = parts.next()?.to_owned();
    let os = parts.next()?.to_owned();
    let port: u16 = parts.next()?.parse().ok()?;
    let (ip, version) = match tag {
        "v2" => {
            let ip: Ipv4Addr = parts.next()?.parse().ok()?;
            let version = parts.next().unwrap_or("unknown").to_owned();
            (ip, version)
        },
        "v1" => {
            let ip = match src.ip() {
                IpAddr::V4(ip) => ip,
                IpAddr::V6(_) => return None,
            };
            (ip, parts.next().unwrap_or("unknown").to_owned())
        },
        _ => return None,
    };
    Some(DiscoveredPeer {
        name,
        addr: SocketAddr::new(IpAddr::V4(ip), port),
        os,
        rtt_ms: None,
        version,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Listener / browser (receiver side)
// ─────────────────────────────────────────────────────────────────────────────

/// Hybrid listener: browses mDNS services and listens on UDP simultaneously.
/// Returns the first matching peer found within `timeout`.
///
/// If `target_phrase` is `Some`, only peers whose channel ID matches the
/// SHA-256 prefix of the phrase are accepted. If `None`, the first
/// discovered peer is returned regardless.
///
/// ## Security
///
/// Discovery announcements are unauthenticated and can be spoofed on the
/// local network. The returned address should be treated as a hint; the
/// pairing handshake is what authenticates the peer and must reject any
/// host that does not know the passphrase.
pub fn listen_for_broadcast(
    target_phrase: Option<&str>,
    timeout: Duration,
) -> Result<Option<(String, SocketAddr, String)>, io::Error> {
    let target_channel_id = target_phrase.map(derive_channel_id);

    let (found_tx, found_rx) = flume::bounded::<(String, SocketAddr, String)>(1);

    // mDNS browser (background thread)
    let target_cid_mdns = target_channel_id.clone();
    let found_tx_mdns = found_tx.clone();
    let mdns_task = std::thread::spawn(move || {
        let Ok(mdns) = mdns_sd::ServiceDaemon::new() else {
            return;
        };
        let Ok(receiver) = mdns.browse(MDNS_SERVICE_TYPE) else {
            let _ = mdns.shutdown();
            return;
        };

        let deadline = Instant::now() + timeout;
        while let Ok(event) = receiver.recv_timeout(Duration::from_millis(200)) {
            if Instant::now() > deadline {
                break;
            }
            if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
                let advertised_ip = info
                    .get_property_val_str("ip")
                    .and_then(|value| value.parse::<Ipv4Addr>().ok());
                let addresses: Vec<Ipv4Addr> = match advertised_ip {
                    Some(ip) => vec![ip],
                    None => info.get_addresses_v4().into_iter().collect(),
                };
                for addr in addresses {
                    let remote_chid =
                        info.get_property_val_str("chid").unwrap_or_default().to_owned();
                    let remote_os = info.get_property_val_str("os").unwrap_or("unknown").to_owned();
                    let remote_port = info.get_port();

                    let matches = match &target_cid_mdns {
                        Some(expected) => remote_chid == *expected,
                        None => true,
                    };
                    if matches {
                        let peer_addr = SocketAddr::new(IpAddr::V4(addr), remote_port);
                        let _ = found_tx_mdns.send((
                            format!("mDNS:{remote_chid}"),
                            peer_addr,
                            remote_os,
                        ));
                        let _ = mdns.shutdown();
                        return;
                    }
                }
            }
        }
        let _ = mdns.shutdown();
    });

    // UDP listener (background thread)
    let target_cid_udp = target_channel_id.clone();
    let found_tx_udp = found_tx;
    let udp_task = std::thread::spawn(move || -> Result<(), io::Error> {
        let socket = bind_udp_listener()?;
        let mut buf = [0u8; 1024];
        let deadline = Instant::now() + timeout;

        while Instant::now() <= deadline {
            match socket.recv_from(&mut buf) {
                Ok((n, src_addr)) => {
                    let data = &buf[..n];
                    if let Ok(text) = std::str::from_utf8(data)
                        && let Some(result) =
                            parse_udp_packet(text, target_cid_udp.as_deref(), src_addr)
                    {
                        let _ = found_tx_udp.send(result);
                        return Ok(());
                    }
                },
                Err(ref e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::TimedOut => {},
                Err(_) => break,
            }
        }
        Ok(())
    });

    let adjusted_timeout = timeout.checked_add(Duration::from_secs(2)).unwrap_or(timeout);
    if let Ok(result) = found_rx.recv_timeout(adjusted_timeout) {
        let _ = mdns_task.join();
        let _ = udp_task.join();
        Ok(Some(result))
    } else {
        let _ = mdns_task.join();
        let _ = udp_task.join();
        Ok(None)
    }
}

/// Scans the local network for Hayate peers using mDNS and UDP broadcast.
///
/// Unlike [`listen_for_broadcast`], this does not filter by a passphrase and
/// returns every peer discovered within `timeout`. It is the backing call for
/// the `hayate discover` command: rather than probing every host on a subnet
/// with a QUIC handshake, it passively collects the `HAYATE_PEER` UDP
/// broadcasts and `_hayate._udp.local.` mDNS advertisements that Hayate peers
/// emit while waiting to pair.
///
/// Peers are de-duplicated by socket address (a single peer advertises on both
/// channels) and returned in discovery order.
pub fn scan(timeout: Duration) -> Result<Vec<DiscoveredPeer>, io::Error> {
    // Unbounded: a bounded channel can deadlock `join()` below — a worker
    // blocked in `send` on a full channel never observes its deadline once the
    // main loop stops draining. Peer volume here is naturally rate-limited by
    // the network, so unbounded buffering is safe.
    let (found_tx, found_rx) = flume::unbounded::<DiscoveredPeer>();

    // mDNS browser (background thread).
    let found_tx_mdns = found_tx.clone();
    let mdns_task = std::thread::spawn(move || {
        let Ok(mdns) = mdns_sd::ServiceDaemon::new() else {
            return;
        };
        let Ok(receiver) = mdns.browse(MDNS_SERVICE_TYPE) else {
            let _ = mdns.shutdown();
            return;
        };
        let deadline = Instant::now() + timeout;
        while let Ok(event) = receiver.recv_timeout(Duration::from_millis(200)) {
            if Instant::now() > deadline {
                break;
            }
            if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
                let remote_chid = info.get_property_val_str("chid").unwrap_or_default().to_owned();
                let remote_os = info.get_property_val_str("os").unwrap_or("unknown").to_owned();
                let remote_name = info.get_property_val_str("name").unwrap_or_default().to_owned();
                let remote_version =
                    info.get_property_val_str("version").unwrap_or("unknown").to_owned();
                let remote_port = info.get_port();
                let display = if remote_name.is_empty() {
                    format!("mDNS:{remote_chid}")
                } else {
                    remote_name
                };
                let advertised_ip = info
                    .get_property_val_str("ip")
                    .and_then(|value| value.parse::<Ipv4Addr>().ok());
                let addresses: Vec<Ipv4Addr> = match advertised_ip {
                    Some(ip) => vec![ip],
                    None => info.get_addresses_v4().into_iter().collect(),
                };
                for addr in addresses {
                    let _ = found_tx_mdns.send(DiscoveredPeer {
                        name: display.clone(),
                        addr: SocketAddr::new(IpAddr::V4(addr), remote_port),
                        os: remote_os.clone(),
                        rtt_ms: None,
                        version: remote_version.clone(),
                    });
                }
            }
        }
        let _ = mdns.shutdown();
    });

    // UDP listener (background thread).
    let found_tx_udp = found_tx;
    let udp_task = std::thread::spawn(move || -> Result<(), io::Error> {
        let socket = bind_udp_listener()?;
        let mut buf = [0u8; 1024];
        let deadline = Instant::now() + timeout;
        while Instant::now() <= deadline {
            match socket.recv_from(&mut buf) {
                Ok((n, src_addr)) => {
                    let data = &buf[..n];
                    if let Ok(text) = std::str::from_utf8(data)
                        && let Some(result) = parse_udp_packet(text, None, src_addr)
                    {
                        let (name, addr, os) = result;
                        let _ = found_tx_udp.send(DiscoveredPeer {
                            name,
                            addr,
                            os,
                            rtt_ms: None,
                            version: "unknown".to_owned(),
                        });
                    }
                },
                Err(ref e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::TimedOut => {},
                Err(_) => break,
            }
        }
        Ok(())
    });

    let deadline = Instant::now() + timeout;
    let mut seen = HashSet::new();
    let mut peers = Vec::new();
    while Instant::now() <= deadline {
        match found_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(peer) => {
                if seen.insert(peer.addr) {
                    peers.push(peer);
                }
            },
            Err(flume::RecvTimeoutError::Disconnected) => break,
            Err(flume::RecvTimeoutError::Timeout) => {},
        }
    }

    let _ = mdns_task.join();
    let _ = udp_task.join();
    Ok(peers)
}

/// Binds the shared UDP discovery listener socket.
///
/// Discovery workers run on their own threads and join after a deadline, so the
/// socket needs a read timeout; without one `recv_from` would block forever and
/// the join would hang past the scan deadline. `SO_REUSEPORT` is Unix-only, and
/// a single socket per process is enough on Windows.
fn bind_udp_listener() -> Result<std::net::UdpSocket, io::Error> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    socket.set_reuse_address(true)?;
    #[cfg(not(windows))]
    socket.set_reuse_port(true)?;

    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), UDP_DISCOVERY_PORT);
    socket.bind(&socket2::SockAddr::from(listen_addr))?;
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;
    Ok(socket.into())
}

/// Parses a UDP discovery packet: `HAYATE_PEER:v2:<ChannelID>:<OS>:<Port>`.
fn parse_udp_packet(
    text: &str,
    target_channel_id: Option<&str>,
    src_addr: SocketAddr,
) -> Option<(String, SocketAddr, String)> {
    let mut parts = text.split(':');
    if parts.next()? != "HAYATE_PEER" {
        return None;
    }
    if parts.next()? != "v2" {
        return None;
    }
    let channel_id = parts.next()?.to_owned();
    let os = parts.next()?.to_owned();
    let port_str = parts.next()?.to_owned();

    let matches = match target_channel_id {
        Some(expected) => channel_id == *expected,
        None => true,
    };
    if matches {
        let port = port_str.parse::<u16>().ok()?;
        let peer_addr = SocketAddr::new(src_addr.ip(), port);
        Some((format!("UDP:{channel_id}"), peer_addr, os))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::derive_channel_id;

    // ── derive_channel_id
    // ─────────────────────────────────────────────────────
    //
    // derive_channel_id is the public integration point for hex_encode (change
    // 1): it takes the first 4 bytes of a SHA-256 digest and hex-encodes them.
    //
    // The 4-byte slice means we're encoding bytes in the range 0x00-0xff.
    // A leading-zero regression (0x0a → "a") would shorten the output from
    // 8 to 7 (or fewer) characters, breaking peer matching silently: both
    // sender and receiver would independently compute the ID, but if one
    // runs the old code and one the new, they would disagree on any phrase
    // whose SHA-256 starts with a byte < 0x10.

    /// Output must always be exactly 8 lowercase hex characters (4 bytes × 2).
    /// A 7-character output would indicate a dropped leading zero.
    #[test]
    fn derive_channel_id_is_exactly_8_lowercase_hex_chars() {
        // Use many phrases to maximise the chance of hitting a sub-0x10 first
        // byte.
        for phrase in ["", "a", "abc", "apple-bravo-charlie", "x".repeat(200).as_str()] {
            let id = derive_channel_id(phrase);
            assert_eq!(id.len(), 8, "derive_channel_id({phrase:?}) must be 8 chars, got {id:?}");
            assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "must be valid hex: {id}");
            assert_eq!(id, id.to_lowercase(), "must be lowercase: {id}");
        }
    }

    /// The function must be deterministic: same phrase → same ID, always.
    /// Non-determinism would break pairing entirely.
    #[test]
    fn derive_channel_id_is_deterministic() {
        let phrase = "test-phrase";
        assert_eq!(derive_channel_id(phrase), derive_channel_id(phrase));
    }

    /// Different phrases must (with overwhelming probability) yield different
    /// IDs. A collision here would allow impersonation attacks during
    /// pairing.
    #[test]
    fn derive_channel_id_different_phrases_produce_different_ids() {
        assert_ne!(derive_channel_id("alpha"), derive_channel_id("beta"));
        assert_ne!(derive_channel_id("foo"), derive_channel_id("bar"));
    }

    /// Known-vector test: SHA-256("") = e3b0c442…; first 4 bytes = [0xe3, 0xb0,
    /// 0xc4, 0x42]. The expected channel ID is "e3b0c442".
    ///
    /// If `hex_encode` drops leading zeros this specific vector would still
    /// pass (all bytes ≥ 0x10), but it locks down the ring::digest integration
    /// and confirms the `[..4]` slice boundary is correct.
    #[test]
    fn derive_channel_id_known_vector_empty_phrase() {
        assert_eq!(derive_channel_id(""), "e3b0c442");
    }

    #[test]
    fn parse_reply_reads_port_and_version() {
        let src: std::net::SocketAddr = "192.168.1.9:41234".parse().unwrap();
        let peer = super::parse_reply("HAYATE_HERE:v1:studio-mac:macos:50001:6.1.0", src)
            .expect("valid reply");
        assert_eq!(peer.name, "studio-mac");
        assert_eq!(peer.os, "macos");
        assert_eq!(peer.version, "6.1.0");
        assert_eq!(peer.addr, "192.168.1.9:50001".parse().unwrap());
    }

    #[test]
    fn parse_reply_v2_prefers_the_advertised_address() {
        let src: std::net::SocketAddr = "192.168.1.9:41234".parse().unwrap();
        let peer = super::parse_reply("HAYATE_HERE:v2:studio-mac:macos:50001:127.0.0.1:6.1.0", src)
            .expect("valid v2 reply");
        assert_eq!(peer.addr, "127.0.0.1:50001".parse().unwrap());
        assert_eq!(peer.version, "6.1.0");
    }

    #[test]
    fn parse_reply_rejects_other_packets() {
        let src: std::net::SocketAddr = "192.168.1.9:41234".parse().unwrap();
        assert!(super::parse_reply("HAYATE_PEER:v2:abcd:linux:50001", src).is_none());
        assert!(super::parse_reply("HAYATE_HERE:v1:only-a-name", src).is_none());
    }
}
