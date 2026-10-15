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

/// Whether Isoloom routes this network with its own router (no machine is its gateway).
pub fn plain(spec: &Spec, network: &str) -> bool {
    spec.networks[network].gateway.is_none()
}

/// Whether the spec needs a router: `reach` rules between networks it routes itself.
pub fn needed(spec: &Spec) -> bool {
    spec.reach.iter().any(|r| plain(spec, &r.from) && plain(spec, &r.to))
}

/// Whether traffic crosses networks at all (Isoloom's router, or a machine as gateway).
pub fn routed(spec: &Spec) -> bool {
    needed(spec) || spec.networks.values().any(|n| n.gateway.is_some())
}

/// The networks the router is on.
pub fn networks(spec: &Spec) -> impl Iterator<Item = &String> {
    spec.networks.keys().filter(|n| plain(spec, n))
}

/// The router's address on a network.
pub fn address(spec: &Spec, network: &str) -> Ipv4Addr {
    cidr(spec, network).router()
}

/// Whether the machine is the gateway of one of its networks.
pub fn is_gateway(spec: &Spec, name: &str) -> bool {
    spec.networks.values().any(|n| n.gateway.as_deref() == Some(name))
}

/// Where a machine sends traffic for everywhere else (the internet included): the gateway
/// of its first network routed by another machine.
pub fn default_gateway(spec: &Spec, name: &str, m: &Machine) -> Option<Ipv4Addr> {
    m.networks
        .keys()
        .find(|n| spec.networks[*n].gateway.as_deref().is_some_and(|g| g != name))
        .map(|n| cidr(spec, n).gateway())
}

/// Routes a machine needs to the networks it isn't on: a network with a gateway through that
/// gateway (on a network they share), the others through the router (on the machine's first
/// network the router is on). Networks with neither route are left to the default route.
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
                    let g = &spec.machines[gw];
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
pub fn offline(spec: &Spec, name: &str, m: &Machine) -> bool {
    !m.networks.keys().any(|n| spec.networks[n].internet) && default_gateway(spec, name, m).is_none()
}
