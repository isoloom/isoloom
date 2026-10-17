//! The router, as on Proxmox: an Ubuntu VM on the host's port group (DHCP) and on every network
//! at its router address, forwarding between networks with the `reach` rules and to the
//! internet (the same ruleset). VMware Tools reports its uplink address: the published ports
//! are reached there, and Terraform reaches the machines through it.

use isoloom_plan::{Environment, ipv6};
use isoloom_render::hcl_builder::{BinaryOperator, Block, Module, Template, binary, call, list, not, reference, string};
use isoloom_render::router;
use isoloom_spec::model::Target;
use isoloom_spec::validate::{Cidr, IPV6_PREFIX};

use super::SERVICE_OS;
use super::guest::{Guest, Nic};
use super::machines;
use crate::machine::cloud_init::{self, CloudConfig, file};
use crate::module::ident;
use crate::server::router::{forwarding, outputs as lab_outputs, ruleset_content};

/// The router's place among the VMs (the controller is next, then the machines).
pub(super) const INDEX: u64 = 0;

/// How long the provider waits for the router's uplink address.
const ADDRESS_WAIT_SECS: u64 = 600;

/// The router's VM.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    let mut config = CloudConfig::new("isoloom-router", reference("local.users"));
    config.packages = vec!["nftables".to_owned()];
    config.bootcmd.extend(cloud_init::prelude(env));
    config.files = vec![
        file("/etc/nftables.conf", None, ruleset_content(env)),
        file("/etc/sysctl.d/90-isoloom.conf", None, string(forwarding(env))),
    ];
    // The sealed ruleset as a second file (the live reload applies it; owed on ESXi until its
    // runner integration lands — the Terraform `esxi` target has no run manifest yet).
    config.files.extend(crate::server::router::sealed_file(env));
    config.runcmd = [
        "sysctl -p /etc/sysctl.d/90-isoloom.conf",
        "systemctl enable --now nftables",
        "nft -f /etc/nftables.conf",
    ]
    .map(String::from)
    .to_vec();
    let mut nics = vec![Nic::at(reference("var.uplink_port_group"), None)];
    nics.extend(env.networks.iter().map(|(net, plan)| {
        let len = Cidr::parse(&plan.cidr).map_or(32, |c| c.len);
        let mut nic = machines::nic(env, net, format!("{}/{len}", router::address(env.spec(), net)), false);
        // An IPv6 network's router address too: the machines' IPv6 routes go through it.
        nic.addresses
            .extend(ipv6::router_address(env.spec(), net).map(|a| format!("{a}/{IPV6_PREFIX}")));
        nic
    }));
    let guest = Guest {
        id: "isoloom_router".to_owned(),
        name: "router".to_owned(),
        index: INDEX,
        os: SERVICE_OS.to_owned(),
        size: (1, 512, 10),
        nics,
        dns: (Vec::new(), None),
        user_data: config.document(),
        wait_for_address: Some(ADDRESS_WAIT_SECS),
        depends_on: env.networks.keys().map(|n| reference(&format!("esxi_portgroup.{}", ident(n)))).collect(),
        firmware: None,
        guestos: None,
        disks: Vec::new(),
        windows: false,
        lifecycle: Some(uplink_checked(env)),
    };
    m.commented(
        "The router: forwards between networks with the reach rules, and to the internet. VMware Tools reports its uplink address.",
        guest.block(&env.name),
    );
}

/// The `lifecycle` block failing the apply when the router reports no uplink address: with no
/// DHCP on the uplink port group, VMware Tools reports one of its lab addresses instead.
fn uplink_checked(env: &Environment) -> Block {
    let own = list(env.networks.keys().map(|n| string(router::address(env.spec(), n).to_string())));
    let ip = || reference("self.ip_address");
    let condition = binary(
        binary(ip(), BinaryOperator::NotEq, string("")),
        BinaryOperator::And,
        not(call("contains", [own, ip()])),
    );
    let message = Template::text("The router got no address on its uplink port group `")
        .with_expr(reference("var.uplink_port_group"))
        .with_text("`: it has no DHCP (VMware Tools then reports a lab address). Give it an uplink with DHCP and internet (`uplink_port_group`; on a nested ESXi, promiscuous mode on).");
    crate::server::router::postcondition(condition, message)
}

/// The outputs: Proxmox's, from the router's uplink address.
pub(super) fn outputs(env: &Environment, m: &mut Module) {
    m.block(
        Block::builder("locals")
            .add_attribute(("router_address", reference("esxi_guest.isoloom_router.ip_address")))
            .build(),
    );
    lab_outputs(env, Target::Esxi, m);
}
