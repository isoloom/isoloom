//! IPv4 blocks: parsing `a.b.c.d/len`, and the addresses Isoloom reserves in a block.

use std::fmt;
use std::net::Ipv4Addr;
use std::str::FromStr;

use crate::Error;

/// An IPv4 block, parsed from `a.b.c.d/len` and written back the same way (`Display`, `FromStr`).
/// It holds only a block: a prefix length of 32 at most, and the network address itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    /// The network address, as a number.
    base: u32,
    /// The prefix length.
    len: u8,
}

impl Cidr {
    /// Every address: `0.0.0.0/0`.
    pub const ALL: Cidr = Cidr { base: 0, len: 0 };

    /// The block of network address `base` and prefix length `len`, when it is one: `len` 32 at
    /// most, and `base` the network address itself (no host bit set).
    #[must_use]
    pub const fn new(base: u32, len: u8) -> Option<Cidr> {
        if len <= 32 && base & Self::mask(len) == base {
            Some(Cidr { base, len })
        } else {
            None
        }
    }

    /// Its network address, as a number.
    #[must_use]
    pub const fn base(self) -> u32 {
        self.base
    }

    /// Its network address.
    #[must_use]
    pub fn address(self) -> Ipv4Addr {
        Ipv4Addr::from(self.base)
    }

    /// Its prefix length.
    #[must_use]
    pub const fn prefix(self) -> u8 {
        self.len
    }

    /// Its two halves, one bit longer, lower first; none for a single address (a /32).
    #[must_use]
    pub const fn halves(self) -> Option<(Cidr, Cidr)> {
        if self.len >= 32 {
            return None;
        }
        let len = self.len + 1;
        Some((
            Cidr { base: self.base, len },
            Cidr {
                base: self.base | (1 << (32 - len)),
                len,
            },
        ))
    }

    /// The block written `a.b.c.d/len`, when it is one: the address must be the network
    /// address itself.
    #[must_use]
    pub fn parse(s: &str) -> Option<Cidr> {
        let (ip, len) = s.split_once('/')?;
        let ip: Ipv4Addr = ip.parse().ok()?;
        // The address must be the network address itself (10.20.0.0/24, not 10.20.0.5/24).
        Cidr::new(u32::from(ip), len.parse().ok()?)
    }

    /// The block of `len` bits (32 at most) that holds `address`: `10.9.3.0/24` for
    /// `10.9.3.10` and 24.
    #[must_use]
    pub const fn enclosing(address: Ipv4Addr, len: u8) -> Cidr {
        let len = if len > 32 { 32 } else { len };
        Cidr {
            base: address.to_bits() & Self::mask(len),
            len,
        }
    }

    /// The block's netmask: `255.255.255.0` for a /24.
    #[must_use]
    pub fn netmask(self) -> Ipv4Addr {
        Ipv4Addr::from(Self::mask(self.len))
    }

    /// The mask of a prefix of `len` bits; every bit past 32.
    const fn mask(len: u8) -> u32 {
        if len == 0 {
            0
        } else if len >= 32 {
            u32::MAX
        } else {
            u32::MAX << (32 - len)
        }
    }

    /// Whether `other` lies inside this block.
    #[must_use]
    pub fn contains(self, other: Cidr) -> bool {
        self.len <= other.len && other.base & Self::mask(self.len) == self.base
    }

    /// Whether the two blocks share an address.
    #[must_use]
    pub fn overlaps(self, other: Cidr) -> bool {
        self.contains(other) || other.contains(self)
    }

    /// The address with this last octet, if it is a usable host address in the block.
    /// Reserved on every target: `.1` (gateway) and the two last usable addresses (controller, router).
    #[must_use]
    pub fn host(self, last_octet: u8) -> Option<Ipv4Addr> {
        if self.len < 24 || self.len > 29 {
            return None;
        }
        let size = 1u32 << (32 - self.len);
        let offset = u32::from(last_octet) & 0xff;
        let addr = (self.base & !0xff) | offset;
        let first = self.base + 2; // .0 network, .1 gateway
        let last = self.base + size - 4; // the controller, the router (last usable) and broadcast excluded
        (addr >= first && addr <= last).then(|| Ipv4Addr::from(addr))
    }

    /// The gateway's address: the first usable address of the block (e.g. .1 in a /24).
    #[must_use]
    pub fn gateway(self) -> Ipv4Addr {
        Ipv4Addr::from(self.base + 1)
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
        Ipv4Addr::from(self.base + (1u32 << (32 - self.len)) - 3)
    }

    /// The address of the `i`th tool (0-based): just below the controller (.252, .251, .250 in
    /// a /24), reserved for tools when the spec has any.
    #[must_use]
    pub fn tool(self, i: usize) -> Ipv4Addr {
        let below = u32::try_from(i).unwrap_or(u32::MAX);
        Ipv4Addr::from((self.base + (1u32 << (32 - self.len)) - 4).saturating_sub(below))
    }

    /// The router's address: the last usable address of the block (e.g. .254 in a /24).
    #[must_use]
    pub fn router(self) -> Ipv4Addr {
        Ipv4Addr::from(self.base + (1u32 << (32 - self.len)) - 2)
    }
}

/// The private ranges a network can use (RFC 1918).
pub const PRIVATE: [Cidr; 3] = [
    Cidr { base: 0x0a00_0000, len: 8 },
    Cidr { base: 0xac10_0000, len: 12 },
    Cidr { base: 0xc0a8_0000, len: 16 },
];

impl fmt::Display for Cidr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", Ipv4Addr::from(self.base), self.len)
    }
}

impl FromStr for Cidr {
    type Err = Error;

    fn from_str(s: &str) -> Result<Cidr, Error> {
        Cidr::parse(s).ok_or_else(|| Error::Cidr(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(text: &str) -> Cidr {
        Cidr::parse(text).unwrap()
    }

    fn ip(text: &str) -> Ipv4Addr {
        text.parse().unwrap()
    }

    #[test]
    fn a_blocks_netmask_and_the_block_enclosing_an_address() {
        assert_eq!(block("10.20.0.0/24").netmask(), ip("255.255.255.0"));
        assert_eq!(block("10.20.0.64/28").netmask(), ip("255.255.255.240"));
        assert_eq!(block("0.0.0.0/0").netmask(), Ipv4Addr::UNSPECIFIED);
        assert_eq!(block("10.20.0.5/32").netmask(), Ipv4Addr::BROADCAST);
        assert_eq!(Cidr::enclosing(ip("10.9.3.77"), 28), block("10.9.3.64/28"));
        assert_eq!(Cidr::enclosing(ip("10.9.3.77"), 0), block("0.0.0.0/0"));
        assert_eq!(Cidr::enclosing(ip("10.9.3.77"), 40), block("10.9.3.77/32"));
    }

    #[test]
    fn a_block_reads_and_writes_as_its_text() {
        let lan: Cidr = "10.20.0.0/24".parse().unwrap();
        assert_eq!(lan, Cidr::new(0x0a14_0000, 24).unwrap());
        assert_eq!(lan.to_string(), "10.20.0.0/24");
        assert_eq!(Cidr::ALL.to_string(), "0.0.0.0/0");
        assert_eq!(Cidr::new(0x0a14_0005, 24), None, "a host address");
        assert_eq!(Cidr::new(0x0a14_0000, 33), None, "past 32 bits");
        let host = "10.20.0.5/24".parse::<Cidr>().unwrap_err();
        assert_eq!(
            host.to_string(),
            "`10.20.0.5/24` isn't an IPv4 block: write the network's own address, `/` and the prefix length (`10.20.0.0/24`)"
        );
    }

    #[test]
    fn a_24_reserves_its_first_and_last_addresses() {
        let lan = block("10.9.0.0/24");
        assert_eq!((lan.gateway(), lan.gateway_octet()), (ip("10.9.0.1"), 1));
        assert_eq!((lan.controller(), lan.router()), (ip("10.9.0.253"), ip("10.9.0.254")));
        assert_eq!((lan.tool(0), lan.tool(2)), (ip("10.9.0.252"), ip("10.9.0.250")));
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
            (ip("10.0.0.13"), ip("10.0.0.14"), ip("10.0.0.12"))
        );
        assert_eq!((small.host(10), small.host(12)), (Some(ip("10.0.0.10")), Some(ip("10.0.0.12"))));
        for outside in [8, 9, 13, 14, 15, 16, 7] {
            assert_eq!(small.host(outside), None, "{outside}");
        }
    }

    #[test]
    fn a_tool_below_the_block_saturates_at_its_base() {
        assert_eq!(block("10.0.0.8/29").tool(usize::MAX), ip("0.0.0.0"));
    }

    #[test]
    fn only_a_24_to_a_29_has_host_addresses() {
        for size in ["10.0.0.0/23", "10.0.0.0/16", "10.0.0.0/30", "10.0.0.0/32"] {
            assert_eq!(block(size).host(2), None, "{size}");
        }
        assert_eq!(block("10.0.0.0/25").host(2), Some(ip("10.0.0.2")));
    }
}

#[cfg(test)]
mod halves {
    use super::Cidr;

    #[test]
    fn a_block_halves_into_two_one_bit_longer() {
        let all = Cidr::ALL.halves().unwrap();
        assert_eq!((all.0.to_string(), all.1.to_string()), ("0.0.0.0/1".to_owned(), "128.0.0.0/1".to_owned()));
        let lan: Cidr = "10.9.0.0/24".parse().unwrap();
        let (low, high) = lan.halves().unwrap();
        assert_eq!((low.to_string(), high.to_string()), ("10.9.0.0/25".to_owned(), "10.9.0.128/25".to_owned()));
        assert_eq!("10.9.0.1/32".parse::<Cidr>().unwrap().halves(), None);
    }
}
