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

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, attr, index, list, number, reference, string};

use super::{named, resource};
use crate::vm::gateways::routed;
use crate::vm::reach::{self, Rule};
use crate::vm::res;

/// A VCN holds five blocks at most.
const BLOCKS: usize = 5;

/// Why the VCN can't hold the networks, if it can't.
pub fn refusal(env: &Environment) -> Option<String> {
    (env.networks.len() > BLOCKS).then(|| format!("an Oracle Cloud VCN holds {BLOCKS} address blocks at most, one per network"))
}

fn in_vcn(b: BlockBuilder, name: Expression) -> BlockBuilder {
    named(b)
        .add_attribute(("vcn_id", reference("oci_core_vcn.env.id")))
        .add_attribute(("display_name", name))
}

fn display(net: &str) -> Expression {
    crate::vm::cloud::name_with(&format!("-{net}"))
}

/// The VCN, its internet gateway and route tables, a security list and a subnet per network.
pub fn blocks(env: &Environment, m: &mut Module) {
    let cidrs: Vec<&str> = env.networks.values().map(|n| n.cidr.as_str()).collect();
    let vcn = named(resource("oci_core_vcn", "env")).add_attribute(("display_name", reference("local.name")));
    let vcn = match cidrs.as_slice() {
        [one] => vcn.add_attribute(("cidr_block", string(*one))),
        many => vcn.add_attribute(("cidr_blocks", list(many.iter().map(|c| string(*c))))),
    };
    m.commented(
        "The environment's networks: a VCN holding each one's block, routed to the internet.",
        vcn.build(),
    );
    m.block(in_vcn(resource("oci_core_internet_gateway", "env"), reference("local.name")).build());
    m.block(
        in_vcn(resource("oci_core_route_table", "env"), reference("local.name"))
            .add_block(internet())
            .build(),
    );
    for (net, p) in &env.networks {
        m.commented(
            &format!("Network `{net}`: what may reach its machines, and where they may go."),
            security_list(env, net, &p.cidr),
        );
        let mut subnet = named(resource("oci_core_subnet", &res(net)))
            .add_attribute(("vcn_id", reference("oci_core_vcn.env.id")))
            .add_attribute(("cidr_block", string(p.cidr.as_str())))
            .add_attribute(("display_name", display(net)))
            .add_attribute(("route_table_id", reference("oci_core_route_table.env.id")))
            .add_attribute(("security_list_ids", list([reference(&format!("oci_core_security_list.{}.id", res(net)))])));
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

fn internet() -> Block {
    Block::builder("route_rules")
        .add_attribute(("destination", string("0.0.0.0/0")))
        .add_attribute(("network_entity_id", reference("oci_core_internet_gateway.env.id")))
        .build()
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

/// ICMP's "fragmentation needed" (type 3, code 4), from or to anywhere.
fn fragmentation(kind: &str) -> Block {
    let key = if kind.starts_with("ingress") { "source" } else { "destination" };
    Block::builder(kind)
        .add_attribute(("description", string("path MTU discovery")))
        .add_attribute((key, string("0.0.0.0/0")))
        .add_attribute(("protocol", string("1")))
        .add_block(
            Block::builder("icmp_options")
                .add_attribute(("type", number(3)))
                .add_attribute(("code", number(4)))
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

/// A network's security list.
fn security_list(env: &Environment, net: &str, cidr: &str) -> Block {
    let (inbound, outbound) = ("ingress_security_rules", "egress_security_rules");
    let allowed = || reference("var.allowed_cidr");
    let mut b = in_vcn(resource("oci_core_security_list", &res(net)), display(net))
        .add_block(rule(inbound, "its own network", string(cidr), "all", None))
        .add_blocks(reach_rules(inbound, &reach::into(env, net), "from"))
        .add_block(rule(inbound, "SSH from allowed_cidr", allowed(), "6", Some(22)))
        .add_block(fragmentation(inbound))
        .add_block(rule(outbound, "its own network", string(cidr), "all", None))
        .add_blocks(reach_rules(outbound, &reach::out_of(env, net), "to"));
    for block in reach::public_blocks() {
        b = b.add_block(rule(outbound, "the internet", string(block), "all", None));
    }
    b.add_block(fragmentation(outbound)).build()
}

/// The private IP of machine `gateway`'s VNIC toward `net` (on it, else its first), looked up
/// once the VNIC exists.
fn gateway_ip(env: &Environment, gateway: &str, net: &str) -> (String, Block) {
    let addresses = &env.machines[gateway].addresses;
    let on = if addresses.contains_key(net) {
        net
    } else {
        addresses.keys().next().map_or(net, String::as_str)
    };
    let id = format!("{}_{}", res(gateway), res(on));
    let mut after = vec![reference(&format!("oci_core_instance.{}", res(gateway)))];
    if addresses.keys().next().is_some_and(|first| first != on) {
        after.push(reference(&format!("oci_core_vnic_attachment.{id}")));
    }
    let data = Block::builder("data")
        .add_label("oci_core_private_ips")
        .add_label(&id)
        .add_attribute(("ip_address", string(addresses[on].to_string())))
        .add_attribute(("subnet_id", reference(&format!("oci_core_subnet.{}.id", res(on)))))
        .add_attribute(("depends_on", list(after)))
        .build();
    (id, data)
}

/// With a machine routing a network: each network's own route table, attached to its subnet
/// once the gateways exist, and the gateways' private IPs it names.
pub fn gateway_routes(env: &Environment, m: &mut Module) {
    let gateways = routed(env);
    if gateways.is_empty() {
        return;
    }
    let mut lookups: Vec<String> = Vec::new();
    for net in env.networks.keys() {
        let own = gateways.iter().find(|(n, _)| *n == net.as_str()).map(|(_, g)| *g);
        let mut table = in_vcn(resource("oci_core_route_table", &res(net)), display(net)).add_block(internet());
        for (other, p) in env.networks.iter().filter(|(n, _)| *n != net) {
            let Some(gateway) = own.or_else(|| gateways.iter().find(|(n, _)| *n == other.as_str()).map(|(_, g)| *g)) else {
                continue;
            };
            let (id, data) = gateway_ip(env, gateway, net);
            if !lookups.contains(&id) {
                m.block(data);
                lookups.push(id.clone());
            }
            let ip = attr(index(reference(&format!("data.oci_core_private_ips.{id}.private_ips")), 0), "id");
            table = table.add_block(
                Block::builder("route_rules")
                    .add_attribute(("destination", string(p.cidr.as_str())))
                    .add_attribute(("network_entity_id", ip))
                    .build(),
            );
        }
        m.block(table.build());
        m.block(
            resource("oci_core_route_table_attachment", &res(net))
                .add_attribute(("subnet_id", reference(&format!("oci_core_subnet.{}.id", res(net)))))
                .add_attribute(("route_table_id", reference(&format!("oci_core_route_table.{}.id", res(net)))))
                .build(),
        );
    }
}
