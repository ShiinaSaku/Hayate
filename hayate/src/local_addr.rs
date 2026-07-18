//! Utilities for resolving and querying local network interface addresses and
//! subnets.
//!
//! This module helps library users discover local IPv4 and IPv6 addresses on
//! active network interfaces without requiring manual network configuration. It
//! combines operating system interface queries with UDP socket route probes to
//! find the most reliable local address candidates.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

/// Retrieves a list of all usable, non-loopback local IPv4 addresses.
///
/// This function queries network interfaces using operating-system APIs,
/// checks active routing tables, and initiates UDP socket connection probes to
/// compile a list of all IPv4 addresses that can be used for direct local
/// communication.
///
/// Duplicate entries are filtered, and loopback, multicast, or unroutable
/// addresses are excluded.
///
/// # Examples
///
/// ```
/// use hayate::local_addr::local_ipv4s;
///
/// let ips = local_ipv4s();
/// for ip in ips {
///     println!("Active local IPv4: {ip}");
/// }
/// ```
#[must_use]
pub fn local_ipv4s() -> Vec<Ipv4Addr> {
    let mut ips = interface_ipv4s();

    for ip in route_probe_ipv4s() {
        if !ips.contains(&ip) {
            ips.push(ip);
        }
    }

    ips
}

/// Retrieves a list of the base network addresses (subnets) for all local
/// interfaces.
///
/// The returned `Ipv4Addr`s represent the subnet prefix (e.g. `192.168.1.0` for
/// `192.168.1.45`), which is useful for scanning or network discovery tasks on
/// local networks.
///
/// # Examples
///
/// ```
/// use hayate::local_addr::local_subnets;
///
/// let subnets = local_subnets();
/// for subnet in subnets {
///     println!("Available local subnet base: {subnet}");
/// }
/// ```
#[must_use]
pub fn local_subnets() -> Vec<Ipv4Addr> {
    let mut bases = Vec::new();
    for ip in local_ipv4s() {
        let o = ip.octets();
        let base = Ipv4Addr::new(o[0], o[1], o[2], 0);
        if !bases.contains(&base) {
            bases.push(base);
        }
    }
    bases
}

/// Gets the primary local IPv4 address of the machine, if any.
///
/// This returns the first usable, non-loopback IPv4 address discovered.
///
/// # Examples
///
/// ```
/// use hayate::local_addr::primary_local_ipv4;
///
/// if let Some(ip) = primary_local_ipv4() {
///     println!("Primary local IPv4 is: {ip}");
/// } else {
///     println!("No local network IPv4 address found.");
/// }
/// ```
#[must_use]
pub fn primary_local_ipv4() -> Option<Ipv4Addr> {
    local_ipv4s().into_iter().next()
}

/// Checks if a given IP address belongs to one of the local machine's active
/// network interfaces.
///
/// Supports both IPv4 and IPv6 checks.
///
/// # Examples
///
/// ```
/// use std::net::IpAddr;
/// use hayate::local_addr::is_local_ip;
///
/// let test_ip: IpAddr = "127.0.0.1".parse().unwrap();
/// assert!(!is_local_ip(test_ip)); // Loopback is excluded by default
/// ```
#[must_use]
pub fn is_local_ip(ip: IpAddr) -> bool {
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        return ifaces.into_iter().any(|iface| !iface.is_loopback() && iface.ip() == ip);
    }
    false
}

/// Queries network interfaces for IPv4 addresses.
fn interface_ipv4s() -> Vec<Ipv4Addr> {
    let mut ips = Vec::new();
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            if let if_addrs::IfAddr::V4(ifv4) = iface.addr
                && is_usable_local_ipv4(ifv4.ip)
                && !ips.contains(&ifv4.ip)
            {
                ips.push(ifv4.ip);
            }
        }
    }
    ips
}

/// Performs active UDP connection probes to common targets to deduce the local
/// routing IP.
fn route_probe_ipv4s() -> Vec<Ipv4Addr> {
    let mut ips = Vec::new();
    let probes = [
        SocketAddr::from(([8, 8, 8, 8], 80)),
        SocketAddr::from(([1, 1, 1, 1], 80)),
        SocketAddr::from(([192, 168, 1, 1], 80)),
        SocketAddr::from(([10, 0, 0, 1], 80)),
    ];

    for target in probes {
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
            continue;
        };
        if socket.connect(target).is_err() {
            continue;
        }
        let Ok(local_addr) = socket.local_addr() else {
            continue;
        };
        let IpAddr::V4(ip) = local_addr.ip() else {
            continue;
        };
        if is_usable_local_ipv4(ip) && !ips.contains(&ip) {
            ips.push(ip);
        }
    }

    ips
}

/// Returns true if the IP is a valid routable local address.
fn is_usable_local_ipv4(ip: Ipv4Addr) -> bool {
    !ip.is_loopback()
        && !ip.is_unspecified()
        && !ip.is_multicast()
        && !ip.is_broadcast()
        && !ip.is_documentation()
}
