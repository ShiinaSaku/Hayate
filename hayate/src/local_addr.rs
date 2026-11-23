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

/// An IPv4 network (subnet) derived from an interface address and netmask.
///
/// Host bits are cleared, so `192.168.1.45/255.255.255.0` becomes
/// `192.168.1.0/24`. Use [`Ipv4Network::hosts`] to iterate the usable host
/// addresses instead of assuming a /24.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ipv4Network {
    /// Network base address (host bits cleared).
    pub base: Ipv4Addr,
    /// Prefix length in bits (`0..=32`).
    pub prefix: u8,
}

impl Ipv4Network {
    /// Builds a network from an address and a contiguous netmask.
    ///
    /// Non-contiguous masks are rounded to the number of set bits, which keeps
    /// scanning bounded even for unusual interface configurations.
    #[must_use]
    pub fn from_addr_netmask(addr: Ipv4Addr, netmask: Ipv4Addr) -> Self {
        let prefix = netmask.octets().iter().map(|byte| byte.count_ones() as u8).sum();
        Self { base: mask_base(addr, prefix), prefix }
    }

    /// Parses `a.b.c.d/prefix` (prefix `0..=32`).
    ///
    /// Returns `None` for malformed input. Host bits in the address are
    /// cleared, so `10.1.2.3/16` yields `10.1.0.0/16`.
    #[must_use]
    pub fn parse_cidr(text: &str) -> Option<Self> {
        let (addr, prefix) = text.split_once('/')?;
        let addr: Ipv4Addr = addr.trim().parse().ok()?;
        let prefix: u8 = prefix.trim().parse().ok()?;
        if prefix > 32 {
            return None;
        }
        Some(Self { base: mask_base(addr, prefix), prefix })
    }

    /// Number of usable host addresses in the network.
    ///
    /// `/31` and `/32` return `1` and `2` respectively; every other prefix
    /// excludes the network and broadcast addresses.
    #[must_use]
    pub fn host_count(&self) -> u64 {
        let (start, end) = self.host_range();
        end - start
    }

    /// Broadcast address for this network.
    #[must_use]
    pub fn broadcast(&self) -> Ipv4Addr {
        let mask = if self.prefix == 0 { 0 } else { u32::MAX << (32 - u32::from(self.prefix)) };
        Ipv4Addr::from(u32::from(self.base) | !mask)
    }

    /// Iterates usable host addresses in ascending order.
    pub fn hosts(&self) -> impl Iterator<Item = Ipv4Addr> + '_ {
        let (start, end) = self.host_range();
        let base = u32::from(self.base);
        (start..end).map(move |offset| Ipv4Addr::from(base.wrapping_add(offset as u32)))
    }

    /// Returns true when `ip` belongs to this network.
    #[must_use]
    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        mask_base(ip, self.prefix) == self.base
    }

    /// Half-open offset range of usable host addresses.
    fn host_range(self) -> (u64, u64) {
        let total = 1u64 << (32 - u32::from(self.prefix));
        if self.prefix >= 31 { (0, total) } else { (1, total - 1) }
    }
}

fn mask_base(addr: Ipv4Addr, prefix: u8) -> Ipv4Addr {
    let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - u32::from(prefix.min(32))) };
    Ipv4Addr::from(u32::from(addr) & mask)
}

/// Retrieves the local IPv4 networks together with their real prefix lengths.
///
/// Unlike [`local_subnets`], which assumes every interface is a `/24`, this
/// honours the interface netmask reported by the OS (`/16`, `/22`, `/23`, …).
/// Interfaces that report a `/0` or `/32` mask (unknown or point-to-point) fall
/// back to `/24`, matching the historical scanning behaviour.
#[must_use]
pub fn local_networks() -> Vec<Ipv4Network> {
    let mut networks = Vec::new();
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            if let if_addrs::IfAddr::V4(ifv4) = iface.addr
                && is_usable_local_ipv4(ifv4.ip)
            {
                let mut net = Ipv4Network::from_addr_netmask(ifv4.ip, ifv4.netmask);
                if net.prefix == 0 || net.prefix > 30 {
                    net = Ipv4Network::from_addr_netmask(ifv4.ip, Ipv4Addr::new(255, 255, 255, 0));
                }
                if !networks.contains(&net) {
                    networks.push(net);
                }
            }
        }
    }
    networks
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

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::Ipv4Network;

    fn net(base: [u8; 4], prefix: u8) -> Ipv4Network {
        Ipv4Network { base: Ipv4Addr::from(base), prefix }
    }

    #[test]
    fn netmask_builds_network_and_clears_host_bits() {
        let n = Ipv4Network::from_addr_netmask(
            Ipv4Addr::new(10, 1, 2, 3),
            Ipv4Addr::new(255, 255, 0, 0),
        );
        assert_eq!(n, net([10, 1, 0, 0], 16));
        assert_eq!(n.broadcast(), Ipv4Addr::new(10, 1, 255, 255));
        assert_eq!(n.host_count(), 65_534);
    }

    #[test]
    fn parse_cidr_clears_host_bits() {
        assert_eq!(Ipv4Network::parse_cidr("192.168.1.45/24"), Some(net([192, 168, 1, 0], 24)));
        assert_eq!(Ipv4Network::parse_cidr("172.16.5.9/22"), Some(net([172, 16, 4, 0], 22)));
        assert_eq!(Ipv4Network::parse_cidr("bogus"), None);
        assert_eq!(Ipv4Network::parse_cidr("10.0.0.1/33"), None);
    }

    #[test]
    fn hosts_exclude_network_and_broadcast() {
        let n = net([192, 168, 1, 0], 24);
        let hosts: Vec<_> = n.hosts().collect();
        assert_eq!(hosts.len(), 254);
        assert_eq!(hosts[0], Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(hosts[253], Ipv4Addr::new(192, 168, 1, 254));
    }

    #[test]
    fn contains_matches_only_the_network() {
        let n = net([172, 16, 4, 0], 22);
        assert!(n.contains(Ipv4Addr::new(172, 16, 7, 200)));
        assert!(!n.contains(Ipv4Addr::new(172, 16, 8, 1)));
    }

    #[test]
    fn point_to_point_and_host_prefixes_stay_scannable() {
        assert_eq!(net([10, 0, 0, 1], 32).host_count(), 1);
        assert_eq!(net([10, 0, 0, 2], 31).host_count(), 2);
        assert_eq!(net([10, 0, 0, 2], 31).hosts().collect::<Vec<_>>().len(), 2);
    }
}
