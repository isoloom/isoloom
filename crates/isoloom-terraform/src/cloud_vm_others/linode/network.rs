//! The VPC, a subnet per network at its exact range, and a firewall per machine.
//!
//! A machine's firewall lets in: SSH and its own published ports from `allowed_cidr` (never a
//! port another machine publishes), its network, and the `reach` rules into it; the rest is
//! dropped. Out: the `reach` rules out of its network, then nothing to the environment's other
//! networks; the rest (its own network, the internet) is accepted. Replies come back either way.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, Expression, Module, Template, list, reference, string};
use isoloom_spec::validate::Cidr;

use super::resource;
use crate::cloud_vm_others::shared;
use crate::vm::cloud::name_with;
use crate::vm::reach::{self, Rule};
use crate::vm::{res, vms};

/// The VPC and a subnet per network.
pub fn blocks(env: &Environment, m: &mut Module) {
    m.commented(
        "The environment's networks: a VPC with a subnet per network at its exact range, so every\nmachine keeps its address.",
        resource("linode_vpc", "env")
            .add_attribute(("label", reference("local.name")))
            .add_attribute(("region", reference("var.region")))
            .build(),
    );
    for (net, p) in &env.networks {
        m.block(
            resource("linode_vpc_subnet", &res(net))
                .add_attribute(("vpc_id", reference("linode_vpc.env.id")))
                .add_attribute(("label", string(net.as_str())))
                .add_attribute(("ipv4", string(p.cidr.as_str())))
                .build(),
        );
    }
    for (name, mp) in vms(env) {
        if let Some(net) = mp.addresses.keys().next() {
            m.commented(
                &format!("Machine `{name}` on `{net}`: what may reach it, and where it may not go."),
                firewall(env, name, net),
            );
        }
    }
}

/// A rule (`kind`: `inbound` or `outbound`, `action`: `ACCEPT` or `DROP`) on `protocol`, its
/// ports (`None`: all of them).
fn rule((kind, action): (&str, &str), label: &str, protocol: &str, ports: Option<String>, peers: Expression) -> Block {
    let mut b = Block::builder(kind)
        .add_attribute(("label", string(label)))
        .add_attribute(("action", string(action)))
        .add_attribute(("protocol", string(protocol)));
    if let Some(p) = ports {
        b = b.add_attribute(("ports", string(p)));
    }
    b.add_attribute(("ipv4", peers)).build()
}

/// Every protocol to or from `cidr`: TCP and UDP on every port, ICMP.
fn everything(kind: &str, label: &str, action: &str, cidr: &str) -> Vec<Block> {
    let peers = || list([string(cidr)]);
    vec![
        rule((kind, action), &format!("{label}-tcp"), "TCP", Some("1-65535".into()), peers()),
        rule((kind, action), &format!("{label}-udp"), "UDP", Some("1-65535".into()), peers()),
        rule((kind, action), &format!("{label}-icmp"), "ICMP", None, peers()),
    ]
}

/// The rules accepting the `reach` rules (`kind`: `inbound` for those into the network,
/// `outbound` for those out of it).
fn reach_rules(kind: &str, rules: &[Rule<'_>]) -> Vec<Block> {
    let mut out = Vec::new();
    for r in rules {
        let label = format!("reach-{}", r.network);
        if r.ports.is_empty() {
            out.extend(everything(kind, &label, "ACCEPT", r.cidr));
            continue;
        }
        let ports = r.ports.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
        for protocol in ["TCP", "UDP"] {
            let l = format!("{label}-{}", protocol.to_lowercase());
            out.push(rule((kind, "ACCEPT"), &l, protocol, Some(ports.clone()), list([string(r.cidr)])));
        }
    }
    out
}

/// Machine `name`'s firewall, on network `net`.
pub fn firewall(env: &Environment, name: &str, net: &str) -> Block {
    let allowed = || list([reference("var.allowed_cidr")]);
    let mut b = resource("linode_firewall", &res(name))
        .add_attribute(("label", name_with(&format!("-{name}"))))
        .add_attribute(("inbound_policy", string("DROP")))
        .add_attribute(("outbound_policy", string("ACCEPT")))
        .add_block(rule(("inbound", "ACCEPT"), "ssh", "TCP", Some("22".into()), allowed()));
    let published = shared::published_ports(&env.machines[name]);
    for (udp, label, protocol) in [(false, "published", "TCP"), (true, "published-udp", "UDP")] {
        let ports: Vec<String> = published.iter().filter(|(p, _)| p.is_tcp() != udp).map(|(_, n)| n.to_string()).collect();
        if !ports.is_empty() {
            b = b.add_block(rule(("inbound", "ACCEPT"), label, protocol, Some(ports.join(",")), allowed()));
        }
    }
    b = b
        .add_blocks(everything("inbound", "vpc", "ACCEPT", &env.networks[net].cidr))
        .add_blocks(reach_rules("inbound", &reach::into(env, net)))
        .add_blocks(reach_rules("outbound", &reach::out_of(env, net)));
    for (other, p) in env.networks.iter().filter(|(n, _)| n.as_str() != net) {
        b = b.add_blocks(everything("outbound", &format!("no-{other}"), "DROP", &p.cidr));
    }
    b.add_attribute(("linodes", list([reference(&format!("linode_instance.{}.id", res(name)))])))
        .build()
}

/// A machine's routes to the other networks, through its subnet's gateway (the VPC routes
/// between subnets; its default route is the public interface's): at every boot.
pub fn routes(env: &Environment, net: &str) -> Vec<Template> {
    let Some(gateway) = Cidr::parse(&env.networks[net].cidr).map(Cidr::gateway) else {
        return Vec::new();
    };
    env.networks
        .iter()
        .filter(|(n, _)| n.as_str() != net)
        .map(|(_, p)| Template::text(format!("sudo ip route replace {} via {gateway}", p.cidr)))
        .collect()
}
