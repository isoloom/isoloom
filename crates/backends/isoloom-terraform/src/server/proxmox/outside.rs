//! A machine's interfaces on networks outside the environment: an existing bridge of the server
//! (`external`), or a bridge to its LAN (`bridged`: the uplink bridge unless an interface is
//! named, tagged with the VLAN when one is given). Isoloom makes no VNet for them, and the
//! router and the controller aren't on them.

use crate::hcl_builder::{Block, number, reference, string};
use isoloom_plan::{Environment, Join, MachinePlan, OutsideAddress};

use super::machine::ip_config;

/// The device and the address of each outside network the machine joins, in its order.
pub(super) fn interfaces(env: &Environment, mp: &MachinePlan) -> Vec<(Block, Block)> {
    mp.outside
        .iter()
        .filter_map(|(net, address)| {
            let n = env.outside.get(net)?;
            let mut device = Block::builder("network_device");
            device = match &n.join {
                Join::Existing(bridge) => device.add_attribute(("bridge", string(bridge.as_str()))),
                Join::Bridged(b) => {
                    let bridge = b.interface.as_deref().map_or_else(|| reference("var.uplink_bridge"), string);
                    let device = device.add_attribute(("bridge", bridge));
                    match b.vlan {
                        Some(tag) => device.add_attribute(("vlan_id", number(tag.into()))),
                        None => device,
                    }
                }
            };
            let ip = match address {
                OutsideAddress::Fixed(ip) => format!("{ip}/{}", n.cidr.prefix()),
                OutsideAddress::Dhcp => "dhcp".to_owned(),
            };
            // The machine's rate limit holds on every link; reach rules don't reach outside.
            Some((super::links::limited(device, mp).build(), ip_config(&ip, None)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;

    #[test]
    fn existing_and_bridged_networks_are_devices_on_their_bridge() {
        let env = crate::testing::planned(
            "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.20.0.0/24 }\n  corp: { cidr: 10.50.0.0/24 }\n  home: { cidr: 192.168.1.0/24, bridged: { dhcp: true, default_route: true, vlan: 20 } }\nmachines:\n  a: { networks: { lan: 10, corp: 5, home: 0 }, os: debian@12 }\noverrides: { proxmox: { networks: { corp: { external: vmbr1 } } } }\n",
            Target::Proxmox,
        );
        let mp = &env.machines["a"];
        let mut body = crate::hcl_builder::Module::new("");
        for (device, ip) in interfaces(&env, mp) {
            body.block(device);
            body.block(ip);
        }
        let parsed = isoloom_test_support::hcl_body(&body.render().unwrap());
        // Each device's bridge and tag, each interface's address, in the networks' order.
        let devices: Vec<(String, Option<String>)> = isoloom_test_support::hcl_blocks(&parsed, "network_device", &[])
            .iter()
            .map(|d| {
                let tag = isoloom_test_support::hcl_attribute(d.body(), "vlan_id").map(ToString::to_string);
                (isoloom_test_support::hcl_text(d.body(), "bridge"), tag)
            })
            .collect();
        assert_eq!(devices, [("vmbr1".to_owned(), None), ("var.uplink_bridge".to_owned(), Some("20".to_owned()))]);
        let addresses = isoloom_test_support::hcl_values(&parsed, "address");
        assert_eq!(addresses, ["10.50.0.5/24", "dhcp"]);
        assert!(!crate::server::router::is_default_gateway(&env, mp));
    }
}
