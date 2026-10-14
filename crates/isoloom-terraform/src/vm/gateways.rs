//! Networks routed by one of the environment's machines (`gateway:`), on a cloud. A cloud keeps
//! a subnet's first address for its own router, so the gateway machine can't take the gateway
//! address there: it takes the address Isoloom's router would have, unused on a network a
//! machine routes, and the cloud's route tables send the network's traffic through it.

use std::borrow::Cow;
use std::net::Ipv4Addr;

use isoloom_plan::{Environment, MachinePlan, Step};
use isoloom_render::hcl_builder::Template;
use isoloom_spec::validate::Cidr;

/// The networks a machine routes, each with that machine, in spec order.
pub fn routed(env: &Environment) -> Vec<(&str, &str)> {
    env.networks.iter().filter_map(|(n, p)| p.gateway.as_deref().map(|g| (n.as_str(), g))).collect()
}

/// Whether the octet is the machine's gateway address on `network` (validation gives the
/// gateway machine that one address, which a cloud keeps for its router).
pub fn is_gateway_address(env: &Environment, machine: &str, network: &str) -> bool {
    env.networks.get(network).and_then(|p| p.gateway.as_deref()) == Some(machine)
}

/// Each gateway address moved to the router's: (gateway address, the address it moves to).
fn moves(env: &Environment) -> Vec<(Ipv4Addr, Ipv4Addr)> {
    routed(env)
        .into_iter()
        .filter_map(|(n, _)| Cidr::parse(&env.networks[n].cidr).map(|c| (c.gateway(), c.router())))
        .collect()
}

/// The environment with each gateway machine at its network's router address instead of the
/// gateway address: in its own addresses and in every machine's hosts file.
pub fn relocated(env: &Environment) -> Cow<'_, Environment> {
    let moves = moves(env);
    if moves.is_empty() {
        return Cow::Borrowed(env);
    }
    let moved = |a: Ipv4Addr| moves.iter().find(|(from, _)| *from == a).map_or(a, |(_, to)| *to);
    let mut env = env.clone();
    for mp in env.machines.values_mut() {
        for a in mp.addresses.values_mut() {
            *a = moved(*a);
        }
        for step in &mut mp.lifecycle {
            if let Step::Hosts { entries } = step {
                for (a, _) in entries.iter_mut() {
                    *a = moved(*a);
                }
            }
        }
    }
    Cow::Owned(env)
}

/// A gateway forwards between its networks (its own provisioning sets the rules): forwarding
/// on at every boot.
pub fn at_boot(mp: &MachinePlan) -> Vec<Template> {
    if !mp.is_gateway {
        return Vec::new();
    }
    vec![Template::text(
        "echo 'net.ipv4.ip_forward=1' | sudo tee /etc/sysctl.d/90-isoloom-forwarding.conf >/dev/null && sudo sysctl -q -p /etc/sysctl.d/90-isoloom-forwarding.conf",
    )]
}

/// The smallest block holding every network, when one of `shortest` bits or longer does: a
/// cloud whose routes can only be more specific than its network's block puts them all in it.
pub fn covering(env: &Environment, shortest: u8) -> Option<Cidr> {
    let nets: Vec<Cidr> = env.networks.values().filter_map(|n| Cidr::parse(&n.cidr)).collect();
    let first = nets.first()?;
    (shortest..=first.len)
        .rev()
        .filter_map(|len| Cidr::parse(&format!("{}/{len}", Ipv4Addr::from(first.base & mask(len)))))
        .find(|c| nets.iter().all(|n| c.contains(*n)))
}

fn mask(len: u8) -> u32 {
    if len == 0 { 0 } else { u32::MAX << (32 - len) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(spec: &str) -> Environment {
        let spec = isoloom_spec::parse(spec).unwrap();
        assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
        Environment::build(&spec, isoloom_spec::model::Target::CloudVm, None)
    }

    const EDGE: &str = "version: 1\nname: e\nnetworks:\n  out: { cidr: 10.70.0.0/24 }\n  dmz: { cidr: 10.70.10.0/24, gateway: fw }\nmachines:\n  fw: { networks: { out: 5, dmz: 1 }, vm: { os: debian-12, provision: [fw.sh] } }\n  web: { networks: { dmz: 10 }, vm: { os: debian-12, provision: [web.sh] } }\n";

    #[test]
    fn the_gateway_moves_to_the_router_address_everywhere() {
        let e = env(EDGE);
        let r = relocated(&e);
        assert_eq!(r.machines["fw"].addresses["dmz"], Ipv4Addr::new(10, 70, 10, 254));
        let hosts = r.machines["web"].lifecycle.iter().find_map(|s| match s {
            Step::Hosts { entries } => Some(entries.clone()),
            _ => None,
        });
        assert!(hosts.unwrap().iter().any(|(a, n)| *a == Ipv4Addr::new(10, 70, 10, 254) && n[0] == "fw"));
        assert!(is_gateway_address(&e, "fw", "dmz") && !is_gateway_address(&e, "fw", "out"));
        assert!(at_boot(&e.machines["fw"]).len() == 1 && at_boot(&e.machines["web"]).is_empty());
    }

    #[test]
    fn the_covering_block_holds_every_network() {
        let e = env(EDGE);
        assert_eq!(covering(&e, 16), Cidr::parse("10.70.0.0/20"));
        assert_eq!(covering(&e, 22), None);
    }
}
