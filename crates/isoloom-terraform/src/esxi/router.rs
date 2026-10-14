//! The router, as on Proxmox: an Ubuntu VM on the host's port group (DHCP) and on every network
//! at its router address, forwarding between networks with the `reach` rules and to the
//! internet (the same ruleset). VMware Tools reports its uplink address: the published ports
//! are reached there, and Terraform reaches the machines through it.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, Module, reference, string};
use isoloom_render::router;
use isoloom_spec::validate::Cidr;

use super::guest::{Guest, Nic};
use super::{DIR, SERVICE_OS};
use crate::proxmox::outputs::behind_router;
use crate::proxmox::router::{forwarding, ruleset};
use crate::vm::cloud_init::{self, CloudConfig, file};
use crate::vm::res;

/// The router's place among the VMs (the controller is next, then the machines).
pub const INDEX: usize = 0;

/// How long the provider waits for the router's uplink address.
const ADDRESS_WAIT_SECS: u64 = 600;

/// The router's VM.
pub fn blocks(env: &Environment, m: &mut Module) {
    let mut config = CloudConfig::new("isoloom-router", reference("local.users"));
    config.packages = vec!["nftables".into()];
    config.bootcmd.extend(cloud_init::prelude(env));
    config.files = vec![
        file("/etc/nftables.conf", None, string(ruleset(env))),
        file("/etc/sysctl.d/90-isoloom.conf", None, string(forwarding(env))),
    ];
    config.runcmd = [
        "sysctl -p /etc/sysctl.d/90-isoloom.conf",
        "systemctl enable --now nftables",
        "nft -f /etc/nftables.conf",
    ]
    .map(String::from)
    .to_vec();
    let mut nics = vec![Nic {
        port_group: reference("var.uplink_port_group"),
        address: None,
        gateway: None,
    }];
    nics.extend(env.networks.iter().map(|(net, plan)| {
        let len = Cidr::parse(&plan.cidr).map_or(32, |c| c.len);
        Nic {
            port_group: reference(&format!("esxi_portgroup.{}.name", res(net))),
            address: Some(format!("{}/{len}", router::address(env.spec(), net))),
            gateway: None,
        }
    }));
    let guest = Guest {
        id: "isoloom_router".into(),
        name: "router".into(),
        index: INDEX,
        os: SERVICE_OS.into(),
        size: (1, 512, 10),
        nics,
        dns: (Vec::new(), None),
        user_data: config.document(),
        wait_for_address: Some(ADDRESS_WAIT_SECS),
        depends_on: env.networks.keys().map(|n| reference(&format!("esxi_portgroup.{}", res(n)))).collect(),
    };
    m.commented(
        "The router: forwards between networks with the reach rules, and to the internet. VMware Tools reports its uplink address.",
        guest.block(&env.name),
    );
}

/// The outputs: Proxmox's, from the router's uplink address.
pub fn outputs(env: &Environment, m: &mut Module) {
    m.block(
        Block::builder("locals")
            .add_attribute(("router_address", reference("esxi_guest.isoloom_router.ip_address")))
            .build(),
    );
    behind_router(env, DIR, m);
}
