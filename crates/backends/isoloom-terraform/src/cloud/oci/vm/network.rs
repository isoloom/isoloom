//! The VCN: a subnet per network at its exact range, each with its own security list, routed to
//! the internet and, for a network a machine routes (`gateway:`), through that machine.
//!
//! - A security list allows only: into a network, its own machines, the `reach` rules into it,
//!   SSH from `allowed_cidr` (a machine's published ports are its own group's); out of it, its
//!   own network, the `reach` rules out of it, and the internet (every address outside the private ranges, since a
//!   security list can't deny). Both ways, ICMP's "fragmentation needed", so a path with a
//!   smaller MTU still works.
//! - A routed network's subnet starts on the internet table, so its machines can be set up, and
//!   moves to its own table once the gateway exists: Oracle Cloud's routes name the gateway's
//!   private IP, which exists only with its VNIC. Every other network's table sends the routed
//!   network through the gateway too, so traffic crosses it both ways.
//! - A dual-stack network's IPv6 block is one of the VCN's private prefixes and its subnet's
//!   prefix, with Oracle's public prefix left out: the spec's block is the network's only one.
//!   Its security list mirrors the IPv4 rules on the IPv6 blocks (its own, the `reach` rules
//!   between dual-stack networks, ICMPv6's "packet too big"). The VCN routes between its
//!   prefixes itself; no IPv6 route leaves it, since Oracle Cloud routes no private prefix to
//!   the internet (machines go out over IPv4).

use isoloom_plan::{Environment, NetworkPlan};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, attr, boolean, index, list, number, reference, string};

use crate::cloud::instance::name_with;
use crate::cloud::oci::{owned, route_out};
use crate::cloud::vm::gateways::{Hop, routed, through, toward};
use crate::cloud::vm::reach::{self, Rule};
use crate::module::{ident, resource};

/// A VCN holds five blocks at most.
const BLOCKS: usize = 5;

/// Why the VCN can't hold the networks, if it can't.
pub(super) fn refusal(env: &Environment) -> Option<String> {
    (env.networks.len() > BLOCKS).then(|| format!("an Oracle Cloud VCN holds {BLOCKS} address blocks at most, one per network"))
}

fn in_vcn(b: BlockBuilder, name: Expression) -> BlockBuilder {
    owned(b)
        .add_attribute(("vcn_id", reference("oci_core_vcn.env.id")))
        .add_attribute(("display_name", name))
}

fn display(net: &str) -> Expression {
    name_with(&format!("-{net}"))
}

/// The VCN, its internet gateway and route tables, a security list and a subnet per network.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    let cidrs: Vec<&str> = env.networks.values().map(|n| n.cidr.as_str()).collect();
    let vcn = owned(resource("oci_core_vcn", "env")).add_attribute(("display_name", reference("local.name")));
    let vcn = match cidrs.as_slice() {
        [one] => vcn.add_attribute(("cidr_block", string(*one))),
        many => vcn.add_attribute(("cidr_blocks", list(many.iter().map(|c| string(*c))))),
    };
    m.commented(
        "The environment's networks: a VCN holding each one's block, routed to the internet.",
        ipv6_vcn(env, vcn).build(),
    );
    m.block(in_vcn(resource("oci_core_internet_gateway", "env"), reference("local.name")).build());
    m.block(
        in_vcn(resource("oci_core_route_table", "env"), reference("local.name"))
            .add_block(route_out())
            .build(),
    );
    for (net, p) in &env.networks {
        m.commented(
            &format!("Network `{net}`: what may reach its machines, and where they may go."),
            security_list(env, (net, p)),
        );
        let mut subnet = owned(resource("oci_core_subnet", &ident(net)))
            .add_attribute(("vcn_id", reference("oci_core_vcn.env.id")))
            .add_attribute(("cidr_block", string(p.cidr.as_str())))
            .add_attributes(p.ipv6.as_ref().map(|v| ("ipv6cidr_block", string(v.cidr.as_str()))))
            .add_attribute(("display_name", display(net)))
            .add_attribute(("route_table_id", reference("oci_core_route_table.env.id")))
            .add_attribute(("security_list_ids", list([reference(&format!("oci_core_security_list.{}.id", ident(net)))])));
        if !routed(env).is_empty() {
            // Its own table replaces the internet one once the gateway exists.
            subnet = subnet.add_block(
                Block::builder("lifecycle")
                    .add_attribute(("ignore_changes", list([reference("route_table_id")])))
                    .build(),
            );
        }
        m.block(subnet.build());
    }
}

/// With dual-stack networks: IPv6 on, their blocks as the VCN's private prefixes, and no
/// Oracle-allocated public prefix beside them.
fn ipv6_vcn(env: &Environment, vcn: BlockBuilder) -> BlockBuilder {
    let blocks: Vec<Expression> = env.networks.values().filter_map(|n| n.ipv6.as_ref()).map(|v| string(v.cidr.as_str())).collect();
    if blocks.is_empty() {
        return vcn;
    }
    vcn.add_attribute(("is_ipv6enabled", boolean(true)))
        .add_attribute(("is_oracle_gua_allocation_enabled", boolean(false)))
        .add_attribute(("ipv6private_cidr_blocks", list(blocks)))
}

/// A rule's TCP or UDP port range (`options`: `tcp_options` or `udp_options`).
fn port(options: &str, p: u16) -> Block {
    Block::builder(options)
        .add_attribute(("min", number(u64::from(p))))
        .add_attribute(("max", number(u64::from(p))))
        .build()
}

/// A rule (`kind`: `ingress_security_rules` or `egress_security_rules`) with `peer` (its source
/// or destination): every protocol, or one port of TCP (6) or UDP (17).
fn rule(kind: &str, description: &str, peer: Expression, protocol: &str, p: Option<u16>) -> Block {
    let key = if kind.starts_with("ingress") { "source" } else { "destination" };
    let mut b = Block::builder(kind)
        .add_attribute(("description", string(description)))
        .add_attribute((key, peer))
        .add_attribute(("protocol", string(protocol)));
    if let Some(p) = p {
        b = b.add_block(port(if protocol == "17" { "udp_options" } else { "tcp_options" }, p));
    }
    b.build()
}

/// ICMP's "fragmentation needed" (type 3, code 4), from or to anywhere; with `v6`, ICMPv6's
/// "packet too big" (protocol 58, type 2, code 0), its IPv6 counterpart.
fn fragmentation(kind: &str, v6: bool) -> Block {
    let key = if kind.starts_with("ingress") { "source" } else { "destination" };
    let (anywhere, protocol, icmp, code) = if v6 { ("::/0", "58", 2, 0) } else { ("0.0.0.0/0", "1", 3, 4) };
    Block::builder(kind)
        .add_attribute(("description", string("path MTU discovery")))
        .add_attribute((key, string(anywhere)))
        .add_attribute(("protocol", string(protocol)))
        .add_block(
            Block::builder("icmp_options")
                .add_attribute(("type", number(icmp)))
                .add_attribute(("code", number(code)))
                .build(),
        )
        .build()
}

/// The rules of the `reach` rules between this network and others, one way.
fn reach_rules(kind: &str, rules: &[Rule<'_>], what: &str) -> Vec<Block> {
    let mut out = Vec::new();
    for r in rules {
        let peer = || string(r.cidr);
        let description = format!("reach {what} {}", r.network);
        if r.ports.is_empty() {
            out.push(rule(kind, &description, peer(), "all", None));
        }
        for p in r.ports {
            out.push(rule(kind, &description, peer(), "6", Some(*p)));
            out.push(rule(kind, &description, peer(), "17", Some(*p)));
        }
    }
    out
}

/// A network's security list: the IPv4 rules, then their IPv6 mirror on a dual-stack network.
fn security_list(env: &Environment, (net, p): (&str, &NetworkPlan)) -> Block {
    let cidr = p.cidr.as_str();
    let (inbound, outbound) = ("ingress_security_rules", "egress_security_rules");
    let allowed = || reference("var.allowed_cidr");
    let mut b = in_vcn(resource("oci_core_security_list", &ident(net)), display(net))
        .add_block(rule(inbound, "its own network", string(cidr), "all", None))
        .add_blocks(reach_rules(inbound, &reach::into(env, net), "from"))
        .add_block(rule(inbound, "SSH from allowed_cidr", allowed(), "6", Some(22)))
        .add_block(fragmentation(inbound, false))
        .add_block(rule(outbound, "its own network", string(cidr), "all", None))
        .add_blocks(reach_rules(outbound, &reach::out_of(env, net), "to"));
    for block in reach::public_blocks() {
        b = b.add_block(rule(outbound, "the internet", string(block), "all", None));
    }
    b = b.add_block(fragmentation(outbound, false));
    if let Some(v6) = p.ipv6.as_ref().map(|v| v.cidr.as_str()) {
        b = b
            .add_block(rule(inbound, "its own network (IPv6)", string(v6), "all", None))
            .add_blocks(reach_rules(inbound, &ipv6(env, reach::into(env, net)), "from"))
            .add_block(fragmentation(inbound, true))
            .add_block(rule(outbound, "its own network (IPv6)", string(v6), "all", None))
            .add_blocks(reach_rules(outbound, &ipv6(env, reach::out_of(env, net)), "to"))
            .add_block(fragmentation(outbound, true));
    }
    b.build()
}

/// The `reach` rules whose other network is dual-stack too, on its IPv6 block.
fn ipv6<'a>(env: &'a Environment, rules: Vec<Rule<'a>>) -> Vec<Rule<'a>> {
    rules
        .into_iter()
        .filter_map(|r| {
            let v6 = env.networks.get(r.network)?.ipv6.as_ref()?;
            Some(Rule { cidr: &v6.cidr, ..r })
        })
        .collect()
}

/// The private IP of the gateway's VNIC toward `net` (`gateways::toward`), looked up once the
/// VNIC exists.
fn gateway_ip(hop: &Hop, net: &str) -> Option<(String, Block)> {
    let (on, address) = toward(hop.machine, net)?;
    let id = format!("{}_{}", ident(hop.gateway), ident(on));
    let mut after = vec![reference(&format!("oci_core_instance.{}", ident(hop.gateway)))];
    if hop.machine.addresses.keys().next().is_some_and(|first| first != on) {
        after.push(reference(&format!("oci_core_vnic_attachment.{id}")));
    }
    let data = Block::builder("data")
        .add_label("oci_core_private_ips")
        .add_label(&id)
        .add_attribute(("ip_address", string(address.to_string())))
        .add_attribute(("subnet_id", reference(&format!("oci_core_subnet.{}.id", ident(on)))))
        .add_attribute(("depends_on", list(after)))
        .build();
    Some((id, data))
}

/// With a machine routing a network: each network's own route table, attached to its subnet
/// once the gateways exist, and the gateways' private IPs it names.
pub(super) fn gateway_routes(env: &Environment, m: &mut Module) {
    if routed(env).is_empty() {
        return;
    }
    let mut lookups: Vec<String> = Vec::new();
    for net in env.networks.keys() {
        let mut table = in_vcn(resource("oci_core_route_table", &ident(net)), display(net)).add_block(route_out());
        for hop in through(env, net) {
            let Some((id, data)) = gateway_ip(&hop, net) else { continue };
            if !lookups.contains(&id) {
                m.block(data);
                lookups.push(id.clone());
            }
            let ip = attr(index(reference(&format!("data.oci_core_private_ips.{id}.private_ips")), 0), "id");
            table = table.add_block(
                Block::builder("route_rules")
                    .add_attribute(("destination", string(hop.plan.cidr.as_str())))
                    .add_attribute(("network_entity_id", ip))
                    .build(),
            );
        }
        m.block(table.build());
        m.block(
            resource("oci_core_route_table_attachment", &ident(net))
                .add_attribute(("subnet_id", reference(&format!("oci_core_subnet.{}.id", ident(net)))))
                .add_attribute(("route_table_id", reference(&format!("oci_core_route_table.{}.id", ident(net)))))
                .build(),
        );
    }
}
