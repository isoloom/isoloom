//! Where every machine sits: its address on each network, the address another machine reaches
//! it at, its names, and the order machines start in. Pure functions of a validated spec, shared
//! by the plan and every backend (re-exported by `isoloom-render`).

use std::net::Ipv4Addr;

use isoloom_spec::model::Spec;
use isoloom_spec::validate::{Cidr, starts_after};

/// A network's block (validated specs only).
pub(crate) fn cidr(spec: &Spec, network: &str) -> Cidr {
    Cidr::parse(&spec.networks[network].cidr).expect("validated cidr")
}

/// A machine's address on a network (validated specs only).
pub fn address(spec: &Spec, network: &str, octet: u8) -> Ipv4Addr {
    let cidr = cidr(spec, network);
    cidr.host(octet)
        .or_else(|| (octet == cidr.gateway_octet()).then(|| cidr.gateway()))
        .expect("validated address")
}

/// Dotted netmask of a network.
pub fn netmask(spec: &Spec, network: &str) -> Ipv4Addr {
    let cidr = cidr(spec, network);
    Ipv4Addr::from(if cidr.len == 0 { 0 } else { u32::MAX << (32 - cidr.len) })
}

/// The host's own address on a network (Docker's bridge): the gateway address, unless a
/// machine is the network's gateway; then the router's address, unused on such networks.
pub fn host_address(spec: &Spec, network: &str) -> Ipv4Addr {
    let cidr = cidr(spec, network);
    if spec.networks[network].gateway.is_some() {
        cidr.router()
    } else {
        cidr.gateway()
    }
}

/// The address `from` should use to reach `to`: on a network they share if any, else
/// `to`'s first address.
pub fn address_for(spec: &Spec, from: &str, to: &str) -> Ipv4Addr {
    let a = &spec.machines[from];
    let b = &spec.machines[to];
    let (net, octet) = b
        .networks
        .iter()
        .find(|(n, _)| a.networks.contains_key(*n))
        .or_else(|| b.networks.first())
        .expect("validated: machines have a network");
    address(spec, net, *octet)
}

/// A machine's names for a hosts line: its own, then its `aliases`.
pub fn names_of(spec: &Spec, name: &str) -> String {
    std::iter::once(name)
        .chain(spec.machines.get(name).map(|m| m.aliases.iter().map(String::as_str)).into_iter().flatten())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Machines in start order: each after the machines it depends on (validated: no cycles).
pub fn start_order(spec: &Spec) -> Vec<&str> {
    fn visit<'a>(spec: &'a Spec, name: &'a str, order: &mut Vec<&'a str>) {
        if order.contains(&name) {
            return;
        }
        for dep in starts_after(spec, name, &spec.machines[name]) {
            visit(spec, dep, order);
        }
        order.push(name);
    }
    let mut order: Vec<&str> = Vec::new();
    for name in spec.machines.keys() {
        visit(spec, name, &mut order);
    }
    order
}
