//! IPv6: each network's IPv6 block, each machine's IPv6 address (derived from its octet, or
//! left to the target), the router's IPv6 addresses and the IPv6 routes through it. Pure
//! functions of a validated spec, beside their IPv4 counterparts in `addressing` and `routing`.

use std::fmt;
use std::net::Ipv6Addr;

use indexmap::IndexMap;
use isoloom_spec::model::{Ipv6Addresses, Machine, Spec};
use isoloom_spec::validate::Cidr6;

use crate::addressing::cidr;
use crate::requirements::Requirement;
use crate::routing;

/// A network's IPv6 side, as the target lays it out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ipv6Plan {
    /// Its /64 block.
    pub cidr: String,
    /// The host's address on it (Docker's bridge, the hypervisor): `::1`.
    pub gateway: Ipv6Addr,
    /// Whether machines get the address derived from their octet, or the target's.
    pub addresses: Ipv6Addresses,
    /// Whether the network carries no IPv4.
    pub only: bool,
}

/// An IPv6 route to a block a machine isn't on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route6 {
    /// The block.
    pub cidr: String,
    /// The next hop: the router's IPv6 address on a network the machine is on.
    pub via: Ipv6Addr,
}

/// A network's IPv6 block, when it has one that parses.
#[must_use]
pub fn block(spec: &Spec, network: &str) -> Option<Cidr6> {
    spec.networks.get(network)?.ipv6.as_ref().and_then(|v| Cidr6::parse(&v.cidr))
}

/// A network's IPv6 side, when it has one.
#[must_use]
pub fn network(spec: &Spec, network: &str) -> Option<Ipv6Plan> {
    let v6 = spec.networks.get(network)?.ipv6.as_ref()?;
    let c = Cidr6::parse(&v6.cidr)?;
    Some(Ipv6Plan {
        cidr: c.text(),
        gateway: c.gateway(),
        addresses: v6.addresses,
        only: v6.only,
    })
}

/// Whether the network carries no IPv4.
#[must_use]
pub fn only(spec: &Spec, network: &str) -> bool {
    spec.networks.get(network).and_then(|n| n.ipv6.as_ref()).is_some_and(|v| v.only)
}

/// The IPv6 address a machine at `octet` has on a network, when the network is IPv6 and its
/// addresses are derived (with `auto`, the target picks it).
#[must_use]
pub fn address(spec: &Spec, network: &str, octet: u8) -> Option<Ipv6Addr> {
    let v6 = spec.networks.get(network)?.ipv6.as_ref()?;
    if v6.addresses != Ipv6Addresses::Static {
        return None;
    }
    block(spec, network).map(|c| c.host(octet))
}

/// A machine's derived IPv6 addresses, by network, in its order.
#[must_use]
pub fn addresses(spec: &Spec, m: &Machine) -> IndexMap<String, Ipv6Addr> {
    m.networks.iter().filter_map(|(n, o)| Some((n.clone(), address(spec, n, *o)?))).collect()
}

/// The router's IPv6 address on a network it routes: the one its IPv4 octet gives (`::254` in
/// a /24), fixed whatever the network's `addresses`.
#[must_use]
pub fn router_address(spec: &Spec, network: &str) -> Option<Ipv6Addr> {
    let octet = cidr(spec, network).router().octets()[3];
    block(spec, network).map(|c| c.host(octet))
}

/// The router's IPv6 addresses, on each IPv6 network it routes (empty without a router).
#[must_use]
pub fn router(spec: &Spec) -> IndexMap<String, Ipv6Addr> {
    if !routing::needed(spec) {
        return IndexMap::new();
    }
    routing::networks(spec).filter_map(|n| Some((n.clone(), router_address(spec, n)?))).collect()
}

/// A machine's IPv6 routes: each IPv6 block it isn't on, through the router's IPv6 address on
/// its first IPv6 network the router is on. Gateway networks carry no IPv6 (validated), so the
/// router is the only next hop.
#[must_use]
pub fn routes(spec: &Spec, m: &Machine) -> Vec<Route6> {
    let router = router(spec);
    let Some(via) = m.networks.keys().find_map(|n| router.get(n)) else {
        return Vec::new();
    };
    router
        .keys()
        .filter(|n| !m.networks.contains_key(*n))
        .filter_map(|n| {
            Some(Route6 {
                cidr: block(spec, n)?.text(),
                via: *via,
            })
        })
        .collect()
}

/// A cloud's network routes between its subnets, IPv6 included, and has no Isoloom router:
/// the IPv6 step keeps the addresses and drops the routes.
pub(crate) fn cloud_routes(steps: &mut [crate::lifecycle::Step]) {
    for s in steps {
        if let crate::lifecycle::Step::Ipv6 { routes, .. } = s {
            routes.clear();
        }
    }
}

/// Why a target without IPv6-only networks refuses network `network`.
pub(crate) fn no_ipv6_only(f: &mut fmt::Formatter<'_>, network: &str) -> fmt::Result {
    write!(
        f,
        "network `{network}` is IPv6-only (`ipv6.only`): this target needs IPv4 on every network (its router, its set-up, its transport)"
    )
}

/// Why a target that hands out no IPv6 addresses refuses network `network`'s `auto` ones.
pub(crate) fn no_auto_addresses(f: &mut fmt::Formatter<'_>, network: &str) -> fmt::Result {
    write!(
        f,
        "network `{network}` leaves IPv6 addresses to the target (`ipv6.addresses: auto`), and nothing on this target hands them out: use `static`"
    )
}

/// What the IPv6 networks ask of a backend: IPv6 at all, IPv6-only networks, addresses the
/// target hands out. At most one of each kind, naming the first network that asks.
#[must_use]
pub fn requirements(spec: &Spec) -> Vec<Requirement> {
    let first = |pick: fn(&isoloom_spec::model::Ipv6) -> bool| {
        spec.networks
            .iter()
            .find(|(_, n)| n.ipv6.as_ref().is_some_and(pick))
            .map(|(name, _)| name.clone())
    };
    let mut out = Vec::new();
    if let Some(network) = first(|_| true) {
        out.push(Requirement::Ipv6 { network });
    }
    if let Some(network) = first(|v| v.only) {
        out.push(Requirement::Ipv6Only { network });
    }
    if let Some(network) = first(|v| v.addresses == Ipv6Addresses::Auto) {
        out.push(Requirement::Ipv6Auto { network });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::Step;

    fn spec() -> Spec {
        crate::testing::spec(
            "version: 1
name: v6
networks:
  a: { cidr: 10.1.0.0/24, ipv6: { cidr: \"fd00:1::/64\" } }
  b: { cidr: 10.2.0.0/24, ipv6: { cidr: \"fd00:2::/64\", addresses: auto } }
  c: { cidr: 10.3.0.0/24 }
reach: [{ from: a, to: b }]
machines:
  x: { networks: { a: 10 }, app: { image: alpine } }
  y: { networks: { b: 20, c: 5 }, app: { image: alpine } }
",
        )
    }

    #[test]
    fn only_an_ipv6_only_network_carries_no_ipv4() {
        let s = crate::testing::spec(
            "version: 1\nname: v6\nnetworks:\n  a: { cidr: 10.1.0.0/24, ipv6: { cidr: \"fd00:1::/64\", only: true } }\n  b: { cidr: 10.2.0.0/24, ipv6: { cidr: \"fd00:2::/64\" } }\nmachines: {}\n",
        );
        assert!(only(&s, "a") && !only(&s, "b") && !only(&s, "nowhere"));
        let plan = network(&s, "a").expect("an IPv6 side");
        assert_eq!(
            (plan.cidr.as_str(), plan.gateway.to_string(), plan.only),
            ("fd00:1::/64", "fd00:1::1".to_owned(), true)
        );
        assert_eq!(network(&spec(), "c"), None);
        let kinds: Vec<Requirement> = requirements(&s);
        assert_eq!(
            kinds,
            [Requirement::Ipv6 { network: "a".into() }, Requirement::Ipv6Only { network: "a".into() }]
        );
        assert_eq!(requirements(&spec()).last(), Some(&Requirement::Ipv6Auto { network: "b".into() }));
    }

    #[test]
    fn a_cloud_keeps_the_addresses_and_drops_the_routes() {
        let mut steps = vec![
            Step::Ipv6 {
                addresses: Vec::new(),
                routes: routes(&spec(), &spec().machines["x"]),
            },
            Step::Ready,
        ];
        cloud_routes(&mut steps);
        assert_eq!(
            steps,
            [
                Step::Ipv6 {
                    addresses: Vec::new(),
                    routes: Vec::new()
                },
                Step::Ready
            ]
        );
    }

    #[test]
    fn derived_addresses_only_where_static() {
        let s = spec();
        assert_eq!(addresses(&s, &s.machines["x"])["a"].to_string(), "fd00:1::10");
        assert!(addresses(&s, &s.machines["y"]).is_empty());
    }

    #[test]
    fn routes_go_through_the_router_on_ipv6_networks() {
        let s = spec();
        assert_eq!(router(&s)["b"].to_string(), "fd00:2::254");
        let r = routes(&s, &s.machines["x"]);
        assert_eq!(
            r,
            vec![Route6 {
                cidr: "fd00:2::/64".into(),
                via: "fd00:1::254".parse().unwrap()
            }]
        );
        assert_eq!(routes(&s, &s.machines["y"])[0].cidr, "fd00:1::/64");
    }
}
