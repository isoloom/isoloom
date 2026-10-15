//! IPv4 blocks: parsing `a.b.c.d/len`, the addresses Isoloom reserves in a block, and where
//! each network lands on the Docker target.

use std::fmt;
use std::net::Ipv4Addr;
use std::str::FromStr;

use crate::Error;
use crate::model::Spec;

/// An IPv4 block, parsed from `a.b.c.d/len` and written back the same way (`Display`, `FromStr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    /// The network address, as a number.
    pub base: u32,
    /// The prefix length.
    pub len: u8,
}

impl Cidr {
    /// The block written `a.b.c.d/len`, when it is one: the address must be the network
    /// address itself.
    #[must_use]
    pub fn parse(s: &str) -> Option<Cidr> {
        let (ip, len) = s.split_once('/')?;
        let ip: Ipv4Addr = ip.parse().ok()?;
        let len: u8 = len.parse().ok()?;
        if len > 32 {
            return None;
        }
        let mask = Self::mask(len);
        let base = u32::from(ip);
        // The address must be the network address itself (10.20.0.0/24, not 10.20.0.5/24).
        (base & mask == base).then_some(Cidr { base, len })
    }

    fn mask(len: u8) -> u32 {
        if len == 0 { 0 } else { u32::MAX << (32 - len) }
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

/// Where the Docker target keeps its networks: 172.16.0.0/12 and 192.168.0.0/16 clash with
/// Docker's own pools, Docker Desktop's network and home LANs.
pub const DOCKER_BLOCK: Cidr = Cidr { base: 0x0a00_0000, len: 8 };

/// Each network's block on the Docker target: its `docker.cidr` when set; its `cidr` when it's
/// inside 10.0.0.0/8 or outside the environment (`external`, `bridged`); otherwise moved there
/// automatically, keeping its size (so each machine keeps its last octet): 192.168.X.0 -> 10.192.X.0, 172.N.X.0 -> 10.N.X.0, or the next free
/// block when that one is taken. Validated specs only: a network whose block doesn't parse is
/// left out (validation reports it).
#[must_use]
pub fn docker_cidrs(spec: &Spec) -> Vec<(String, Cidr)> {
    let mut fixed: Vec<(String, Cidr)> = Vec::new();
    let mut moved: Vec<(String, Cidr)> = Vec::new();
    for (name, n) in &spec.networks {
        let Some(c) = Cidr::parse(&n.cidr) else { continue };
        match n.docker.as_ref().and_then(|d| Cidr::parse(&d.cidr)) {
            Some(d) => fixed.push((name.clone(), d)),
            // A network outside the environment keeps its own block: it exists as it is.
            None if DOCKER_BLOCK.contains(c) || n.is_outside() => fixed.push((name.clone(), c)),
            None => moved.push((name.clone(), c)),
        }
    }
    let mut taken: Vec<Cidr> = fixed.iter().map(|(_, c)| *c).collect();
    let mut out: indexmap::IndexMap<String, Cidr> = fixed.into_iter().collect();
    for (name, c) in moved {
        let pick = moved_block(c, &taken);
        taken.push(pick);
        out.insert(name, pick);
    }
    spec.networks.keys().filter_map(|n| Some((n.clone(), *out.get(n)?))).collect()
}

/// Where a block outside 10.0.0.0/8 moves on Docker: the mapped block, else the next free one
/// of the same size.
fn moved_block(block: Cidr, taken: &[Cidr]) -> Cidr {
    let [first_octet, second, third, fourth] = Ipv4Addr::from(block.base).octets();
    let first = if first_octet == 192 {
        u32::from_be_bytes([10, 192, third, fourth])
    } else {
        u32::from_be_bytes([10, second, third, fourth])
    };
    let size = 1u32 << (32 - block.len);
    // The mapped block, else the next free one of the same size scanning 10.240.0.0 upward
    // within 10.0.0.0/8. A bounded, checked successor sequence: it can't overflow, and it
    // ends at the top of the block so `find` terminates. Running out needs thousands of
    // auto-moved networks: the mapped block is kept then, and validation reports the overlap.
    let scan = std::iter::successors(Some(0x0af0_0000u32), move |&b| b.checked_add(size).filter(|&n| n <= 0x0aff_ffff));
    std::iter::once(first)
        .chain(scan)
        .map(|base| Cidr { base, len: block.len })
        .find(|cand| DOCKER_BLOCK.contains(*cand) && !taken.iter().any(|t| t.overlaps(*cand)))
        .unwrap_or(Cidr { base: first, len: block.len })
}

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
    fn a_block_reads_and_writes_as_its_text() {
        let lan: Cidr = "10.20.0.0/24".parse().unwrap();
        assert_eq!(lan, Cidr { base: 0x0a14_0000, len: 24 });
        assert_eq!(lan.to_string(), "10.20.0.0/24");
        assert_eq!(Cidr { base: 0, len: 0 }.to_string(), "0.0.0.0/0");
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

    /// The Docker blocks of the networks `networks` (YAML) describes.
    fn docker(networks: &str) -> Vec<(String, String)> {
        let spec = crate::testing::model(&format!("version: 1\nname: t\nnetworks:\n{networks}")).unwrap();
        docker_cidrs(&spec)
            .into_iter()
            .map(|(n, c)| (n, format!("{}/{}", Ipv4Addr::from(c.base), c.len)))
            .collect()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect()
    }

    #[test]
    fn a_block_outside_10_moves_into_it_keeping_its_octets() {
        assert_eq!(
            docker("  home: { cidr: 192.168.5.0/24 }\n  dmz: { cidr: 172.20.3.0/24 }\n  lab: { cidr: 10.7.0.0/24 }\n"),
            pairs(&[("home", "10.192.5.0/24"), ("dmz", "10.20.3.0/24"), ("lab", "10.7.0.0/24")])
        );
    }

    #[test]
    fn a_networks_own_docker_block_and_an_outside_network_stay_as_written() {
        assert_eq!(
            docker("  home: { cidr: 192.168.5.0/24, docker: { cidr: 10.50.0.0/24 } }\n  corp: { cidr: 192.168.50.0/24, external: corp-net }\n"),
            pairs(&[("home", "10.50.0.0/24"), ("corp", "192.168.50.0/24")])
        );
    }

    #[test]
    fn a_taken_block_moves_to_the_next_free_one_from_10_240() {
        assert_eq!(
            docker("  lab: { cidr: 10.192.5.0/24 }\n  home: { cidr: 192.168.5.0/24 }\n  also: { cidr: 192.168.5.0/24 }\n"),
            pairs(&[("lab", "10.192.5.0/24"), ("home", "10.240.0.0/24"), ("also", "10.240.1.0/24")])
        );
        assert_eq!(
            docker("  lab: { cidr: 10.240.0.0/23 }\n  dmz: { cidr: 172.240.0.0/24 }\n"),
            pairs(&[("lab", "10.240.0.0/23"), ("dmz", "10.240.2.0/24")])
        );
    }
}
