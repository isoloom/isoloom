//! The router: a Debian VM on the uplink bridge (DHCP) and on every network at its router
//! address (matched by MAC), forwarding between networks with the `reach` rules, forwarding the
//! published ports from its uplink address, and giving every network the internet (NAT).

use std::fmt::Write as _;

use isoloom_plan::Environment;
use isoloom_plan::ipv6;
use isoloom_render::hcl_builder::{Expression, Module, Template, call, list, number, reference, string};
use isoloom_render::router;
use isoloom_spec::validate::{Cidr, IPV6_PREFIX};

use super::machine::{Boot, Vm, ip_config, nic, snippet};
use crate::vm::cloud_init::{self, CloudConfig, file};
use crate::vm::{controller, res};

/// The router's snippet and VM.
pub fn blocks(env: &Environment, m: &mut Module) {
    let mut config = CloudConfig::new("isoloom-router", reference("local.users"));
    config.packages = vec!["nftables".into(), "qemu-guest-agent".into()];
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
    m.commented(
        "The router: forwards between networks with the reach rules, and to the internet.",
        snippet("router", "router", config.document()),
    );
    let mut nics = vec![nic("var.uplink_bridge", Some(call("upper", [mac(None)])))];
    nics.extend(
        env.networks
            .keys()
            .enumerate()
            .map(|(i, n)| nic(&format!("proxmox_sdn_vnet.{}.id", res(n)), Some(call("upper", [mac(Some(i))])))),
    );
    let vm = Vm {
        id: "isoloom_router".into(),
        snippet: "router".into(),
        name: "router".into(),
        size: (1, 512, 8),
        boot: Boot::Image(res(controller::OS)),
        tpm: false,
        own_boot: None,
        agent: true,
        nics,
        dns: None,
        ips: vec![ip_config("dhcp", None)],
        depends_on: vec![reference("proxmox_sdn_applier.env")],
    };
    m.commented("The guest agent reports its uplink address (DHCP), for the published ports.", vm.block(env));
}

/// The router's MAC on its uplink (`None`) or on network `i`: unique per slot.
fn mac(i: Option<usize>) -> Expression {
    match i {
        None => call("format", [string("02:15:%02x:00:00:00"), reference("var.slot")]),
        Some(i) => call("format", [string("02:15:%02x:01:%02x:00"), reference("var.slot"), number(i as u64)]),
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
    for (i, (net, plan)) in env.networks.iter().enumerate() {
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

/// Forwarding on: IPv4, and IPv6 when a network is IPv6.
fn forwarding(env: &Environment) -> &'static str {
    if env.networks.values().any(|n| n.ipv6.is_some()) {
        "net.ipv4.ip_forward=1\nnet.ipv6.conf.all.forwarding=1\n"
    } else {
        "net.ipv4.ip_forward=1\n"
    }
}

/// The ruleset: the `reach` rules (the router's shared forward rules), the published ports
/// forwarded from the uplink address (anything not from the environment), and the internet for
/// every network (machines install their software; offline machines block it themselves once
/// set up).
fn ruleset(env: &Environment) -> String {
    let lab = format!("{{ {} }}", env.networks.values().map(|n| n.cidr.as_str()).collect::<Vec<_>>().join(", "));
    let mut forward: Vec<String> = router::forward_rules(env.spec());
    let mut dnat = String::new();
    for (name, m) in &env.spec().machines {
        let Some(addr) = env.machines.get(name).and_then(|mp| mp.addresses.values().next()) else {
            continue;
        };
        for sv in m.services.iter().filter(|s| s.publish.is_some()) {
            let (proto, port, host) = (sv.protocol.id(), sv.port, sv.publish.unwrap_or_default());
            forward.push(format!("ip daddr {addr} {proto} dport {port} ct status dnat accept"));
            let _ = writeln!(dnat, "    ip saddr != {lab} {proto} dport {host} dnat ip to {addr}:{port}");
        }
    }
    forward.push(format!("ip saddr {lab} ip daddr != {lab} accept"));
    let forward = forward.iter().fold(String::new(), |mut s, r| {
        let _ = writeln!(s, "    {r}");
        s
    });
    format!(
        "flush ruleset\ntable inet isoloom {{\n  chain forward {{\n    type filter hook forward priority 0; policy drop;\n{forward}  }}\n  chain prerouting {{\n    type nat hook prerouting priority -100;\n{dnat}  }}\n  chain postrouting {{\n    type nat hook postrouting priority 100;\n    ip saddr {lab} ip daddr != {lab} masquerade\n  }}\n}}\n"
    )
}
