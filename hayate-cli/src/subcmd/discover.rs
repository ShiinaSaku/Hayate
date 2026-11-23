//! `hayate discover` subcommand.
//!
//! Discovery has three layers, cheapest first:
//!
//! 1. **mDNS browse** — receivers and pairing senders register
//!    `_hayate._udp.local.`, so browsing finds them without any scanning.
//! 2. **UDP query** — a broadcast `HAYATE_DISCOVER:v1` packet that receivers
//!    answer unicast. Works on networks that block mDNS and costs two packets.
//! 3. **QUIC subnet sweep** — probes every host in each local network. Only
//!    used with `--scan`, an explicit `--cidr`, or when the fast layers found
//!    nothing.
//!
//! The sweep core ([`scan_for_peers`]) is shared with `hayate send --pick`.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use futures_util::stream::{self, StreamExt};
use hayate::discovery::DiscoveredPeer;
use hayate::local_addr::Ipv4Network;

use crate::cli::DiscoverArgs;
use crate::output;

const DEFAULT_PORT: u16 = 50001;
const CONCURRENCY: usize = 128;

/// Upper bound on hosts swept by a subnet scan.
///
/// A /16 has 65k hosts; probing all of them is slow and noisy. The cap keeps a
/// mistaken CIDR from turning `discover` into a port scanner; the remainder is
/// reported so the user can narrow with `--cidr`.
const MAX_SCAN_HOSTS: u64 = 4096;

/// Budget for the fast (mDNS + UDP query) phase.
const FAST_DISCOVERY: Duration = Duration::from_secs(3);

/// A receiver found by discovery or a subnet scan.
pub struct FoundPeer {
    /// Display name ("Hayate Peer" / hostname).
    pub name: String,
    /// Reachable address of the peer.
    pub addr: SocketAddr,
    /// Peer OS when known, else "unknown".
    pub os: String,
    /// QUIC handshake round-trip time in milliseconds, when measured.
    pub rtt_ms: Option<f64>,
    /// Version reported by the peer, when known.
    pub version: String,
}

/// Result of probing a single host.
struct ProbeOutcome {
    addr: SocketAddr,
    name: String,
    resolved_ip: IpAddr,
    rtt_ms: f64,
}

pub async fn run(args: DiscoverArgs, cancelled: Arc<AtomicBool>) -> Result<()> {
    let fast_budget = FAST_DISCOVERY.min(Duration::from_secs(args.timeout.max(1)));
    let mut peers = fast_discover(fast_budget).await;
    for peer in &peers {
        announce_live(peer);
    }

    // Fall back to a sweep when the cheap layers found nothing, or whenever the
    // user asked for one explicitly.
    if !cancelled.load(Ordering::SeqCst) && (args.scan || args.cidr.is_some() || peers.is_empty()) {
        let swept = scan_for_peers(
            args.timeout.max(1),
            args.cidr.as_deref(),
            Arc::clone(&cancelled),
            true,
            announce_live,
        )
        .await?;
        merge_peers(&mut peers, swept);
    }

    let rows: Vec<(String, SocketAddr, String)> =
        peers.iter().map(|p| (p.name.clone(), p.addr, os_label(p))).collect();
    output::print_peer_table(&rows);

    if peers.is_empty() {
        output::info(
            "No Hayate peers found. Start one with `hayate receive`, or try --scan to sweep the subnet.",
        );
    } else {
        output::info("Tip: run `hayate receive` to start accepting transfers.");
    }

    Ok(())
}

/// Collects peers from mDNS and the UDP discovery query in parallel.
pub(crate) async fn fast_discover(timeout: Duration) -> Vec<FoundPeer> {
    let query = compio::runtime::spawn_blocking(move || hayate::discovery::query(timeout));
    let browse = compio::runtime::spawn_blocking(move || hayate::discovery::scan(timeout));
    let (queried, browsed) = futures_util::future::join(query, browse).await;

    let queried = queried.ok().and_then(Result::ok).unwrap_or_default();
    let browsed = browsed.ok().and_then(Result::ok).unwrap_or_default();

    let mut peers: Vec<FoundPeer> = Vec::new();
    for peer in queried.into_iter().chain(browsed) {
        push_unique(&mut peers, from_discovered(peer));
    }
    peers
}

/// Adds a peer unless an equivalent one is already known.
///
/// A receiver bound to all interfaces answers both the loopback and broadcast
/// query from the same machine, yielding `127.0.0.1:p` and `<lan-ip>:p`.
/// Collapse those into one row, preferring the routable address.
fn push_unique(peers: &mut Vec<FoundPeer>, peer: FoundPeer) {
    if let Some(existing) = peers
        .iter_mut()
        .find(|p| p.addr.port() == peer.addr.port() && p.name == peer.name && p.os == peer.os)
    {
        if existing.addr.ip().is_loopback() && !peer.addr.ip().is_loopback() {
            *existing = peer;
        }
        return;
    }
    peers.push(peer);
}

fn from_discovered(peer: DiscoveredPeer) -> FoundPeer {
    FoundPeer {
        name: peer.name,
        addr: peer.addr,
        os: peer.os,
        rtt_ms: peer.rtt_ms,
        version: peer.version,
    }
}

fn merge_peers(peers: &mut Vec<FoundPeer>, extra: Vec<FoundPeer>) {
    for peer in extra {
        let duplicate = peers.iter().any(|known| {
            known.addr.port() == peer.addr.port()
                && (known.addr.ip() == peer.addr.ip()
                    || (known.addr.ip().is_loopback()
                        && hayate::local_addr::is_local_ip(peer.addr.ip()))
                    || (peer.addr.ip().is_loopback()
                        && hayate::local_addr::is_local_ip(known.addr.ip())))
        });
        if !duplicate {
            // Sweep results are anonymous; give them a usable label.
            peers.push(peer);
        }
    }
}

fn announce_live(peer: &FoundPeer) {
    let rtt = format_rtt(peer.rtt_ms);
    output::peer_found_live(&peer.name, &peer.addr, &peer.os, &rtt, quality_indicator(peer.rtt_ms));
}

/// OS column text, including the peer version when it is known.
fn os_label(peer: &FoundPeer) -> String {
    if peer.version.is_empty() || peer.version == "unknown" {
        peer.os.clone()
    } else {
        format!("{} {}", peer.os, peer.version)
    }
}

/// Sweeps local subnets with QUIC probes for Hayate receivers.
///
/// `on_found` fires (from the collector task) for each newly discovered peer,
/// enabling live UIs. When `show_progress` is set a scan progress bar is drawn
/// on the shared MultiProgress.
pub async fn scan_for_peers(
    timeout_secs: u64,
    cidr: Option<&str>,
    cancelled: Arc<AtomicBool>,
    show_progress: bool,
    on_found: impl Fn(&FoundPeer),
) -> Result<Vec<FoundPeer>> {
    let networks = match cidr {
        Some(cidr) => vec![parse_cidr(cidr)?],
        None => detect_networks(),
    };

    if networks.is_empty() {
        output::warn("No local subnets detected. Loopback only. Use --cidr to specify a subnet.");
    } else {
        let list = networks
            .iter()
            .map(|n| format!("{}/{}", n.base, n.prefix))
            .collect::<Vec<_>>()
            .join(", ");
        output::info(&format!("Scanning {list} with {timeout_secs}s timeout"));
    }

    let timeout = Duration::from_secs(timeout_secs);
    let (mut targets, skipped) = build_targets(&networks, DEFAULT_PORT);
    if skipped > 0 {
        output::warn(&format!(
            "Sweeping the first {MAX_SCAN_HOSTS} of {} hosts; narrow with --cidr to cover the rest.",
            targets.len() as u64 + skipped
        ));
    }

    // Always probe loopback (same-machine multi-instance discovery).
    let loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT);
    if !targets.contains(&loopback) {
        targets.push(loopback);
    }

    let total_targets = targets.len() as u64;
    let show_progress = show_progress && !crate::policy::get().no_progress();
    let pb = show_progress.then(|| output::scan_progress_bar(total_targets));
    let found_count = AtomicU64::new(0);
    let seen = Mutex::new(HashSet::new());
    let (result_tx, result_rx) = flume::bounded::<ProbeOutcome>(64);

    let pb_clone = pb.clone();
    let cancelled_scan = Arc::clone(&cancelled);
    compio::runtime::spawn(async move {
        stream::iter(targets)
            .map(|addr| {
                let pb_ref = &pb_clone;
                let found_ref = &found_count;
                let result_tx = result_tx.clone();
                let seen = &seen;
                let cancelled_inner = Arc::clone(&cancelled_scan);
                async move {
                    if cancelled_inner.load(Ordering::SeqCst) {
                        return;
                    }
                    let outcome = probe_one_with_rtt(addr, timeout).await;
                    if let Some(pb) = pb_ref {
                        pb.inc(1);
                    }
                    if let Some(oc) = outcome {
                        let key = format!("{}-{}", oc.resolved_ip, oc.addr.port());
                        let mut is_new = false;
                        if let Ok(mut set) = seen.lock()
                            && !set.contains(&key)
                        {
                            set.insert(key);
                            is_new = true;
                        }
                        if is_new {
                            let n = found_ref.fetch_add(1, Ordering::Relaxed) + 1;
                            if let Some(pb) = pb_ref {
                                pb.set_message(format!("{n} peer(s) found"));
                            }
                            let _ = result_tx.send_async(oc).await;
                        }
                    }
                }
            })
            .buffer_unordered(CONCURRENCY)
            .collect::<Vec<()>>()
            .await;
        drop(result_tx);
    })
    .detach();

    // Real-time output: collect peers as they are discovered.
    let mut discovered = Vec::new();
    while let Ok(peer) = result_rx.recv_async().await {
        if cancelled.load(Ordering::SeqCst) {
            break;
        }
        let found = FoundPeer {
            name: peer.name,
            addr: SocketAddr::new(peer.resolved_ip, peer.addr.port()),
            os: "unknown".to_owned(),
            rtt_ms: Some(peer.rtt_ms),
            version: "unknown".to_owned(),
        };
        on_found(&found);
        discovered.push(found);
    }

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    Ok(discovered)
}

// ---------------------------------------------------------------------------
// Subnet helpers
// ---------------------------------------------------------------------------

fn parse_cidr(cidr: &str) -> Result<Ipv4Network> {
    Ipv4Network::parse_cidr(cidr).with_context(|| {
        format!("invalid CIDR \"{cidr}\" (expected e.g. 192.168.1.0/24 or 10.0.0.0/16)")
    })
}

/// Detects local networks using the real interface netmasks.
fn detect_networks() -> Vec<Ipv4Network> {
    hayate::local_addr::local_networks()
}

/// Expands networks into probe targets, capped at [`MAX_SCAN_HOSTS`].
fn build_targets(networks: &[Ipv4Network], port: u16) -> (Vec<SocketAddr>, u64) {
    let total: u64 = networks.iter().map(Ipv4Network::host_count).sum();
    let capacity = total.min(MAX_SCAN_HOSTS) as usize + 1;
    let mut targets = Vec::with_capacity(capacity);

    'outer: for network in networks {
        for ip in network.hosts() {
            if targets.len() as u64 >= MAX_SCAN_HOSTS {
                break 'outer;
            }
            targets.push(SocketAddr::new(IpAddr::V4(ip), port));
        }
    }

    let skipped = total.saturating_sub(targets.len() as u64);
    (targets, skipped)
}

// ---------------------------------------------------------------------------
// Probe with RTT measurement
// ---------------------------------------------------------------------------

/// Probes a single host, measuring round-trip time via the QUIC handshake.
/// Per-probe endpoint creation is acceptable because TLS config is cached
/// (OnceLock in `network.rs`) and UDP socket bind is sub-millisecond.
async fn probe_one_with_rtt(addr: SocketAddr, timeout: Duration) -> Option<ProbeOutcome> {
    let start = Instant::now();
    let result = compio::time::timeout(timeout, async move {
        let client_cfg = hayate::network::client_config().ok()?;
        let endpoint = hayate::network::bind_client_for(addr).await.ok()?;
        let conn = endpoint.connect(addr, "hayate.local", Some(client_cfg)).ok()?.await.ok()?;
        conn.close(0u32.into(), b"discover");
        let elapsed = start.elapsed();
        Some(elapsed)
    })
    .await
    .ok()
    .flatten();

    match result {
        Some(elapsed) => {
            let rtt_ms = elapsed.as_secs_f64() * 1000.0;
            let ip = addr.ip();
            let is_local = ip.is_loopback() || hayate::local_addr::is_local_ip(ip);
            let name =
                if is_local { "Local Instance".to_owned() } else { "Hayate Peer".to_owned() };
            let resolved_ip = if is_local {
                hayate::local_addr::primary_local_ipv4().map(IpAddr::V4).unwrap_or(ip)
            } else {
                ip
            };

            Some(ProbeOutcome { addr, name, resolved_ip, rtt_ms })
        },
        None => None,
    }
}

/// Formats round-trip time in a human-readable way.
fn format_rtt(rtt_ms: Option<f64>) -> String {
    let Some(rtt_ms) = rtt_ms else {
        return "—".to_owned();
    };
    if rtt_ms < 1.0 {
        format!("{:.0}µs", rtt_ms * 1000.0)
    } else if rtt_ms < 10.0 {
        format!("{:.1}ms", rtt_ms)
    } else {
        format!("{:.0}ms", rtt_ms)
    }
}

/// Returns a quality indicator character based on RTT.
fn quality_indicator(rtt_ms: Option<f64>) -> &'static str {
    match rtt_ms {
        Some(ms) if ms < 2.0 => "●",  // excellent
        Some(ms) if ms < 10.0 => "◉", // good
        Some(ms) if ms < 50.0 => "◎", // fair
        Some(_) => "○",               // poor
        None => "○",                  // not measured
    }
}
