//! How traffic crosses networks: Isoloom's router (a machine on every network it routes, at the
//! last address, when `reach` rules need one) or a machine named as a network's `gateway`. Facts
//! only; `isoloom-render`'s `router` module renders them (routes as commands, the nftables
//! ruleset, netem) and re-exports these.

use std::net::Ipv4Addr;

use isoloom_spec::model::{Machine, Spec};

use crate::addressing::{address as machine_address, cidr};

/// The router machine's name (a spec can't use it: machine names are user names, and this
/// one is prefixed).
pub const NAME: &str = "isoloom-router";

/// Whether Isoloom routes this network with its own router: no machine is its gateway (a
/// network the spec doesn't declare has none).
#[must_use]
pub fn plain(spec: &Spec, network: &str) -> bool {
    spec.networks.get(network).is_none_or(|n| n.gateway.is_none())
}

/// Whether the spec needs a router: `reach` rules between networks it routes itself.
#[must_use]
pub fn needed(spec: &Spec) -> bool {
    spec.reach.iter().any(|r| plain(spec, &r.from) && plain(spec, &r.to))
}

/// Whether traffic crosses networks at all (Isoloom's router, or a machine as gateway).
#[must_use]
pub fn routed(spec: &Spec) -> bool {
    needed(spec) || spec.networks.values().any(|n| n.gateway.is_some())
}

/// The networks the router is on.
pub fn networks(spec: &Spec) -> impl Iterator<Item = &String> {
    spec.networks.keys().filter(|n| plain(spec, n))
}

/// The router's address on a network.
#[must_use]
pub fn address(spec: &Spec, network: &str) -> Ipv4Addr {
    cidr(spec, network).router()
}

/// Whether the machine is the gateway of one of its networks.
#[must_use]
pub fn is_gateway(spec: &Spec, name: &str) -> bool {
    spec.networks.values().any(|n| n.gateway.as_deref() == Some(name))
}

/// Where a machine sends traffic for everywhere else (the internet included): the gateway
/// of its first network routed by another machine.
#[must_use]
pub fn default_gateway(spec: &Spec, name: &str, m: &Machine) -> Option<Ipv4Addr> {
    m.networks
        .keys()
        .find(|n| spec.networks.get(*n).and_then(|net| net.gateway.as_deref()).is_some_and(|g| g != name))
        .map(|n| cidr(spec, n).gateway())
}

/// Routes a machine needs to the networks it isn't on: a network with a gateway through that
/// gateway (on a network they share), the others through the router (on the machine's first
/// network the router is on). Networks with neither route are left to the default route.
#[must_use]
pub fn routes(spec: &Spec, name: &str, m: &Machine) -> Vec<(String, Ipv4Addr)> {
    let via_router = needed(spec)
        .then(|| m.networks.keys().find(|n| plain(spec, n)))
        .flatten()
        .map(|n| address(spec, n));
    spec.networks
        .iter()
        .filter(|(n, _)| !m.networks.contains_key(*n))
        .filter_map(|(_, net)| {
            let via = match &net.gateway {
                Some(gw) if gw != name => {
                    let g = spec.machines.get(gw)?;
                    m.networks
                        .keys()
                        .find_map(|shared| g.networks.get(shared).map(|octet| machine_address(spec, shared, *octet)))
                }
                Some(_) => None,
                None => via_router,
            }?;
            Some((net.cidr.clone(), via))
        })
        .collect()
}

/// A machine is offline when none of its networks reaches the internet and no gateway
/// decides for it.
#[must_use]
pub fn offline(spec: &Spec, name: &str, m: &Machine) -> bool {
    !m.networks.keys().any(|n| spec.networks.get(n).is_some_and(|net| net.internet)) && default_gateway(spec, name, m).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    /// `fw` routes `inside`; Isoloom's router joins `front` and `back`, which `reach` connects.
    const LAB: &str = "version: 1
name: t
networks:
  front: { cidr: 10.1.0.0/24 }
  back: { cidr: 10.2.0.0/24, internet: false }
  inside: { cidr: 10.3.0.0/24, gateway: fw, internet: false }
reach: [{ from: front, to: back }]
machines:
  web: { networks: { front: 10 }, app: { image: nginx } }
  db: { networks: { back: 20 }, app: { image: postgres } }
  fw: { networks: { front: 1, inside: 1 }, app: { image: alpine } }
  pc: { networks: { inside: 30 }, app: { image: alpine } }
";

    #[test]
    fn the_router_joins_the_networks_no_machine_routes() {
        let spec = testing::spec(LAB);
        assert!(needed(&spec) && routed(&spec));
        assert_eq!(networks(&spec).collect::<Vec<_>>(), ["front", "back"]);
        assert!(plain(&spec, "front") && !plain(&spec, "inside"));
        assert_eq!(address(&spec, "back"), Ipv4Addr::new(10, 2, 0, 254));
    }

    #[test]
    fn a_gateway_routes_its_network_and_is_the_default_route_of_its_machines() {
        let spec = testing::spec(LAB);
        assert!(is_gateway(&spec, "fw") && !is_gateway(&spec, "web"));
        let pc = &spec.machines["pc"];
        assert_eq!(default_gateway(&spec, "pc", pc), Some(Ipv4Addr::new(10, 3, 0, 1)));
        assert_eq!(default_gateway(&spec, "fw", &spec.machines["fw"]), None, "a gateway isn't its own");
        assert!(!offline(&spec, "pc", pc), "its gateway decides");
        assert!(offline(&spec, "db", &spec.machines["db"]));
    }

    #[test]
    fn routes_go_through_the_router_or_the_gateway_on_a_shared_network() {
        let spec = testing::spec(LAB);
        assert_eq!(
            routes(&spec, "web", &spec.machines["web"]),
            [
                ("10.2.0.0/24".to_owned(), Ipv4Addr::new(10, 1, 0, 254)),
                ("10.3.0.0/24".to_owned(), Ipv4Addr::new(10, 1, 0, 1))
            ]
        );
        assert_eq!(
            routes(&spec, "fw", &spec.machines["fw"]),
            [("10.2.0.0/24".to_owned(), Ipv4Addr::new(10, 1, 0, 254))]
        );
        assert_eq!(
            routes(&spec, "pc", &spec.machines["pc"]),
            [],
            "nothing it shares a network with routes them: its default route does"
        );
    }

    #[test]
    fn without_reach_or_gateways_nothing_is_routed() {
        let spec = testing::spec(
            "version: 1\nname: t\nnetworks: { a: { cidr: 10.1.0.0/24 }, b: { cidr: 10.2.0.0/24 } }\nmachines:\n  m: { networks: { a: 10 }, app: { image: x } }\n",
        );
        assert!(!needed(&spec) && !routed(&spec));
        assert_eq!(routes(&spec, "m", &spec.machines["m"]), []);
    }
}
