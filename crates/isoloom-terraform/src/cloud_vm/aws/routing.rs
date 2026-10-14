//! Networks routed by a machine (`gateway:`) on AWS. Every network sits in one VPC block, so a
//! route to a network is more specific than the VPC's own route, which AWS lets a route table
//! send to an instance's interface. Each network then has its route table: the internet through
//! the internet gateway, and a network a machine routes through that machine's interface (its
//! source/destination check off). A routed network's own table sends every other network
//! through its gateway, so traffic crosses the gateway both ways.
//!
//! The machines keep their own public addresses (Terraform sets them up over SSH there), so a
//! routed network still goes out to the internet directly rather than through its gateway.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, Expression, Module, string};

use super::network::resource;
use super::r;
use crate::vm::gateways::{covering, routed};
use crate::vm::res;

/// Every network in one VPC block: a /16 at most (AWS's largest).
const LARGEST: u8 = 16;

/// The VPC's block when a machine routes a network: the smallest one holding every network.
/// `None` without gateways, or when no /16 holds them (the refusal says so).
pub fn vpc_block(env: &Environment) -> Option<String> {
    if routed(env).is_empty() {
        return None;
    }
    covering(env, LARGEST).map(|c| format!("{}/{}", std::net::Ipv4Addr::from(c.base), c.len))
}

/// Why AWS can't route these networks through their gateways, if it can't.
pub fn refusal(env: &Environment) -> Option<String> {
    (!routed(env).is_empty() && covering(env, LARGEST).is_none()).then(|| {
        "networks routed by a `gateway` machine need every network inside one /16 on AWS (the VPC's block: a route through a machine must be more specific than it)".to_string()
    })
}

/// Machine `gateway`'s interface toward `net`: its interface on it when it has one there, else
/// its first.
fn interface(env: &Environment, gateway: &str, net: &str) -> Expression {
    let id = res(gateway);
    let addresses = &env.machines[gateway].addresses;
    if addresses.len() == 1 {
        return r(&format!("aws_instance.{id}.primary_network_interface_id"));
    }
    let on = if addresses.contains_key(net) {
        net
    } else {
        addresses.keys().next().map_or(net, String::as_str)
    };
    r(&format!("aws_network_interface.{id}_{}.id", res(on)))
}

fn route(cidr: &str, target: (&str, Expression)) -> Block {
    Block::builder("route")
        .add_attribute(("cidr_block", string(cidr)))
        .add_attribute(target)
        .build()
}

/// Network `net`'s route table: the internet through the internet gateway, the networks its
/// gateway routes it to (all of them, on a routed network) through that gateway, and each
/// routed network through its own.
fn table(env: &Environment, net: &str) -> Block {
    let gateways = routed(env);
    let mut b = resource("aws_route_table", &res(net))
        .add_attribute(("vpc_id", r("aws_vpc.env.id")))
        .add_block(route("0.0.0.0/0", ("gateway_id", r("aws_internet_gateway.env.id"))));
    let own = gateways.iter().find(|(n, _)| *n == net).map(|(_, g)| *g);
    for (other, p) in env.networks.iter().filter(|(n, _)| n.as_str() != net) {
        let through = own.or_else(|| gateways.iter().find(|(n, _)| *n == other.as_str()).map(|(_, g)| *g));
        if let Some(gateway) = through {
            b = b.add_block(route(&p.cidr, ("network_interface_id", interface(env, gateway, net))));
        }
    }
    b.build()
}

/// A route table per network when a machine routes one, else none (`None`: every subnet shares
/// the environment's table).
pub fn blocks(env: &Environment, m: &mut Module) -> bool {
    if routed(env).is_empty() {
        return false;
    }
    let mut first = true;
    for net in env.networks.keys() {
        let t = table(env, net);
        if std::mem::take(&mut first) {
            m.commented("A route table per network: the networks a machine routes go through its interface.", t);
        } else {
            m.block(t);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use isoloom_plan::Environment;
    use isoloom_spec::model::Target;

    const SPEC: &str = "version: 1\nname: e\nnetworks:\n  out: { cidr: 10.70.0.0/24 }\n  dmz: { cidr: 10.70.10.0/24, gateway: fw }\n  far: { cidr: 10.80.0.0/24 }\nmachines:\n  fw: { networks: { out: 5, dmz: 1 }, vm: { os: debian-12 } }\n  web: { networks: { dmz: 10 }, vm: { os: debian-12 } }\n  x: { networks: { far: 10 }, vm: { os: debian-12 } }\n";

    #[test]
    fn networks_beyond_one_slash_16_are_refused() {
        let spec = isoloom_spec::parse(SPEC).unwrap();
        let env = Environment::build(&spec, Target::CloudVm, None);
        assert!(super::refusal(&env).unwrap().contains("one /16"));
        assert_eq!(super::vpc_block(&env), None);
    }
}
