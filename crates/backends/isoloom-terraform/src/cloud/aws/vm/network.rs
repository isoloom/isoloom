//! The VPC, its subnets, and each machine's security group.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Module, boolean, list, object, reference, string};

use super::routing;
use crate::cloud::aws::{egress, first_zone, ingress, key_pair, route_out};
use crate::cloud::instance::name_with;
use crate::module::{ident, resource};

/// The networks: one VPC, a subnet per network with its exact range (the first network's block
/// is the VPC's, the others are added to it; with a gateway machine, one block holds them all),
/// a route to the internet, and the key pair.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    let nets: Vec<(&String, &String)> = env.networks.iter().map(|(n, p)| (n, &p.cidr)).collect();
    let Some((_, first)) = nets.first() else { return };
    let whole = routing::vpc_block(env);
    m.commented(
        "The environment's networks: one VPC, a subnet per network with its exact range.",
        dns_hostnames(
            env,
            resource("aws_vpc", "env").add_attribute(("cidr_block", string(whole.as_deref().unwrap_or(first.as_str())))),
        )
        .add_attribute(("tags", object([("Name", reference("local.name"))])))
        .build(),
    );
    if whole.is_none() {
        for (net, cidr) in nets.iter().skip(1) {
            m.block(
                resource("aws_vpc_ipv4_cidr_block_association", &ident(net))
                    .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
                    .add_attribute(("cidr_block", string(cidr.as_str())))
                    .build(),
            );
        }
    }
    m.block(
        resource("aws_internet_gateway", "env")
            .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
            .build(),
    );
    let own_tables = routing::blocks(env, m);
    if !own_tables {
        m.block(
            resource("aws_route_table", "env")
                .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
                .add_block(route_out())
                .build(),
        );
    }
    for (i, (net, cidr)) in nets.iter().enumerate() {
        let table = if own_tables { ident(net) } else { "env".to_owned() };
        m.blocks(subnet(net, cidr, i > 0 && whole.is_none(), &table));
    }
    m.block(key_pair());
}

/// With a private zone (`dns.private`), the VPC's DNS support and host names on: a private
/// hosted zone answers only in a VPC that has both.
fn dns_hostnames(env: &Environment, vpc: BlockBuilder) -> BlockBuilder {
    if env.private_dns.is_none() {
        return vpc;
    }
    vpc.add_attribute(("enable_dns_support", boolean(true)))
        .add_attribute(("enable_dns_hostnames", boolean(true)))
}

/// A network's subnet (after its block joins the VPC, unless it is the VPC's own) and its
/// association with route table `table`.
fn subnet(net: &str, cidr: &str, added: bool, table: &str) -> [Block; 2] {
    let id = ident(net);
    let mut s = resource("aws_subnet", &id)
        .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
        .add_attribute(("cidr_block", string(cidr)))
        .add_attribute(("availability_zone", first_zone()))
        .add_attribute(("tags", object([("Name", name_with(&format!("-{net}")))])));
    if added {
        s = s.add_attribute(("depends_on", list([reference(&format!("aws_vpc_ipv4_cidr_block_association.{id}"))])));
    }
    [
        s.build(),
        resource("aws_route_table_association", &id)
            .add_attribute(("subnet_id", reference(&format!("aws_subnet.{id}.id"))))
            .add_attribute(("route_table_id", reference(&format!("aws_route_table.{table}.id"))))
            .build(),
    ]
}

/// Who may reach a machine: its own networks, the networks `reach` opens to it (on their ports,
/// TCP and UDP), its set-up from `allowed_cidr` (SSH, or WinRM on Windows), and its published
/// ports from `allowed_cidr`.
pub(super) fn security_group(env: &Environment, name: &str, windows: bool) -> Block {
    let spec = env.spec();
    let m = &spec.machines[name];
    let mut b = resource("aws_security_group", &ident(name))
        .add_attribute(("name", name_with(&format!("-{name}"))))
        .add_attribute(("vpc_id", reference("aws_vpc.env.id")));
    for n in m.networks.keys() {
        b = b.add_block(ingress(
            &format!("its network ({n})"),
            (0, 0),
            "-1",
            list([string(env.networks[n].cidr.as_str())]),
        ));
    }
    let allowed = || list([reference("var.allowed_cidr")]);
    b = b.add_block(if windows {
        ingress("WinRM from allowed_cidr (its set-up)", (5985, 5985), "tcp", allowed())
    } else {
        ingress("SSH from allowed_cidr", (22, 22), "tcp", allowed())
    });
    for reach in spec
        .reach
        .iter()
        .filter(|x| m.networks.contains_key(&x.to) && !m.networks.contains_key(&x.from))
    {
        let from = list([string(env.networks[&reach.from].cidr.as_str())]);
        let what = format!("reach from {}", reach.from);
        if reach.ports.is_empty() {
            b = b.add_block(ingress(&what, (0, 0), "-1", from));
            continue;
        }
        for p in &reach.ports {
            for proto in ["tcp", "udp"] {
                b = b.add_block(ingress(&what, (*p, *p), proto, from.clone()));
            }
        }
    }
    for p in &env.machines[name].ports {
        b = b.add_block(ingress("published", (p.publish, p.publish), p.protocol.id(), allowed()));
    }
    b.add_block(egress()).build()
}

/// The controller's: every network may reach it, and SSH from `allowed_cidr`.
pub(super) fn controller_group(env: &Environment) -> Block {
    let mut b = resource("aws_security_group", "isoloom_controller")
        .add_attribute(("name", name_with("-controller")))
        .add_attribute(("vpc_id", reference("aws_vpc.env.id")));
    for (n, p) in &env.networks {
        b = b.add_block(ingress(n, (0, 0), "-1", list([string(p.cidr.as_str())])));
    }
    b.add_block(ingress("SSH from allowed_cidr", (22, 22), "tcp", list([reference("var.allowed_cidr")])))
        .add_block(egress())
        .build()
}
