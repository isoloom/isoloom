//! Networks routed by a machine (`gateway:`) on Azure. The virtual network routes between its
//! subnets itself, but a subnet's route table can send a range to a virtual appliance: a
//! machine's private address, on an interface with IP forwarding on. Each network then has its
//! route table, as on AWS: a network a machine routes goes through that machine, and a routed
//! network sends every other network through its own gateway, so traffic crosses the gateway
//! both ways.
//!
//! The machines keep their own public addresses (Terraform sets them up over SSH there), so a
//! routed network still goes out to the internet directly rather than through its gateway.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Module, reference, string};

use super::network::placed;
use crate::cloud::instance::name_with;
use crate::cloud::vm::gateways::{routed, through, toward};
use crate::module::{ident, resource};

/// With a machine routing a network: each network's route table, its routes through the
/// gateways and its association with the network's subnet.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    if routed(env).is_empty() {
        return;
    }
    let mut first = true;
    for net in env.networks.keys() {
        let id = ident(net);
        let table = placed("azurerm_route_table", &id, name_with(&format!("-{net}"))).build();
        if std::mem::take(&mut first) {
            m.commented("A route table per network: the networks a machine routes go through its address.", table);
        } else {
            m.block(table);
        }
        for hop in through(env, net) {
            // The gateway's address toward `net` (one virtual network: any of its interfaces is
            // reachable from every subnet).
            let next_hop = toward(hop.machine, net).map(|(_, a)| a.to_string()).unwrap_or_default();
            m.block(
                resource("azurerm_route", &format!("{id}_{}", ident(hop.network)))
                    .add_attribute(("name", string(format!("to-{}", hop.network))))
                    .add_attribute(("resource_group_name", reference("azurerm_resource_group.env.name")))
                    .add_attribute(("route_table_name", reference(&format!("azurerm_route_table.{id}.name"))))
                    .add_attribute(("address_prefix", string(hop.plan.cidr.as_str())))
                    .add_attribute(("next_hop_type", string("VirtualAppliance")))
                    .add_attribute(("next_hop_in_ip_address", string(next_hop)))
                    .build(),
            );
        }
        m.block(
            resource("azurerm_subnet_route_table_association", &id)
                .add_attribute(("subnet_id", reference(&format!("azurerm_subnet.{id}.id"))))
                .add_attribute(("route_table_id", reference(&format!("azurerm_route_table.{id}.id"))))
                .build(),
        );
    }
}
