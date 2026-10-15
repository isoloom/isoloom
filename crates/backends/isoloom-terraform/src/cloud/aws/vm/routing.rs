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
use isoloom_render::hcl_builder::{Block, Expression, Module, reference, string};

use crate::cloud::aws::route_out;
use crate::cloud::vm::gateways::{Hop, covering, routed, through, toward};
use crate::module::{ident, resource};

/// Every network in one VPC block: a /16 at most (AWS's largest).
const LARGEST: u8 = 16;

/// The VPC's block when a machine routes a network: the smallest one holding every network.
/// `None` without gateways, or when no /16 holds them (the refusal says so).
pub(super) fn vpc_block(env: &Environment) -> Option<String> {
    if routed(env).is_empty() {
        return None;
    }
    covering(env, LARGEST).map(|c| format!("{}/{}", std::net::Ipv4Addr::from(c.base), c.len))
}

/// Why AWS can't route these networks through their gateways, if it can't.
pub(super) fn refusal(env: &Environment) -> Option<String> {
    (!routed(env).is_empty() && covering(env, LARGEST).is_none()).then(|| {
        "networks routed by a `gateway` machine need every network inside one /16 on AWS (the VPC's block: a route through a machine must be more specific than it)".to_owned()
    })
}

/// The gateway's interface toward `net` (`gateways::toward`): the instance's own when it has
/// one interface, else the network interface it has there.
fn interface(hop: &Hop, net: &str) -> Expression {
    let id = ident(hop.gateway);
    if hop.machine.addresses.len() == 1 {
        return reference(&format!("aws_instance.{id}.primary_network_interface_id"));
    }
    let on = toward(hop.machine, net).map_or(net, |(on, _)| on);
    reference(&format!("aws_network_interface.{id}_{}.id", ident(on)))
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
    let b = resource("aws_route_table", &ident(net))
        .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
        .add_block(route_out());
    through(env, net)
        .iter()
        .fold(b, |b, hop| b.add_block(route(&hop.plan.cidr, ("network_interface_id", interface(hop, net)))))
        .build()
}

/// A route table per network when a machine routes one, else none (`None`: every subnet shares
/// the environment's table).
pub(super) fn blocks(env: &Environment, m: &mut Module) -> bool {
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

    use isoloom_spec::model::Target;

    const SPEC: &str = "version: 1\nname: e\nnetworks:\n  out: { cidr: 10.70.0.0/24 }\n  dmz: { cidr: 10.70.10.0/24, gateway: fw }\n  far: { cidr: 10.80.0.0/24 }\nmachines:\n  fw: { networks: { out: 5, dmz: 1 }, os: debian@12 }\n  web: { networks: { dmz: 10 }, os: debian@12 }\n  x: { networks: { far: 10 }, os: debian@12 }\n";

    #[test]
    fn networks_beyond_one_slash_16_are_refused() {
        let env = crate::testing::planned(SPEC, Target::CloudVm);
        assert!(super::refusal(&env).unwrap().contains("one /16"));
        assert_eq!(super::vpc_block(&env), None);
    }
}
