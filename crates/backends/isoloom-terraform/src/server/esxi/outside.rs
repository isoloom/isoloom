//! A machine's interfaces on networks outside the environment: an existing port group of the
//! host (`external`), or the host's LAN (`bridged`). A bridged network is a port group of the
//! environment's on the host's own vSwitch (`uplink_vswitch`), or, when it names an interface,
//! on a vSwitch of its own whose uplink is that physical NIC (`vmnic1`, not on another
//! vSwitch); tagged with the VLAN when one is given. The router and the controller aren't on
//! them, as on Proxmox.

use isoloom_plan::{Environment, Join, MachinePlan, OutsideAddress};
use isoloom_render::hcl_builder::{Block, Expression, Module, number, reference, string};
use isoloom_spec::validate::Cidr;

use super::guest::Nic;
use crate::module::ident;
use crate::server::slotted;

/// Whether a bridged network rides the host's own vSwitch (it names no interface): the module
/// then has the `uplink_vswitch` variable.
pub(super) fn on_host_vswitch(env: &Environment) -> bool {
    env.outside.values().any(|n| matches!(&n.join, Join::Bridged(b) if b.interface.is_none()))
}

/// The vSwitch on a physical NIC's resource name.
fn vswitch(interface: &str) -> String {
    format!("uplink_{}", ident(interface))
}

/// The bridged networks' vSwitches (one per physical NIC named) and port groups.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    let mut nics: Vec<&str> = Vec::new();
    for (net, n) in &env.outside {
        let Join::Bridged(b) = &n.join else { continue };
        let on = match b.interface.as_deref() {
            Some(nic) => {
                if !nics.contains(&nic) {
                    nics.push(nic);
                    m.commented(
                        &format!("The host's NIC `{nic}`, on a vSwitch of its own: the LAN of the bridged networks on it."),
                        Block::builder("resource")
                            .add_label("esxi_vswitch")
                            .add_label(vswitch(nic))
                            .add_attribute(("name", slotted(&format!("-{nic}"))))
                            .add_block(Block::builder("uplink").add_attribute(("name", string(nic))).build())
                            .build(),
                    );
                }
                reference(&format!("esxi_vswitch.{}.name", vswitch(nic)))
            }
            None => reference("var.uplink_vswitch"),
        };
        m.commented(
            &format!("Network `{net}`: {}, bridged to the host's LAN.", n.cidr),
            Block::builder("resource")
                .add_label("esxi_portgroup")
                .add_label(ident(net))
                .add_attribute(("name", slotted(&format!("-{net}"))))
                .add_attribute(("vswitch", on))
                .add_attribute(("vlan", number(b.vlan.unwrap_or(0).into())))
                .build(),
        );
    }
}

/// The machine's interfaces on the outside networks it joins, in its order.
pub(super) fn nics(env: &Environment, mp: &MachinePlan) -> Vec<Nic> {
    mp.outside
        .iter()
        .filter_map(|(net, address)| {
            let n = env.outside.get(net)?;
            let (port_group, routes): (Expression, bool) = match &n.join {
                Join::Existing(group) => (string(group.as_str()), false),
                Join::Bridged(b) => (reference(&format!("esxi_portgroup.{}.name", ident(net))), b.default_route),
            };
            let address = match address {
                OutsideAddress::Fixed(ip) => Some(format!("{ip}/{}", Cidr::parse(&n.cidr).map_or(32, |c| c.len))),
                OutsideAddress::Dhcp => None,
            };
            let mut nic = Nic::at(port_group, address);
            nic.dhcp_routes = routes;
            Some(nic)
        })
        .collect()
}
