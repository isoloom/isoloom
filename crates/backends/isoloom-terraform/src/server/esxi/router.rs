//! The router, as on Proxmox: an Ubuntu VM on the host's port group (DHCP) and on every network
//! at its router address, forwarding between networks with the `reach` rules and to the
//! internet (the same ruleset), and keeping the sealed one beside it from its first boot. VMware
//! Tools reports its uplink address: the published ports are reached there, and Terraform and the
//! runner reach the machines through it. Once the machines are set up, the run manifest's seal
//! applies the sealed ruleset over SSH, which closes the internet to the `internet: false`
//! networks (`crate::server::router`).

use crate::hcl_builder::{BinaryOperator, Block, Module, Template, binary, call, list, not, reference, string};
use isoloom_plan::Environment;
use isoloom_spec::model::Target;
use isoloom_spec::net::IPV6_PREFIX;

use super::SERVICE_OS;
use super::guest::{Guest, Nic};
use super::machines;
use crate::module::ident;
use crate::server::router::{NAME, Platform};

/// The router's place among the VMs (the controller is next, then the machines).
pub(super) const INDEX: u64 = 0;

/// How long the provider waits for the router's uplink address.
const ADDRESS_WAIT_SECS: u64 = 600;

/// The router's VM.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    let config = crate::server::router::cloud_config(env, Platform::default());
    let mut nics = vec![Nic::at(reference("var.uplink_port_group"), None)];
    nics.extend(env.networks.iter().map(|(net, plan)| {
        let len = plan.cidr.prefix();
        let mut nic = machines::nic(env, net, format!("{}/{len}", env.block(net).router()), false);
        // An IPv6 network's router address too: the machines' IPv6 routes go through it.
        nic.addresses.extend(env.router6_on(net).map(|a| format!("{a}/{IPV6_PREFIX}")));
        nic
    }));
    let guest = Guest {
        id: "isoloom_router".to_owned(),
        name: NAME.to_owned(),
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
    let sealed = if crate::seal::any(env) {
        " The run's seal closes the internet to the internet: false networks once the machines are set up."
    } else {
        ""
    };
    m.commented(
        &format!("The router: forwards between networks with the reach rules, and to the internet.{sealed} VMware Tools reports its uplink address."),
        guest.block(&env.name),
    );
}

/// The `lifecycle` block failing the apply when the router reports no uplink address: with no
/// DHCP on the uplink port group, VMware Tools reports one of its environment addresses instead.
fn uplink_checked(env: &Environment) -> Block {
    let own = list(env.networks.keys().map(|n| string(env.block(n).router().to_string())));
    let ip = || reference("self.ip_address");
    let condition = binary(
        binary(ip(), BinaryOperator::NotEq, string("")),
        BinaryOperator::And,
        not(call("contains", [own, ip()])),
    );
    let message = Template::text("The router got no address on its uplink port group `")
        .with_expr(reference("var.uplink_port_group"))
        .with_text("`: it has no DHCP (VMware Tools then reports one of the environment's addresses). Give it an uplink with DHCP and internet (`uplink_port_group`; on a nested ESXi, promiscuous mode on).");
    crate::server::router::postcondition(condition, message)
}

/// The outputs: the server router's, from its uplink address.
pub(super) fn outputs(env: &Environment, m: &mut Module) {
    m.block(
        Block::builder("locals")
            .add_attribute(("router_address", reference("esxi_guest.isoloom_router.ip_address")))
            .build(),
    );
    crate::server::router::outputs(env, Target::Esxi, m);
}
