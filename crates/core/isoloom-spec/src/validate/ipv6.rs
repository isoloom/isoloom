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

#[cfg(test)]
mod tests {
    use crate::testing::{lab, problems};

    /// The problems of a lab whose second network, `v6`, is `network`, with `rest` after it.
    fn of(network: &str, rest: &str) -> Vec<String> {
        problems(&lab(&format!(
            "  v6: {network}\nmachines:\n  web: {{ networks: {{ lan: 10, v6: 10 }}, docker: {{ image: nginx }} }}\n{rest}"
        )))
    }

    #[test]
    fn a_dual_stack_network_has_its_own_64() {
        assert_eq!(of("{ cidr: 10.1.0.0/24, ipv6: { cidr: 'fd00:20::/64' } }", ""), Vec::<String>::new());
        assert_eq!(
            of("{ cidr: 10.1.0.0/24, ipv6: { cidr: 'fd00:20::5/64' } }", ""),
            ["networks.v6.ipv6.cidr: `fd00:20::5/64` isn't an IPv6 /64 network address, unique local (fd00:20::/64) or global"]
        );
    }

    #[test]
    fn two_networks_never_share_a_64() {
        let found = problems(&lab(
            "  a: { cidr: 10.1.0.0/24, ipv6: { cidr: 'fd00:20::/64' } }\n  b: { cidr: 10.2.0.0/24, ipv6: { cidr: 'fd00:20::/64' } }\nmachines:\n  web: { networks: { a: 10, b: 10 }, docker: { image: nginx } }\n",
        ));
        assert_eq!(found, ["networks.b.ipv6.cidr: network `a` has this block already"]);
    }

    #[test]
    fn a_gateway_network_carries_ipv4_only() {
        let found = problems(&lab(
            "  v6: { cidr: 10.1.0.0/24, gateway: gw, ipv6: { cidr: 'fd00:20::/64' } }\nmachines:\n  gw: { networks: { lan: 10, v6: 1 }, docker: { image: router } }\n",
        ));
        assert_eq!(
            found,
            ["networks.v6.ipv6: a network routed by a `gateway` machine carries IPv4 only for now: drop `ipv6` or the `gateway`"]
        );
    }

    #[test]
    fn an_ipv6_only_network_has_no_reach_tc_or_internet() {
        let only = "{ cidr: 10.1.0.0/24, internet: false, ipv6: { cidr: 'fd00:20::/64', only: true } }";
        assert_eq!(of(only, ""), Vec::<String>::new());
        let reach = "networks.v6.ipv6.only: `reach` rules go through Isoloom's IPv4 router: drop the rules naming `v6`, or make it dual-stack (`only: false`)";
        assert_eq!(of(only, "reach: [{ from: v6, to: lan }]"), [reach]);
        assert_eq!(of(only, "reach: [{ from: lan, to: v6 }]"), [reach]);
        assert_eq!(
            of(only, "reach: [{ from: lan, to: lan }]").iter().filter(|p| p.contains("ipv6.only")).count(),
            0
        );
        assert_eq!(
            of(
                "{ cidr: 10.1.0.0/24, internet: false, tc: { delay: 10ms }, ipv6: { cidr: 'fd00:20::/64', only: true } }",
                ""
            ),
            ["networks.v6.ipv6.only: `tc` applies on Isoloom's IPv4 router: drop `tc`, or make the network dual-stack"]
        );
        assert_eq!(
            of("{ cidr: 10.1.0.0/24, ipv6: { cidr: 'fd00:20::/64', only: true } }", ""),
            ["networks.v6.ipv6.only: an IPv6-only network has no way out (Isoloom's NAT is IPv4): set `internet: false`"]
        );
    }
}
