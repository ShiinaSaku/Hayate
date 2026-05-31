//! `hayate discover` subcommand.
//!
//! Probes every host in the local /24 subnet for an active Hayate QUIC
//! listener.  Up to `CONCURRENCY` simultaneous QUIC connects run in parallel
//! via `futures_util::stream::buffer_unordered`.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use anyhow::Result;
use futures_util::stream::{self, StreamExt};

use crate::{cli::DiscoverArgs, output};

const DEFAULT_PORT: u16 = 50001;
const CONCURRENCY: usize = 64;

pub async fn run(args: DiscoverArgs) -> Result<()> {
    output::print_banner();

    let subnets = if let Some(cidr) = args.cidr {
        parse_cidr(&cidr)?
    } else {
        local_subnets()
    };

    if subnets.is_empty() {
        output::warn("No local subnets detected. Loopback only. Use --cidr to specify a subnet.");
    } else {
        output::info(&format!(
            "Scanning {} subnet(s) for {}s...",
            subnets.len(),
            args.timeout
        ));
    }

    let timeout = Duration::from_secs(args.timeout);

    let mut targets: Vec<SocketAddr> = subnets
        .iter()
        .flat_map(|base| {
            let octets = base.octets();
            (1u8..=254).map(move |host| {
                SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(octets[0], octets[1], octets[2], host)),
                    DEFAULT_PORT,
                )
            })
        })
        .collect();

    // Always scan loopback (same-machine multi-tab discovery)
    targets.push(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        DEFAULT_PORT,
    ));

    let peers: Vec<(String, SocketAddr, String)> = stream::iter(targets)
        .map(|addr| async move {
            let res = probe_one(addr, timeout).await;
            (addr, res)
        })
        .buffer_unordered(CONCURRENCY)
        .filter_map(|(addr, res)| async move {
            res.map(|(name, resolved_ip)| {
                let os = if name == "Local Instance" {
                    std::env::consts::OS.to_owned()
                } else {
                    "unknown".to_owned()
                };
                (name, SocketAddr::new(resolved_ip, addr.port()), os)
            })
        })
        .collect()
        .await;

    let mut unique_addrs = std::collections::HashSet::new();
    let mut deduplicated_peers = Vec::new();
    for peer in peers {
        let addr_key = format!("{}", peer.1);
        if unique_addrs.insert(addr_key) {
            deduplicated_peers.push(peer);
        }
    }

    output::print_peer_table(&deduplicated_peers);
    Ok(())
}

// ---------------------------------------------------------------------------
// Subnet helpers
// ---------------------------------------------------------------------------

fn is_valid_local_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    // Exclude loopback (127.0.0.0/8)
    if octets[0] == 127 {
        return false;
    }
    // Exclude unspecified (0.0.0.0)
    if octets[0] == 0 {
        return false;
    }
    // Exclude multicast, reserved, and broadcast (224.0.0.0/4)
    if octets[0] >= 224 {
        return false;
    }
    true
}

fn local_subnets() -> Vec<Ipv4Addr> {
    let mut bases = Vec::new();
    if let Ok(ifaces) = get_if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            if let get_if_addrs::IfAddr::V4(ifv4) = iface.addr
                && is_valid_local_ipv4(ifv4.ip)
            {
                let o = ifv4.ip.octets();
                let base = Ipv4Addr::new(o[0], o[1], o[2], 0);
                if !bases.contains(&base) {
                    bases.push(base);
                }
            }
        }
    }
    bases
}

fn parse_cidr(cidr: &str) -> Result<Vec<Ipv4Addr>> {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!("invalid CIDR: {cidr}");
    }
    let base: Ipv4Addr = parts[0].parse()?;
    let prefix: u8 = parts[1].parse()?;
    if prefix != 24 {
        anyhow::bail!("only /24 CIDR is supported for scanning");
    }
    let o = base.octets();
    Ok(vec![Ipv4Addr::new(o[0], o[1], o[2], 0)])
}

fn get_local_ip() -> String {
    if let Ok(ifaces) = get_if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            if let get_if_addrs::IfAddr::V4(ifv4) = iface.addr
                && is_valid_local_ipv4(ifv4.ip)
            {
                return ifv4.ip.to_string();
            }
        }
    }
    "127.0.0.1".to_owned()
}

fn is_local_ip(ip: IpAddr) -> bool {
    if let Ok(ifaces) = get_if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.ip() == ip {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------------

/// Returns `Some((name, resolved_ip))` if a Hayate receiver is live at `addr`.
async fn probe_one(addr: SocketAddr, timeout: Duration) -> Option<(String, IpAddr)> {
    compio::time::timeout(timeout, async move {
        let client_cfg = hayate_engine::network::client_config().ok()?;
        let endpoint = hayate_engine::network::bind_client().await.ok()?;
        let conn = endpoint
            .connect(addr, "hayate.local", Some(client_cfg))
            .ok()?
            .await
            .ok()?;
        conn.close(0u32.into(), b"discover");

        let ip = addr.ip();
        let is_local = ip.is_loopback() || is_local_ip(ip);
        let name = if is_local {
            "Local Instance".to_owned()
        } else {
            "Hayate Peer".to_owned()
        };
        let resolved_ip = if is_local {
            get_local_ip().parse().unwrap_or(ip)
        } else {
            ip
        };

        Some((name, resolved_ip))
    })
    .await
    .ok()
    .flatten()
}
