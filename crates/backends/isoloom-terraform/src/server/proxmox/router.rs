//! The router on Proxmox (`crate::server::router`): a Debian VM on the uplink bridge (DHCP) and
//! on every network at its router address (matched by MAC). The guest agent reports its uplink
//! address, which the outputs give.

use isoloom_plan::Environment;
use isoloom_plan::ipv6;
use isoloom_render::hcl_builder::{Block, Expression, Module, Template, call, list, number, reference, string};
use isoloom_render::router;
use isoloom_spec::model::Target;
use isoloom_spec::validate::{Cidr, IPV6_PREFIX};

use super::machine::{Boot, Vm, ip_config, nic, snippet};
use crate::machine::cloud_init::{self, CloudConfig, file};
use crate::machine::controller;
use crate::module::ident;
use crate::server::router::{forwarding, ruleset};

/// The router's snippet and VM.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    let mut config = CloudConfig::new("isoloom-router", reference("local.users"));
    config.packages = vec!["nftables".to_owned(), "qemu-guest-agent".to_owned()];
    config.bootcmd.extend(cloud_init::prelude(env));
    config.files = files(env);
    config.runcmd = [
        "sysctl -p /etc/sysctl.d/90-isoloom.conf",
        "systemctl enable systemd-networkd",
        "systemctl restart systemd-networkd",
        "systemctl enable --now nftables",
        "nft -f /etc/nftables.conf",
        "systemctl enable --now qemu-guest-agent",
    ]
    .map(String::from)
    .to_vec();
    if let Some((files, start)) = super::lxc::router_dns(env) {
        // The LXC containers' resolver: the environment's names.
        config.packages.push("dnsmasq".to_owned());
        config.files.extend(files);
        config.runcmd.push(start.to_owned());
    }
    m.commented(
        "The router: forwards between networks with the reach rules, and to the internet.",
        snippet("router", "router", config.document()),
    );
    let mut nics = vec![nic("var.uplink_bridge", Some(call("upper", [mac(None)])))];
    nics.extend(
        (0..)
            .zip(env.networks.keys())
            .map(|(i, n)| super::links::device(env, n, Some(call("upper", [mac(Some(i))])), None)),
    );
    let vm = Vm {
        id: "isoloom_router".to_owned(),
        snippet: "router".to_owned(),
        name: "router".to_owned(),
        size: (1, 512, 8),
        boot: Boot::Image(ident(controller::OS)),
        tpm: false,
        own_boot: None,
        arm: false,
        agent: Some(own_addresses(env)),
        nics,
        pci: Vec::new(),
        dns: None,
        ips: vec![ip_config("dhcp", None)],
        depends_on: vec![reference("proxmox_sdn_applier.env")],
        data_disks: Vec::new(),
        folders: Vec::new(),
    };
    m.commented("The guest agent reports its uplink address (DHCP), for the published ports.", vm.block(env));
}

/// The router's MAC on its uplink (`None`) or on network `i`: unique per slot.
fn mac(i: Option<u64>) -> Expression {
    match i {
        None => call("format", [string("02:15:%02x:00:00:00"), reference("var.slot")]),
        Some(i) => call("format", [string("02:15:%02x:01:%02x:00"), reference("var.slot"), number(i)]),
    }
}

/// Its files: the uplink's and each network's interface, the ruleset, forwarding on.
fn files(env: &Environment) -> Vec<Expression> {
    let network = |m: Expression, rest: &[&str]| {
        let mut lines = vec![
            string("[Match]"),
            Template::text("MACAddress=").with_expr(m).into_expression(),
            string(""),
            string("[Network]"),
        ];
        lines.extend(rest.iter().map(|l| string(*l)));
        call("join", [string("\n"), list(lines)])
    };
    let mut out = vec![
        file("/etc/systemd/network/10-wan.network", None, network(mac(None), &["DHCP=yes"])),
        file("/etc/nftables.conf", None, string(ruleset(env))),
        file("/etc/sysctl.d/90-isoloom.conf", None, string(forwarding(env))),
    ];
    for (i, (net, plan)) in (0..).zip(&env.networks) {
        let len = Cidr::parse(&plan.cidr).map_or(32, |c| c.len);
        let address = format!("Address={}/{len}", router::address(env.spec(), net));
        // An IPv6 network's router address too (the routes6 of the machines go through it).
        let address6 = ipv6::router_address(env.spec(), net).map(|a| format!("Address={a}/{IPV6_PREFIX}"));
        let mut lines = vec![address.as_str()];
        lines.extend(address6.as_deref());
        lines.push("ConfigureWithoutCarrier=yes");
        out.push(file(&format!("/etc/systemd/network/20-{net}.network"), None, network(mac(Some(i)), &lines)));
    }
    out
}

/// The router's own addresses on the environment's networks.
fn own_addresses(env: &Environment) -> Vec<String> {
    env.networks.keys().map(|n| router::address(env.spec(), n).to_string()).collect()
}

/// The outputs, from `local.router_address`: the router's address that is none of its
/// environment addresses.
pub(super) fn outputs(env: &Environment, m: &mut Module) {
    let first = super::uplink::first_address("proxmox_virtual_environment_vm.isoloom_router", &own_addresses(env));
    m.block(Block::builder("locals").add_attribute(("router_address", first)).build());
    crate::server::router::outputs(env, Target::Proxmox, m);
}
