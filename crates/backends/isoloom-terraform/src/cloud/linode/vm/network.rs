//! The VPC, a subnet per network at its exact range, and a firewall per machine.
//!
//! A machine's firewall lets in: SSH and its own published ports from `allowed_cidr` (never a
//! port another machine publishes), its network, and the `reach` rules into it; the rest is
//! dropped. Out: the `reach` rules out of its network, then nothing to the environment's other
//! networks; the rest (its own network, the internet) is accepted. Replies come back either way.

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{Block, Module, Template, list, reference, string};
use isoloom_spec::validate::Cidr;

use crate::cloud::instance::name_with;
use crate::cloud::linode::{Action, Way, rule};
use crate::cloud::vm::published;
use crate::cloud::vm::reach::{self, Rule};
use crate::machine::vms;
use crate::module::{ident, resource};

/// The VPC and a subnet per network.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    m.commented(
        "The environment's networks: a VPC with a subnet per network at its exact range, so every\nmachine keeps its address.",
        resource("linode_vpc", "env")
            .add_attribute(("label", reference("local.name")))
            .add_attribute(("region", reference("var.region")))
            .build(),
    );
    for (net, p) in &env.networks {
        m.block(
            resource("linode_vpc_subnet", &ident(net))
                .add_attribute(("vpc_id", reference("linode_vpc.env.id")))
                .add_attribute(("label", string(net.as_str())))
                .add_attribute(("ipv4", string(p.cidr.as_str())))
                .build(),
        );
    }
    for (name, mp) in vms(env) {
        let Some((net, plan)) = mp.addresses.keys().next().and_then(|n| env.networks.get_key_value(n)) else {
            continue;
        };
        m.commented(
            &format!("Machine `{name}` on `{net}`: what may reach it, and where it may not go."),
            firewall(env, (name, mp), (net, &plan.cidr)),
        );
    }
}

/// Every protocol to or from `cidr`: TCP and UDP on every port, ICMP.
fn everything(rule_of: (Way, Action), label: &str, cidr: &str) -> Vec<Block> {
    let peers = || list([string(cidr)]);
    vec![
        rule(rule_of, &format!("{label}-tcp"), "TCP", Some("1-65535".to_owned()), peers()),
        rule(rule_of, &format!("{label}-udp"), "UDP", Some("1-65535".to_owned()), peers()),
        rule(rule_of, &format!("{label}-icmp"), "ICMP", None, peers()),
    ]
}

/// The rules accepting the `reach` rules (`way`: in for those into the network, out for those
/// out of it).
fn reach_rules(way: Way, rules: &[Rule<'_>]) -> Vec<Block> {
    let mut out = Vec::new();
    for r in rules {
        let label = format!("reach-{}", r.network);
        if r.ports.is_empty() {
            out.extend(everything((way, Action::Accept), &label, r.cidr));
            continue;
        }
        let ports = r.ports.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
        for protocol in ["TCP", "UDP"] {
            let l = format!("{label}-{}", protocol.to_lowercase());
            out.push(rule((way, Action::Accept), &l, protocol, Some(ports.clone()), list([string(r.cidr)])));
        }
    }
    out
}

/// Machine `name`'s firewall, on network `net` (its block, `cidr`).
fn firewall(env: &Environment, (name, mp): (&str, &MachinePlan), (net, cidr): (&str, &str)) -> Block {
    let allowed = || list([reference("var.allowed_cidr")]);
    let mut b = resource("linode_firewall", &ident(name))
        .add_attribute(("label", name_with(&format!("-{name}"))))
        .add_attribute(("inbound_policy", string("DROP")))
        .add_attribute(("outbound_policy", string("ACCEPT")))
        .add_block(rule((Way::In, Action::Accept), "ssh", "TCP", Some("22".to_owned()), allowed()));
    let published = published::ports(mp);
    for (udp, label, protocol) in [(false, "published", "TCP"), (true, "published-udp", "UDP")] {
        let ports: Vec<String> = published.iter().filter(|(p, _)| p.is_tcp() != udp).map(|(_, n)| n.to_string()).collect();
        if !ports.is_empty() {
            b = b.add_block(rule((Way::In, Action::Accept), label, protocol, Some(ports.join(",")), allowed()));
        }
    }
    b = b
        .add_blocks(everything((Way::In, Action::Accept), "vpc", cidr))
        .add_blocks(reach_rules(Way::In, &reach::into(env, net)))
        .add_blocks(reach_rules(Way::Out, &reach::out_of(env, net)));
    for (other, p) in env.networks.iter().filter(|(n, _)| n.as_str() != net) {
        b = b.add_blocks(everything((Way::Out, Action::Drop), &format!("no-{other}"), &p.cidr));
    }
    b.add_attribute(("linodes", list([reference(&format!("linode_instance.{}.id", ident(name)))])))
        .build()
}

/// A machine's routes to the other networks, through its subnet's gateway (the VPC routes
/// between subnets; its default route is the public interface's): at every boot.
pub(super) fn routes(env: &Environment, net: &str) -> Vec<Template> {
    let Some(gateway) = env.networks.get(net).and_then(|p| Cidr::parse(&p.cidr)).map(Cidr::gateway) else {
        return Vec::new();
    };
    env.networks
        .iter()
        .filter(|(n, _)| n.as_str() != net)
        .map(|(_, p)| Template::text(format!("sudo ip route replace {} via {gateway}", p.cidr)))
        .collect()
}
