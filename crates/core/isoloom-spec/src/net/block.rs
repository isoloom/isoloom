//! A network's block: an IPv4 block of a size a network can have (/24 to /29), and the addresses
//! Isoloom reserves in it.

use std::fmt;
use std::net::Ipv4Addr;

use super::Cidr;

/// A network's IPv4 block: a [`Cidr`] of /24 to /29, the sizes validation lets a network have.
/// The addresses Isoloom reserves (the gateway, the tools, the controller, the router) are its
/// own, so each lies in the block whatever its size: a block of any other size has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkBlock(Cidr);

impl NetworkBlock {
    /// The shortest prefix a network's block has.
    pub const SHORTEST: u8 = 24;

    /// The longest prefix a network's block has.
    pub const LONGEST: u8 = 29;

    /// `block`, when it is a network's size: /24 to /29.
    #[must_use]
    pub const fn new(block: Cidr) -> Option<NetworkBlock> {
        if block.prefix() >= Self::SHORTEST && block.prefix() <= Self::LONGEST {
            Some(NetworkBlock(block))
        } else {
            None
        }
    }

    /// The block written `a.b.c.d/len`, when it is one of a network's size.
    #[must_use]
    pub fn parse(s: &str) -> Option<NetworkBlock> {
        Cidr::parse(s).and_then(NetworkBlock::new)
    }

    /// The block of `len` bits that holds `address`, `len` taken into /24 to /29: `10.9.3.0/24`
    /// for `10.9.3.10` and 24.
    #[must_use]
    pub const fn enclosing(address: Ipv4Addr, len: u8) -> NetworkBlock {
        let len = if len < Self::SHORTEST {
            Self::SHORTEST
        } else if len > Self::LONGEST {
            Self::LONGEST
        } else {
            len
        };
        NetworkBlock(Cidr::enclosing(address, len))
    }

    /// The block, as any IPv4 block.
    #[must_use]
    pub const fn cidr(self) -> Cidr {
        self.0
    }

    /// Its prefix length, 24 to 29.
    #[must_use]
    pub const fn prefix(self) -> u8 {
        self.0.prefix()
    }

    /// Its netmask: `255.255.255.0` for a /24.
    #[must_use]
    pub fn netmask(self) -> Ipv4Addr {
        self.0.netmask()
    }

    /// Its network address, as a number.
    const fn base(self) -> u32 {
        self.0.base()
    }

    /// How many addresses it holds: 256 for a /24, 8 for a /29.
    const fn size(self) -> u32 {
        1 << (32 - self.prefix())
    }

    /// The address of the block's `offset`th address (0 its network address).
    fn at(self, offset: u32) -> Ipv4Addr {
        Ipv4Addr::from(self.base() + offset)
    }

    /// The address with this last octet, the block's own (`10.9.0.10` for 10 in `10.9.0.0/24`):
    /// a machine's address on the network.
    #[must_use]
    pub fn with_octet(self, last_octet: u8) -> Ipv4Addr {
        Ipv4Addr::from((self.base() & !0xff) | u32::from(last_octet))
    }

    /// The address with this last octet, if it is a usable host address in the block.
    /// Reserved on every target: the gateway (the first usable address) and the two last usable
    /// addresses (controller, router).
    #[must_use]
    pub fn host(self, last_octet: u8) -> Option<Ipv4Addr> {
        let addr = u32::from(self.with_octet(last_octet));
        let first = self.base() + 2; // the network address and the gateway excluded
        let last = self.base() + self.size() - 4; // the controller, the router and broadcast excluded
        (addr >= first && addr <= last).then(|| Ipv4Addr::from(addr))
    }

    /// The gateway's address: the first usable address of the block (e.g. .1 in a /24).
    #[must_use]
    pub fn gateway(self) -> Ipv4Addr {
        self.at(1)
    }

    /// The gateway's last octet (1 in a /24, 9 in 10.0.0.8/29).
    #[must_use]
    pub fn gateway_octet(self) -> u8 {
        let [.., last] = self.gateway().octets();
        last
    }

    /// The controller's address: the second-to-last usable one (e.g. .253 in a /24), where
    /// environment-level provisioning runs from.
    #[must_use]
    pub fn controller(self) -> Ipv4Addr {
        self.at(self.size() - 3)
    }

    /// The address of the `i`th tool (0-based): just below the controller (.252, .251, .250 in
    /// a /24), reserved for tools when the spec has any. `None` past the block's host addresses,
    /// where the tool would take the gateway's or the network's address.
    #[must_use]
    pub fn tool(self, i: usize) -> Option<Ipv4Addr> {
        let below = u32::try_from(i).ok()?;
        let offset = (self.size() - 4).checked_sub(below).filter(|o| *o >= 2)?;
        Some(self.at(offset))
    }

    /// The router's address: the last usable address of the block (e.g. .254 in a /24).
    #[must_use]
    pub fn router(self) -> Ipv4Addr {
        self.at(self.size() - 2)
    }
}

impl fmt::Display for NetworkBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(text: &str) -> NetworkBlock {
        NetworkBlock::parse(text).unwrap()
    }

    fn ip(text: &str) -> Ipv4Addr {
        text.parse().unwrap()
    }

    #[test]
    fn a_24_reserves_its_first_and_last_addresses() {
        let lan = block("10.9.0.0/24");
        assert_eq!((lan.gateway(), lan.gateway_octet()), (ip("10.9.0.1"), 1));
        assert_eq!((lan.controller(), lan.router()), (ip("10.9.0.253"), ip("10.9.0.254")));
        assert_eq!((lan.tool(0), lan.tool(2)), (Some(ip("10.9.0.252")), Some(ip("10.9.0.250"))));
        assert_eq!(
            (lan.host(2), lan.host(100), lan.host(252)),
            (Some(ip("10.9.0.2")), Some(ip("10.9.0.100")), Some(ip("10.9.0.252")))
        );
        for reserved in [0, 1, 253, 254, 255] {
            assert_eq!(lan.host(reserved), None, "{reserved}");
        }
    }

    #[test]
    fn a_29_inside_a_24_counts_from_its_own_base() {
        let small = block("10.0.0.8/29");
        assert_eq!((small.gateway(), small.gateway_octet()), (ip("10.0.0.9"), 9));
        assert_eq!(
            (small.controller(), small.router(), small.tool(0)),
            (ip("10.0.0.13"), ip("10.0.0.14"), Some(ip("10.0.0.12")))
        );
        assert_eq!((small.host(10), small.host(12)), (Some(ip("10.0.0.10")), Some(ip("10.0.0.12"))));
        for outside in [8, 9, 13, 14, 15, 16, 7] {
            assert_eq!(small.host(outside), None, "{outside}");
        }
    }

    #[test]
    fn a_tool_past_the_blocks_host_addresses_has_none() {
        let small = block("10.0.0.8/29");
        assert_eq!(small.tool(2), Some(ip("10.0.0.10")), "the lowest host address");
        for past in [3, 4, usize::MAX] {
            assert_eq!(small.tool(past), None, "{past}: the gateway's, the network's, or below the block");
        }
    }

    #[test]
    fn only_a_24_to_a_29_is_a_networks_block() {
        for size in ["10.0.0.0/23", "10.0.0.0/16", "10.0.0.0/30", "10.0.0.0/32", "0.0.0.0/0"] {
            assert_eq!(NetworkBlock::parse(size), None, "{size}");
        }
        assert_eq!(NetworkBlock::new(Cidr::ALL), None, "every address");
        assert_eq!(block("10.0.0.0/25").host(2), Some(ip("10.0.0.2")));
        assert_eq!(block("10.0.0.0/25").to_string(), "10.0.0.0/25");
    }

    #[test]
    fn the_block_enclosing_an_address_is_a_networks_size() {
        let at = ip("10.9.3.77");
        assert_eq!(NetworkBlock::enclosing(at, 28), block("10.9.3.64/28"));
        assert_eq!(NetworkBlock::enclosing(at, 0), block("10.9.3.0/24"));
        assert_eq!(NetworkBlock::enclosing(at, 32), block("10.9.3.72/29"));
    }

    #[test]
    fn every_reserved_address_of_the_last_blocks_lies_in_them() {
        for text in ["255.255.255.0/24", "255.255.255.248/29"] {
            let b = block(text);
            for a in [b.gateway(), b.controller(), b.router()].into_iter().chain(b.tool(0)) {
                assert!(b.cidr().contains(Cidr::enclosing(a, 32)), "{a} outside {text}");
            }
        }
    }
}
