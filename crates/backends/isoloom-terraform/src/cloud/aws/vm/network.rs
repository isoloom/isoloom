//! The VPC, its subnets, and each machine's security group.

use crate::hcl_builder::{Block, BlockBuilder, Module, boolean, list, object, reference, string};
use isoloom_plan::{Environment, MachinePlan};

use super::resources::{self as res, RouteTable};
use super::routing;
use crate::cloud::aws::{egress, egress_to, first_zone, ingress, key_pair, route_out, sealed_route_out};
use crate::cloud::instance::name_with;
use crate::cloud::vm::{reach, seal};

/// The networks: one VPC, a subnet per network with its exact range (the first network's block
/// is the VPC's, the others are added to it; with a gateway machine, one block holds them all),
/// a route to the internet, and the key pair.
pub(super) fn blocks(env: &Environment, module: &mut Module) {
    let nets: Vec<(&String, String)> = env.networks.iter().map(|(n, p)| (n, p.cidr.to_string())).collect();
    let Some((_, first)) = nets.first() else { return };
    let whole = routing::vpc_block(env);
    module.commented(
        "The environment's networks: one VPC, a subnet per network with its exact range.",
        dns_hostnames(
            env,
            res::vpc()
                .block()
                .add_attribute(("cidr_block", string(whole.as_deref().unwrap_or(first.as_str())))),
        )
        .add_attribute(("tags", object([("Name", reference("local.name"))])))
        .build(),
    );
    if whole.is_none() {
        for (net, cidr) in nets.iter().skip(1) {
            module.block(
                res::cidr_association(net)
                    .block()
                    .add_attribute(("vpc_id", res::vpc().attr("id")))
                    .add_attribute(("cidr_block", string(cidr.as_str())))
                    .build(),
            );
        }
    }
    module.block(res::internet_gateway().block().add_attribute(("vpc_id", res::vpc().attr("id"))).build());
    let own_tables = routing::blocks(env, module);
    let closed = |net: &str| crate::seal::any(env) && crate::seal::closed(env, net);
    if !own_tables {
        module.block(
            res::route_table(RouteTable::Env)
                .block()
                .add_attribute(("vpc_id", res::vpc().attr("id")))
                .add_block(route_out())
                .build(),
        );
        if nets.iter().any(|(n, _)| closed(n)) {
            module.commented(
                "The networks without internet: theirs until the run is sealed, then allowed_cidr's replies only.",
                res::route_table(RouteTable::Sealed)
                    .block()
                    .add_attribute(("vpc_id", res::vpc().attr("id")))
                    .add_block(sealed_route_out())
                    .build(),
            );
        }
    }
    for (i, (net, cidr)) in nets.iter().enumerate() {
        let table = match (own_tables, closed(net)) {
            (true, _) => RouteTable::Own(net),
            (false, true) => RouteTable::Sealed,
            (false, false) => RouteTable::Env,
        };
        module.blocks(subnet(net, cidr, i > 0 && whole.is_none(), table));
    }
    module.block(key_pair());
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
fn subnet(net: &str, cidr: &str, added: bool, table: RouteTable<'_>) -> [Block; 2] {
    let mut s = res::subnet(net)
        .block()
        .add_attribute(("vpc_id", res::vpc().attr("id")))
        .add_attribute(("cidr_block", string(cidr)))
        .add_attribute(("availability_zone", first_zone()))
        .add_attribute(("tags", object([("Name", name_with(&format!("-{net}")))])));
    if added {
        s = s.add_attribute(("depends_on", list([res::cidr_association(net).whole()])));
    }
    [
        s.build(),
        res::route_table_association(net)
            .block()
            .add_attribute(("subnet_id", res::subnet(net).attr("id")))
            .add_attribute(("route_table_id", res::route_table(table).attr("id")))
            .build(),
    ]
}

/// Who may reach a machine: its own networks, the networks `reach` opens to it (on their ports,
/// TCP and UDP), its set-up from `allowed_cidr` (SSH, or WinRM on Windows), and its published
/// ports from `allowed_cidr`.
pub(super) fn security_group(env: &Environment, (name, mp): (&str, &MachinePlan), windows: bool) -> Block {
    let mut b = res::security_group(name)
        .block()
        .add_attribute(("name", name_with(&format!("-{name}"))))
        .add_attribute(("vpc_id", res::vpc().attr("id")));
    let Some(m) = env.spec().machines.get(name) else { return b.build() };
    for (n, p) in reach::own(env, m) {
        b = b.add_block(ingress(&format!("its network ({n})"), (0, 0), "-1", list([string(p.cidr.to_string())])));
    }
    let allowed = || list([reference("var.allowed_cidr")]);
    b = b.add_block(if windows {
        ingress("WinRM from allowed_cidr (its set-up)", (5985, 5985), "tcp", allowed())
    } else {
        ingress("SSH from allowed_cidr", (22, 22), "tcp", allowed())
    });
    for (reach, plan) in reach::opening(env, m) {
        let from = list([string(plan.cidr.to_string())]);
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
    for p in &mp.ports {
        b = b.add_block(ingress("published", (p.publish, p.publish), p.protocol.id(), allowed()));
    }
    b.add_block(egress_to(seal::destinations(env, mp))).build()
}

/// The controller's: every network may reach it, and SSH from `allowed_cidr`.
pub(super) fn controller_group(env: &Environment) -> Block {
    let mut b = res::security_group(res::CONTROLLER)
        .block()
        .add_attribute(("name", name_with(&format!("-{}", crate::machine::controller::NAME))))
        .add_attribute(("vpc_id", res::vpc().attr("id")));
    for (n, p) in &env.networks {
        b = b.add_block(ingress(n, (0, 0), "-1", list([string(p.cidr.to_string())])));
    }
    b.add_block(ingress("SSH from allowed_cidr", (22, 22), "tcp", list([reference("var.allowed_cidr")])))
        .add_block(egress())
        .build()
}
