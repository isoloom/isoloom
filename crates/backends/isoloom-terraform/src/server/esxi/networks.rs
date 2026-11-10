//! The environment's networks on the host: a vSwitch of its own per network, with no uplink, and
//! a port group on it passing the machines' VLAN tags.

use crate::hcl_builder::{Module, number, reference};
use isoloom_plan::Environment;

use crate::module::{ident, resource};
use crate::server::slotted;

/// What network `net`'s vSwitch and port group are named after the environment's slot
/// (`iso<slot><suffix>`): the module's names, and the run manifest's.
pub(super) fn suffix(net: &str) -> String {
    format!("-{net}")
}

/// A vSwitch and a port group per network.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    for (net, plan) in &env.networks {
        let name = || slotted(&suffix(net));
        let mut vswitch = resource("esxi_vswitch", &ident(net)).add_attribute(("name", name()));
        if let Some(mtu) = plan.mtu {
            vswitch = vswitch.add_attribute(("mtu", number(mtu.into())));
        }
        m.commented(&format!("Network `{net}`: {}, on its own vSwitch with no uplink.", plan.cidr), vswitch.build());
        // VLAN 4095 passes the machines' own VLAN tags (trunks), and untagged frames as well.
        m.block(
            resource("esxi_portgroup", &ident(net))
                .add_attribute(("name", name()))
                .add_attribute(("vswitch", reference(&format!("esxi_vswitch.{}.name", ident(net)))))
                .add_attribute(("vlan", number(4095)))
                .build(),
        );
    }
}
