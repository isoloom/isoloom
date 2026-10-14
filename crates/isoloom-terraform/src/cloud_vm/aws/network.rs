//! The VPC, its subnets, and each machine's security group.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, boolean, list, number, object, string};

use super::{first_zone, r, routing};
use crate::vm::res;

/// The networks: one VPC, a subnet per network with its exact range (the first network's block
/// is the VPC's, the others are added to it; with a gateway machine, one block holds them all),
/// a route to the internet, and the key pair.
pub fn blocks(env: &Environment, m: &mut Module) {
    let nets: Vec<(&String, &String)> = env.networks.iter().map(|(n, p)| (n, &p.cidr)).collect();
    let Some((_, first)) = nets.first() else { return };
    let whole = routing::vpc_block(env);
    m.commented(
        "The environment's networks: one VPC, a subnet per network with its exact range.",
        dns_hostnames(
            env,
            resource("aws_vpc", "env").add_attribute(("cidr_block", string(whole.as_deref().unwrap_or(first.as_str())))),
        )
        .add_attribute(("tags", object([("Name", r("local.name"))])))
        .build(),
    );
    if whole.is_none() {
        for (net, cidr) in nets.iter().skip(1) {
            m.block(
                resource("aws_vpc_ipv4_cidr_block_association", &res(net))
                    .add_attribute(("vpc_id", r("aws_vpc.env.id")))
                    .add_attribute(("cidr_block", string(cidr.as_str())))
                    .build(),
            );
        }
    }
    m.block(resource("aws_internet_gateway", "env").add_attribute(("vpc_id", r("aws_vpc.env.id"))).build());
    let own_tables = routing::blocks(env, m);
    if !own_tables {
        let route = Block::builder("route")
            .add_attribute(("cidr_block", string("0.0.0.0/0")))
            .add_attribute(("gateway_id", r("aws_internet_gateway.env.id")))
            .build();
        m.block(
            resource("aws_route_table", "env")
                .add_attribute(("vpc_id", r("aws_vpc.env.id")))
                .add_block(route)
                .build(),
        );
    }
    for (i, (net, cidr)) in nets.iter().enumerate() {
        let table = if own_tables { res(net) } else { "env".to_string() };
        m.blocks(subnet(net, cidr, i > 0 && whole.is_none(), &table));
    }
    m.block(
        resource("aws_key_pair", "env")
            .add_attribute(("key_name", r("local.name")))
            .add_attribute(("public_key", r("var.ssh_public_key")))
            .build(),
    );
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
    let id = res(net);
    let mut s = resource("aws_subnet", &id)
        .add_attribute(("vpc_id", r("aws_vpc.env.id")))
        .add_attribute(("cidr_block", string(cidr)))
        .add_attribute(("availability_zone", first_zone()))
        .add_attribute(("tags", object([("Name", name_with(&format!("-{net}")))])));
    if added {
        s = s.add_attribute(("depends_on", list([r(&format!("aws_vpc_ipv4_cidr_block_association.{id}"))])));
    }
    [
        s.build(),
        resource("aws_route_table_association", &id)
            .add_attribute(("subnet_id", r(&format!("aws_subnet.{id}.id"))))
            .add_attribute(("route_table_id", r(&format!("aws_route_table.{table}.id"))))
            .build(),
    ]
}

pub use crate::vm::cloud::{name_with, resource};

/// An `ingress` rule.
fn ingress(description: &str, ports: (u16, u16), protocol: &str, cidrs: Expression) -> Block {
    Block::builder("ingress")
        .add_attribute(("description", string(description)))
        .add_attribute(("from_port", number(u64::from(ports.0))))
        .add_attribute(("to_port", number(u64::from(ports.1))))
        .add_attribute(("protocol", string(protocol)))
        .add_attribute(("cidr_blocks", cidrs))
        .build()
}

/// Everything out.
fn egress() -> Block {
    Block::builder("egress")
        .add_attribute(("from_port", number(0)))
        .add_attribute(("to_port", number(0)))
        .add_attribute(("protocol", string("-1")))
        .add_attribute(("cidr_blocks", list([string("0.0.0.0/0")])))
        .build()
}

/// Who may reach a machine: its own networks, the networks `reach` opens to it (on their ports,
/// TCP and UDP), its set-up from `allowed_cidr` (SSH, or WinRM on Windows), and its published
/// ports from `allowed_cidr`.
pub fn security_group(env: &Environment, name: &str, windows: bool) -> Block {
    let spec = env.spec();
    let m = &spec.machines[name];
    let mut b = resource("aws_security_group", &res(name))
        .add_attribute(("name", name_with(&format!("-{name}"))))
        .add_attribute(("vpc_id", r("aws_vpc.env.id")));
    for n in m.networks.keys() {
        b = b.add_block(ingress(
            &format!("its network ({n})"),
            (0, 0),
            "-1",
            list([string(env.networks[n].cidr.as_str())]),
        ));
    }
    let allowed = || list([r("var.allowed_cidr")]);
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
pub fn controller_group(env: &Environment) -> Block {
    let mut b = resource("aws_security_group", "isoloom_controller")
        .add_attribute(("name", name_with("-controller")))
        .add_attribute(("vpc_id", r("aws_vpc.env.id")));
    for (n, p) in &env.networks {
        b = b.add_block(ingress(n, (0, 0), "-1", list([string(p.cidr.as_str())])));
    }
    b.add_block(ingress("SSH from allowed_cidr", (22, 22), "tcp", list([r("var.allowed_cidr")])))
        .add_block(egress())
        .build()
}
