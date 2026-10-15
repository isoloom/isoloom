//! IPv6 blocks: parsing `prefix::/64`, the address a machine's octet gives it, and the
//! addresses Isoloom keeps for itself (the gateway, its router).

use std::net::Ipv6Addr;

/// The prefix length of every IPv6 block: a LAN's, the one SLAAC and the clouds' subnets use.
pub const PREFIX: u8 = 64;

/// An IPv6 /64 block, parsed from `prefix::/64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr6 {
    /// The network address, as a number.
    pub base: u128,
}

impl Cidr6 {
    /// The block written `prefix::/64`, when it is one: the address must be the network
    /// address itself, unique local (`fc00::/7`) or global unicast (`2000::/3`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Cidr6> {
        let (ip, len) = s.split_once('/')?;
        let ip: Ipv6Addr = ip.parse().ok()?;
        if len.parse::<u8>().ok()? != PREFIX {
            return None;
        }
        let base = u128::from(ip);
        let host_bits = (1u128 << (128 - PREFIX)) - 1;
        let unique_local = base >> 121 == 0x7e;
        let global = base >> 125 == 1;
        (base & host_bits == 0 && (unique_local || global)).then_some(Cidr6 { base })
    }

    /// Whether the two blocks are the same: two /64 blocks share an address only then.
    #[must_use]
    pub fn overlaps(self, other: Cidr6) -> bool {
        self.base == other.base
    }

    /// The machine at IPv4 octet `octet`: the octet's decimal digits as the last group (10 is
    /// `::10`, 254 is `::254`), so the two addresses of a dual-stack machine read alike.
    #[must_use]
    pub fn host(self, octet: u8) -> Ipv6Addr {
        Ipv6Addr::from(self.base | interface_id(octet))
    }

    /// The gateway (the host's bridge, the first address): `::1`.
    #[must_use]
    pub fn gateway(self) -> Ipv6Addr {
        Ipv6Addr::from(self.base | 1)
    }

    /// The block written back, `prefix::/64`.
    #[must_use]
    pub fn text(self) -> String {
        format!("{}/{PREFIX}", Ipv6Addr::from(self.base))
    }
}

/// An octet's decimal digits read as hexadecimal: 10 is 0x10, 254 is 0x254.
fn interface_id(octet: u8) -> u128 {
    u128::from(octet / 100) << 8 | u128::from(octet / 10 % 10) << 4 | u128::from(octet % 10)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn a_machine_reads_like_its_ipv4_address() {
        let c = Cidr6::parse("fd00:20::/64").unwrap();
        assert_eq!(c.host(10).to_string(), "fd00:20::10");
        assert_eq!(c.host(254).to_string(), "fd00:20::254");
        assert_eq!(c.gateway().to_string(), "fd00:20::1");
        assert_eq!(c.text(), "fd00:20::/64");
    }

    #[test]
    fn two_64s_overlap_only_when_they_are_the_same_block() {
        let a = Cidr6::parse("fd00:20::/64").unwrap();
        let b = Cidr6::parse("fd00:21::/64").unwrap();
        assert!(a.overlaps(a) && !a.overlaps(b) && !b.overlaps(a));
    }

    #[test]
    fn only_unicast_network_addresses_of_a_64() {
        assert!(Cidr6::parse("2001:db8:1:2::/64").is_some());
        for bad in [
            "fd00::1/64",
            "fd00::/48",
            "fe80::/64",
            "ff02::/64",
            "::/64",
            "10.0.0.0/24",
            "fd00::",
            "fd00::/x",
        ] {
            assert_eq!(Cidr6::parse(bad), None, "{bad}");
        }
    }

    proptest! {
        /// Two octets never share an address, and each stays in its block, off the gateway.
        #[test]
        fn hosts_are_distinct_and_inside(prefix in any::<u64>(), a in 2u8.., b in 2u8..) {
            let c = Cidr6 { base: u128::from(prefix) << 64 };
            let (ha, hb) = (u128::from(c.host(a)), u128::from(c.host(b)));
            prop_assert_eq!(ha == hb, a == b);
            prop_assert_eq!(ha >> 64, u128::from(prefix));
            prop_assert_ne!(c.host(a), c.gateway());
        }

        /// A parsed block is written back as it was read.
        #[test]
        fn parse_round_trips(groups in proptest::array::uniform4(any::<u16>())) {
            let [a, b, c, d] = groups;
            let ip = Ipv6Addr::new(0xfd00 | (a & 0xff), b, c, d, 0, 0, 0, 0);
            let text = format!("{ip}/64");
            let parsed = Cidr6::parse(&text).unwrap();
            prop_assert_eq!(parsed.text(), text);
        }

        /// Any address with host bits set is refused: only the network address names a block.
        #[test]
        fn host_bits_are_refused(prefix in any::<u16>(), host in 1u64..) {
            let ip = Ipv6Addr::from(u128::from(0xfd00u16 | (prefix & 0xff)) << 112 | u128::from(host));
            prop_assert_eq!(Cidr6::parse(&format!("{ip}/64")), None);
        }
    }
}
