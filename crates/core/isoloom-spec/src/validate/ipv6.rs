//! `networks.*.ipv6`: a /64 block of its own, and what an IPv6-only network can't have (what
//! goes through Isoloom's IPv4 router or NAT).

use super::Problems;
use super::cidr6::Cidr6;
use crate::model::{Network, Spec};

/// Each network's IPv6 block: valid, not shared, and an IPv6-only network kept to what works
/// without IPv4.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    let mut blocks: Vec<(&str, Cidr6)> = Vec::new();
    for (name, net) in &spec.networks {
        let Some(v6) = &net.ipv6 else { continue };
        let at = format!("networks.{name}.ipv6");
        match Cidr6::parse(&v6.cidr) {
            Some(c) => {
                if let Some((other, _)) = blocks.iter().find(|(_, b)| b.overlaps(c)) {
                    p.add(format!("{at}.cidr"), format!("network `{other}` has this block already"));
                }
                blocks.push((name, c));
            }
            None => p.add(
                format!("{at}.cidr"),
                format!("`{}` isn't an IPv6 /64 network address, unique local (fd00:20::/64) or global", v6.cidr),
            ),
        }
        if net.gateway.is_some() {
            p.add(
                &at,
                "a network routed by a `gateway` machine carries IPv4 only for now: drop `ipv6` or the `gateway`",
            );
        }
        if v6.only {
            only(spec, name, net, p);
        }
    }
}

/// An IPv6-only network: no `reach`, `tc` or internet, since Isoloom's router and NAT are IPv4.
fn only(spec: &Spec, name: &str, net: &Network, p: &mut Problems) {
    let at = format!("networks.{name}.ipv6.only");
    if spec.reach.iter().any(|r| r.from == name || r.to == name) {
        p.add(
            &at,
            format!("`reach` rules go through Isoloom's IPv4 router: drop the rules naming `{name}`, or make it dual-stack (`only: false`)"),
        );
    }
    if net.tc.is_some() {
        p.add(&at, "`tc` applies on Isoloom's IPv4 router: drop `tc`, or make the network dual-stack");
    }
    if net.internet {
        p.add(&at, "an IPv6-only network has no way out (Isoloom's NAT is IPv4): set `internet: false`");
    }
}
