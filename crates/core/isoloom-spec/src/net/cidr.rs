//! IPv4 blocks: parsing `a.b.c.d/len`, containment and overlap. The addresses Isoloom reserves
//! are a network's block's ([`NetworkBlock`](super::NetworkBlock)).

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
