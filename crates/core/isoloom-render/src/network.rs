//! The network pieces every target renders the same way: Isoloom's router, VLAN trunks, IPv6 on
//! a VM, and a VM's further addresses on a network.

pub mod ipv6;
pub(crate) mod more_addresses;
pub mod router;
pub mod trunks;

use std::fmt::Display;
use std::net::IpAddr;

/// The command that routes `to` (a block, or `default`) through `via`: `ip route replace <to> via
/// <via>`, `ip -6 route replace` for an IPv6 next hop. It replaces a route already there, so
/// running it again changes nothing. Every target's routes are written by it.
#[must_use]
pub fn route_line(to: impl Display, via: impl Into<IpAddr>) -> String {
    match via.into() {
        IpAddr::V4(via) => format!("ip route replace {to} via {via}"),
        IpAddr::V6(via) => format!("ip -6 route replace {to} via {via}"),
    }
}

#[cfg(test)]
mod route_tests {
    use super::route_line;

    #[test]
    fn a_route_line_is_ips_by_the_next_hops_family() {
        let v4: std::net::Ipv4Addr = "10.1.0.254".parse().unwrap();
        let v6: std::net::Ipv6Addr = "fd00:1::254".parse().unwrap();
        assert_eq!(route_line("10.2.0.0/24", v4), "ip route replace 10.2.0.0/24 via 10.1.0.254");
        assert_eq!(route_line("default", v4), "ip route replace default via 10.1.0.254");
        assert_eq!(route_line("fd00:2::/64", v6), "ip -6 route replace fd00:2::/64 via fd00:1::254");
    }
}
