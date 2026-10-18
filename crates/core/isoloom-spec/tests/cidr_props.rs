//! Properties of IPv4 blocks ([`Cidr`]): parsing accepts exactly the network addresses, and
//! containment, overlap, host and gateway addresses agree with the block's arithmetic.

use std::net::Ipv4Addr;

use isoloom_spec::validate::Cidr;
use proptest::prelude::*;

fn mask(len: u8) -> u32 {
    if len == 0 { 0 } else { u32::MAX << (32 - len) }
}

/// Any valid block: an address cut to its prefix length.
fn block() -> impl Strategy<Value = (u32, u8)> {
    (any::<u32>(), 0u8..=32).prop_map(|(ip, len)| (ip & mask(len), len))
}

fn text(base: u32, len: u8) -> String {
    format!("{}/{len}", Ipv4Addr::from(base))
}

/// The block `base/len`, which must parse.
fn parsed(base: u32, len: u8) -> Result<Cidr, TestCaseError> {
    Cidr::parse(&text(base, len)).ok_or_else(|| TestCaseError::fail(format!("{} should parse", text(base, len))))
}

proptest! {
    #[test]
    fn a_network_address_parses_to_itself((base, len) in block()) {
        let c = parsed(base, len)?;
        prop_assert_eq!((c.base, c.len), (base, len));
    }

    #[test]
    fn a_host_address_is_not_a_block(ip in any::<u32>(), len in 0u8..32) {
        prop_assume!(ip & !mask(len) != 0);
        prop_assert_eq!(Cidr::parse(&text(ip, len)), None);
    }

    #[test]
    fn a_prefix_over_32_or_garbage_is_refused(ip in any::<u32>(), len in 33u16..1000, junk in "[^0-9./]{1,8}") {
        prop_assert_eq!(Cidr::parse(&format!("{}/{len}", Ipv4Addr::from(ip))), None);
        prop_assert_eq!(Cidr::parse(&junk), None);
    }

    #[test]
    fn containment_matches_the_masks((a, la) in block(), (b, lb) in block()) {
        let (x, y) = (parsed(a, la)?, parsed(b, lb)?);
        prop_assert!(x.contains(x));
        let expected = la <= lb && b & mask(la) == a;
        prop_assert_eq!(x.contains(y), expected);
        prop_assert_eq!(x.overlaps(y), y.overlaps(x));
        prop_assert_eq!(x.overlaps(y), x.contains(y) || y.contains(x));
    }

    #[test]
    fn hosts_and_the_gateway_stay_inside_the_block((base, len) in (any::<u32>(), 24u8..=29).prop_map(|(ip, len)| (ip & mask(len), len)), octet in any::<u8>()) {
        let c = parsed(base, len)?;
        let gateway = u32::from(c.gateway());
        prop_assert_eq!(gateway & mask(len), base);
        if let Some(host) = c.host(octet) {
            let h = u32::from(host);
            prop_assert_eq!(h & mask(len), base, "host {} outside {}", host, text(base, len));
            prop_assert!(h > gateway, "host {} is the network or gateway address", host);
            let broadcast = base | !mask(len);
            prop_assert!(h < broadcast - 2, "host {} takes a reserved address", host);
        }
    }
}
