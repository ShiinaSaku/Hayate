//! `hayate discover` subcommand.
//!
//! Probes every host in the local subnet for active Hayate receivers. Uses
//! high-concurrency QUIC probes with RTT measurement and real-time result
//! streaming so peers appear as they are discovered rather than waiting for
//! the full scan to complete.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use futures_util::stream::{self, StreamExt};
use hayate::local_addr;

use crate::cli::DiscoverArgs;
use crate::output;

const DEFAULT_PORT: u16 = 50001;
const CONCURRENCY: usize = 128;

/// Result of probing a single host.
struct ProbeOutcome {
    addr: SocketAddr,
    name: String,
    resolved_ip: IpAddr,
    rtt_ms: f64,
    os: String,
}

pub async fn run(args: DiscoverArgs, cancelled: Arc<AtomicBool>) -> Result<()> {
    let subnets =
        if let Some(cidr) = &args.cidr { parse_cidr(cidr)? } else { detect_all_subnets() };

    if subnets.is_empty() {
        output::warn("No local subnets detected. Loopback only. Use --cidr to specify a subnet.");
    } else {
        output::info(&format!(
            "Scanning {} subnet(s) with {}s timeout",
            subnets.len(),
            args.timeout
        ));
    }

    let timeout = Duration::from_secs(args.timeout);

    let mut targets: Vec<SocketAddr> = subnets
        .iter()
        .flat_map(|base| {
            let octets = base.octets();
            // Prioritise common gateway-adjacent addresses (.1-.10, .254) first,
            // then sweep the remaining host range.
            let priority = [1u8, 2, 3, 254, 253];
            let priority_set: HashSet<u8> = priority.iter().copied().collect();
            let mut hosts: Vec<u8> = priority.to_vec();
            for h in 4u8..=252 {
                if !priority_set.contains(&h) {
                    hosts.push(h);
                }
            }
            hosts.into_iter().map(move |host| {
                SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(octets[0], octets[1], octets[2], host)),
                    DEFAULT_PORT,
                )
            })
        })
        .collect();

    // Always scan loopback (same-machine multi-tab discovery)
    let has_loopback = targets.iter().any(|t| t.ip().is_loopback());
    if !has_loopback {
        targets.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), DEFAULT_PORT));
    }

    let total_targets = targets.len() as u64;
    let pb = output::scan_progress_bar(total_targets);
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
                    pb_ref.inc(1);
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
                            pb_ref.set_message(format!("{n} peer(s) found"));
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

    // Real-time output: print peers as they are discovered.
    let mut discovered = Vec::new();
    while let Ok(peer) = result_rx.recv_async().await {
        if cancelled.load(Ordering::SeqCst) {
            break;
        }
        let rtt_str = format_rtt(peer.rtt_ms);
        output::peer_found_live(
            &peer.name,
            &peer.addr,
            &peer.os,
            &rtt_str,
            quality_indicator(peer.rtt_ms),
        );
        discovered.push(peer);
    }

    pb.finish_and_clear();

    // Convert to the table format for final display
    let peers: Vec<(String, SocketAddr, String)> = discovered
        .into_iter()
        .map(|p| (p.name, SocketAddr::new(p.resolved_ip, p.addr.port()), p.os))
        .collect();

    output::print_peer_table(&peers);

    if !peers.is_empty() {
        output::info("Tip: run `hayate receive` to start accepting transfers.");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Subnet helpers
// ---------------------------------------------------------------------------

fn parse_cidr(cidr: &str) -> Result<Vec<Ipv4Addr>> {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!("invalid CIDR: {cidr}");
    }
    let base: Ipv4Addr = parts[0].parse()?;
    let prefix: u8 = parts[1].parse()?;
    if ![16, 24].contains(&prefix) {
        anyhow::bail!("only /24 and /16 CIDR prefixes are supported for scanning");
    }
    let o = base.octets();
    if prefix == 24 {
        Ok(vec![Ipv4Addr::new(o[0], o[1], o[2], 0)])
    } else {
        let mut subnets = Vec::with_capacity(256);
        for third in 0u8..=255 {
            subnets.push(Ipv4Addr::new(o[0], o[1], third, 0));
        }
        Ok(subnets)
    }
}

/// Detect all local subnets by combining interface info, route probes, and
/// fallback common private ranges.
fn detect_all_subnets() -> Vec<Ipv4Addr> {
    let mut bases = local_addr::local_subnets();

    if bases.is_empty() {
        bases.extend([
            Ipv4Addr::new(192, 168, 1, 0),
            Ipv4Addr::new(192, 168, 0, 0),
            Ipv4Addr::new(10, 0, 0, 0),
            Ipv4Addr::new(172, 16, 0, 0),
        ]);
    }

    bases
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
        let endpoint = hayate::network::bind_client().await.ok()?;
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
            let is_local = ip.is_loopback() || local_addr::is_local_ip(ip);
            let name =
                if is_local { "Local Instance".to_owned() } else { "Hayate Peer".to_owned() };
            let resolved_ip = if is_local {
                local_addr::primary_local_ipv4().map(IpAddr::V4).unwrap_or(ip)
            } else {
                ip
            };
            let os = if is_local { std::env::consts::OS.to_owned() } else { "unknown".to_owned() };

            Some(ProbeOutcome { addr, name, resolved_ip, rtt_ms, os })
        },
        None => None,
    }
}

/// Formats round-trip time in a human-readable way.
fn format_rtt(rtt_ms: f64) -> String {
    if rtt_ms < 1.0 {
        format!("{:.0}µs", rtt_ms * 1000.0)
    } else if rtt_ms < 10.0 {
        format!("{:.1}ms", rtt_ms)
    } else {
        format!("{:.0}ms", rtt_ms)
    }
}

/// Returns a quality indicator character based on RTT.
fn quality_indicator(rtt_ms: f64) -> &'static str {
    if rtt_ms < 2.0 {
        "●" // excellent
    } else if rtt_ms < 10.0 {
        "◉" // good
    } else if rtt_ms < 50.0 {
        "◎" // fair
    } else {
        "○" // poor
    }
}
