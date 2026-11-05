//! Local network reasoning: which of a phone's addresses can we reach, and if
//! none, why. This is what lets us say "your phone and PC are on different
//! Wi-Fi networks" instead of "connection timed out".

use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr};

/// An IPv4 address of this PC with its network prefix, e.g. 192.168.1.20/24.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalIface {
    pub name: String,
    pub ip: Ipv4Addr,
    pub prefix_len: u8,
}

impl LocalIface {
    pub fn new(name: impl Into<String>, ip: Ipv4Addr, prefix_len: u8) -> Self {
        Self {
            name: name.into(),
            ip,
            prefix_len: prefix_len.min(32),
        }
    }

    pub fn from_netmask(name: impl Into<String>, ip: Ipv4Addr, netmask: Ipv4Addr) -> Self {
        Self::new(name, ip, u32::from(netmask).count_ones() as u8)
    }

    pub fn contains(&self, other: Ipv4Addr) -> bool {
        if self.prefix_len == 0 {
            return true;
        }
        let mask = u32::MAX << (32 - u32::from(self.prefix_len));
        (u32::from(self.ip) & mask) == (u32::from(other) & mask)
    }
}

/// Interfaces that can plausibly talk to a phone: not loopback, not
/// link-local (169.254.x.x means "no DHCP answer"), not a /32 VPN tunnel.
pub fn usable_ifaces(all: &[LocalIface]) -> Vec<LocalIface> {
    all.iter()
        .filter(|i| !i.ip.is_loopback() && !i.ip.is_link_local() && !i.ip.is_unspecified())
        .filter(|i| i.prefix_len < 32)
        .cloned()
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressChoice {
    /// On the same subnet as one of our interfaces.
    SameNetwork(IpAddr),
    /// The phone only has addresses outside our subnets.
    DifferentNetwork(IpAddr),
    /// The phone advertised nothing usable.
    None,
}

/// Pick the address to connect to. Prefers IPv4 on one of our subnets; then
/// any IPv6 link-local-free address (rare, but works on some routers); then
/// reports the first IPv4 as "different network".
pub fn choose_address(candidates: &[IpAddr], ifaces: &[LocalIface]) -> AddressChoice {
    let ifaces = usable_ifaces(ifaces);
    for addr in candidates {
        if let IpAddr::V4(v4) = addr {
            if ifaces.iter().any(|i| i.contains(*v4)) {
                return AddressChoice::SameNetwork(*addr);
            }
        }
    }
    let first_v4 = candidates.iter().find(|a| a.is_ipv4()).copied();
    match first_v4 {
        Some(a) => AddressChoice::DifferentNetwork(a),
        None => match candidates
            .iter()
            .find(|a| matches!(a, IpAddr::V6(v6) if !is_v6_link_local(v6)))
        {
            Some(a) => AddressChoice::SameNetwork(*a),
            None => AddressChoice::None,
        },
    }
}

fn is_v6_link_local(v6: &std::net::Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xffc0) == 0xfe80
}

/// `host:port`, bracketing IPv6 as adb expects.
pub fn host_port(ip: IpAddr, port: u16) -> String {
    match ip {
        IpAddr::V4(v4) => format!("{v4}:{port}"),
        IpAddr::V6(v6) => format!("[{v6}]:{port}"),
    }
}

/// Parse `1.2.3.4:5555` (or `[::1]:5555`) into an address and port.
pub fn parse_host_port(s: &str) -> Option<(IpAddr, u16)> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('[') {
        let (ip, port) = rest.split_once("]:")?;
        return Some((ip.parse().ok()?, port.parse().ok()?));
    }
    let (ip, port) = s.rsplit_once(':')?;
    Some((ip.parse().ok()?, port.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> Vec<LocalIface> {
        vec![
            LocalIface::new("lo", Ipv4Addr::LOCALHOST, 8),
            LocalIface::new("Wi-Fi", Ipv4Addr::new(192, 168, 1, 20), 24),
            LocalIface::new("vEthernet (WSL)", Ipv4Addr::new(172, 20, 0, 1), 20),
        ]
    }

    #[test]
    fn subnet_math() {
        let i = LocalIface::new("x", Ipv4Addr::new(10, 1, 2, 3), 16);
        assert!(i.contains(Ipv4Addr::new(10, 1, 200, 9)));
        assert!(!i.contains(Ipv4Addr::new(10, 2, 0, 1)));
        let m = LocalIface::from_netmask(
            "x",
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(255, 255, 255, 0),
        );
        assert_eq!(m.prefix_len, 24);
    }

    #[test]
    fn picks_same_subnet_address() {
        let c = [
            IpAddr::V4(Ipv4Addr::new(10, 9, 9, 9)),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 77)),
        ];
        assert_eq!(
            choose_address(&c, &home()),
            AddressChoice::SameNetwork(c[1])
        );
    }

    #[test]
    fn reports_different_network() {
        let c = [IpAddr::V4(Ipv4Addr::new(192, 168, 43, 5))];
        assert_eq!(
            choose_address(&c, &home()),
            AddressChoice::DifferentNetwork(c[0])
        );
    }

    #[test]
    fn loopback_and_link_local_are_ignored() {
        let ifaces = vec![
            LocalIface::new("lo", Ipv4Addr::LOCALHOST, 8),
            LocalIface::new("eth", Ipv4Addr::new(169, 254, 3, 4), 16),
        ];
        assert!(usable_ifaces(&ifaces).is_empty());
        let c = [IpAddr::V4(Ipv4Addr::new(169, 254, 9, 9))];
        assert_eq!(
            choose_address(&c, &ifaces),
            AddressChoice::DifferentNetwork(c[0])
        );
    }

    #[test]
    fn no_candidates() {
        assert_eq!(choose_address(&[], &home()), AddressChoice::None);
    }

    #[test]
    fn host_port_round_trip() {
        let v4: IpAddr = "192.168.1.5".parse().unwrap();
        assert_eq!(host_port(v4, 37000), "192.168.1.5:37000");
        assert_eq!(parse_host_port("192.168.1.5:37000"), Some((v4, 37000)));
        let v6: IpAddr = "fd00::5".parse().unwrap();
        assert_eq!(host_port(v6, 5555), "[fd00::5]:5555");
        assert_eq!(parse_host_port("[fd00::5]:5555"), Some((v6, 5555)));
        assert_eq!(parse_host_port("adb-XYZ._adb-tls-connect._tcp"), None);
    }
}
