//! The networks: a VPC network per network, holding its one subnetwork with its exact range.
//!
//! An instance takes one interface per VPC network, so a VPC per network is what lets a machine
//! sit on several (an interface on each, up to 8). It is also what lets a machine route a
//! network (`gateway:`): a Google Cloud route can't be as specific as a subnetwork of its own
//! VPC, so within one VPC nothing goes through a machine, but a route in one VPC to another's
//! range can. So:
//!
//! - The networks no machine routes are peered with each other, every pair, as one VPC would
//!   have joined them; the machines' firewall rules still decide what crosses, as before.
//! - A network a machine routes is peered with none: its VPC sends every other network to its
//!   gateway's address there, and every other VPC the gateway is on sends it to the gateway's
//!   address in that one. The peerings exchange these routes, so a network the gateway isn't on
//!   reaches it through a peer the gateway is on.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, Expression, Module, boolean, list, number, reference, string};

use super::mtu;
use crate::cloud::instance::name_with;
use crate::cloud::vm::gateways::{routed, through};
use crate::module::{ident, resource};

/// Network `net`'s VPC network, by id.
pub(super) fn vpc(net: &str) -> Expression {
    reference(&format!("google_compute_network.{}.id", ident(net)))
}

/// The VPC networks and their subnetworks, the peerings between the networks no machine routes
/// and the routes through the gateways.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    let mut first = true;
    for (net, p) in &env.networks {
        let id = ident(net);
        let vpc = resource("google_compute_network", &id)
            .add_attribute(("project", reference("local.project")))
            .add_attribute(("name", name_with(&format!("-{net}"))))
            .add_attribute(("auto_create_subnetworks", boolean(false)))
            .add_attribute(("mtu", number(u64::from(mtu(p)))))
            .build();
        if std::mem::take(&mut first) {
            m.commented(
                "The environment's networks: a VPC network each, holding a subnetwork with its exact range.",
                vpc,
            );
        } else {
            m.block(vpc);
        }
        m.block(
            resource("google_compute_subnetwork", &id)
                .add_attribute(("project", reference("local.project")))
                .add_attribute(("name", name_with(&format!("-{net}"))))
                .add_attribute(("network", reference(&format!("google_compute_network.{id}.id"))))
                .add_attribute(("ip_cidr_range", string(p.cidr.as_str())))
                .add_attribute(("region", reference("var.region")))
                .build(),
        );
    }
    peerings(env, m);
    routes(env, m);
}

/// Every pair of the networks no machine routes, peered both ways and exchanging their routes
/// through the gateways. Google Cloud runs one peering change on a network at a time, so each
/// peering waits for the one before it.
fn peerings(env: &Environment, m: &mut Module) {
    let gateways = routed(env);
    let open: Vec<&str> = env
        .networks
        .keys()
        .map(String::as_str)
        .filter(|n| !gateways.iter().any(|(r, _)| r == n))
        .collect();
    let mut before: Option<String> = None;
    for (i, a) in open.iter().enumerate() {
        for b in open.iter().skip(i + 1) {
            for (from, to) in [(*a, *b), (*b, *a)] {
                let id = format!("{}_{}", ident(from), ident(to));
                let mut peering = resource("google_compute_network_peering", &id)
                    .add_attribute(("name", name_with(&format!("-{from}-{to}"))))
                    .add_attribute(("network", reference(&format!("google_compute_network.{}.self_link", ident(from)))))
                    .add_attribute(("peer_network", reference(&format!("google_compute_network.{}.self_link", ident(to)))))
                    .add_attribute(("export_custom_routes", boolean(true)))
                    .add_attribute(("import_custom_routes", boolean(true)));
                if let Some(prev) = before.replace(format!("google_compute_network_peering.{id}")) {
                    peering = peering.add_attribute(("depends_on", list([reference(&prev)])));
                }
                m.block(peering.build());
            }
        }
    }
}

/// A route in each VPC the gateway is on, for each network it reaches through that gateway: to
/// the gateway's address in that VPC.
fn routes(env: &Environment, m: &mut Module) {
    let mut first = true;
    for net in env.networks.keys() {
        for hop in through(env, net) {
            let Some(at) = hop.machine.addresses.get(net) else { continue };
            let route = route(net, (hop.network, hop.plan.cidr.as_str()), hop.gateway, &at.to_string());
            if std::mem::take(&mut first) {
                m.commented("The networks a machine routes: each VPC sends them to the gateway's address in it.", route);
            } else {
                m.block(route);
            }
        }
    }
}

/// The route in `net`'s VPC to `other` (its range) through `gateway` at address `at`.
fn route(net: &str, (other, range): (&str, &str), gateway: &str, at: &str) -> Block {
    resource("google_compute_route", &format!("{}_{}", ident(net), ident(other)))
        .add_attribute(("project", reference("local.project")))
        .add_attribute(("name", name_with(&format!("-{net}-to-{other}"))))
        .add_attribute(("network", vpc(net)))
        .add_attribute(("dest_range", string(range)))
        .add_attribute(("next_hop_ip", string(at)))
        .add_attribute(("depends_on", list([reference(&format!("google_compute_instance.{}", ident(gateway)))])))
        .build()
}
