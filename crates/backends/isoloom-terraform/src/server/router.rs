//! The router of a server's environment (Proxmox, ESXi): a VM on the server's own network
//! (DHCP) and on every network at its router address. It forwards between networks with the
//! `reach` rules and gives them the internet ([`ruleset`], [`forwarding`]), and everything is
//! reached through it from outside ([`outputs`]): each published service at its uplink address,
//! each machine over SSH by it. Each platform makes the VM itself.

use isoloom_plan::checks::Position;
use isoloom_plan::{Environment, Join, MachinePlan, OutsideAddress};
use isoloom_render::hcl_builder::{Expression, Module, Template, object, reference, string};
use isoloom_render::router;
use isoloom_spec::model::Target;
use isoloom_spec::validate::Cidr;

use crate::checks::{Invocation, Runner, output_block};
use crate::machine::{controller, vms};
use crate::module::output;

/// Forwarding on: IPv4, and IPv6 when a network is IPv6.
pub(crate) fn forwarding(env: &Environment) -> &'static str {
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
pub(crate) fn ruleset(env: &Environment) -> String {
    let lab = format!("{{ {} }}", env.networks.values().map(|n| n.cidr.as_str()).collect::<Vec<_>>().join(", "));
    let mut forward: Vec<String> = router::forward_rules(env.spec());
    let mut dnat = Vec::new();
    for (name, m) in &env.spec().machines {
        let Some(addr) = env.machines.get(name).and_then(|mp| mp.addresses.values().next()) else {
            continue;
        };
        for (sv, host) in m.ports.iter().filter_map(|s| Some((s, s.publish?))) {
            let (proto, port) = (sv.protocol.id(), sv.port);
            forward.push(format!("ip daddr {addr} {proto} dport {port} ct status dnat accept"));
            dnat.push(format!("    ip saddr != {lab} {proto} dport {host} dnat ip to {addr}:{port}\n"));
        }
    }
    forward.push(format!("ip saddr {lab} ip daddr != {lab} accept"));
    let forward = forward.iter().map(|r| format!("    {r}\n")).collect::<Vec<_>>().concat();
    let dnat = dnat.concat();
    format!(
        "flush ruleset\ntable inet isoloom {{\n  chain forward {{\n    type filter hook forward priority 0; policy drop;\n{forward}  }}\n  chain prerouting {{\n    type nat hook prerouting priority -100;\n{dnat}  }}\n  chain postrouting {{\n    type nat hook postrouting priority 100;\n    ip saddr {lab} ip daddr != {lab} masquerade\n  }}\n}}\n"
    )
}

/// Whether the router is the machine's default gateway: unless a bridged LAN's DHCP gives it its
/// default route (`default_route`).
pub(crate) fn is_default_gateway(env: &Environment, mp: &MachinePlan) -> bool {
    !mp.outside
        .iter()
        .any(|(net, a)| *a == OutsideAddress::Dhcp && env.outside.get(net).is_some_and(|n| matches!(&n.join, Join::Bridged(b) if b.default_route)))
}

/// The outputs of `target`'s module, once `local.router_address` holds the router's uplink
/// address: the address (`ip`), the machines' ready file, the published services there, the
/// machines through it, the checks.
pub(crate) fn outputs(env: &Environment, target: Target, m: &mut Module) {
    let dir = target.id();
    m.commented(
        "The router's uplink address: the lab's way in.",
        output("ip", reference("local.router_address")),
    );
    m.commented(
        "Each Linux machine's set-up progress (`isoloom-ready/1`: `running: <step>`, `failed: <step>`, `ready`).",
        output("ready_file", string(crate::machine::cloud_init::READY_FILE)),
    );
    let published: Vec<(String, Expression)> = env
        .spec()
        .machines
        .iter()
        .filter(|(_, machine)| !machine.networks.is_empty())
        .flat_map(|(name, machine)| {
            machine.ports.iter().filter_map(move |sv| {
                let h = sv.publish?;
                let at = Template::new().with_expr(reference("local.router_address")).with_text(format!(":{h}"));
                Some((format!("{name}/{}", machine.port_label(sv)), at.into_expression()))
            })
        })
        .collect();
    if !published.is_empty() {
        m.commented("Published services, from the router's uplink address.", output("published", object(published)));
    }
    let machines: Vec<(String, Expression)> = env
        .spec()
        .machines
        .iter()
        .filter(|(_, machine)| machine.vm.is_some())
        .filter_map(|(n, _)| first_address(env, n).map(|a| (n.clone(), string(a))))
        .collect();
    m.commented(
        "The machines, through the router: ssh -J isoloom@<ip> isoloom@<machine>.",
        output("machines", object(machines)),
    );
    m.block(output("ssh_user", string("isoloom")));
    if let Some(checks) = output_block(env, dir, Invocation::Script, &|p| runner(env, p)) {
        m.commented(
            "The checks: each runner piped to its machine (ssh -J isoloom@<ip> isoloom@<host> sh -s < <script>).",
            checks,
        );
    }
}

/// A machine's first address.
fn first_address(env: &Environment, name: &str) -> Option<String> {
    env.machines.get(name)?.addresses.values().next().map(ToString::to_string)
}

/// Where a position's checks run, as the machine's name and address: on its machine when it is a
/// VM, else from the controller, else from the first machine.
pub(crate) fn check_host(env: &Environment, p: &Position) -> Option<(String, String)> {
    if let Position::Machine(m) = p
        && env.spec().machines.get(m).is_some_and(|x| x.vm.is_some())
    {
        return first_address(env, m).map(|a| (m.clone(), a));
    }
    if controller::needed(env) {
        let first = env.networks.values().next().and_then(|n| Cidr::parse(&n.cidr))?;
        return Some(("controller".to_owned(), first.controller().to_string()));
    }
    vms(env).find_map(|(n, _)| first_address(env, n).map(|a| (n.to_owned(), a)))
}

/// [`check_host`], as the `checks` output's runner (the `isoloom` user).
fn runner(env: &Environment, p: &Position) -> Option<Runner> {
    check_host(env, p).map(|(machine, host)| Runner {
        machine,
        host: string(host),
        user: string("isoloom"),
    })
}
