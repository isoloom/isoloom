//! The router of a server's environment (Proxmox, ESXi): a VM on the server's own network
//! (DHCP) and on every network at its router address. It forwards between networks with the
//! `reach` rules and gives them the internet ([`ruleset`], [`forwarding`]), and everything is
//! reached through it from outside ([`outputs`]): each published service at its uplink address,
//! each machine over SSH by it. Each platform makes the VM itself.

use isoloom_plan::checks::Position;
use isoloom_plan::{Environment, Join, MachinePlan, OutsideAddress};
use isoloom_render::hcl_builder::{Block, Expression, Module, Template, object, reference, string};
use isoloom_render::router;
use isoloom_spec::model::Target;
use isoloom_spec::validate::Cidr;

use crate::checks::{Invocation, Runner, output_block};
use crate::cloud::vm::seal;
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

/// The `/etc/nftables.conf` content: always the unsealed ruleset (the `reach` rules, the
/// published ports forwarded from the uplink address, and the internet for every network). It
/// never depends on `isoloom_sealed`, so the seal's second apply leaves the router's cloud-init
/// snippet untouched (a changed snippet forces the VM to be recreated). The seal is applied live
/// instead, from [`SEALED_NFT`], by the runner's seal-reload step ([`sealed_reload`]).
pub(crate) fn ruleset_content(env: &Environment) -> Expression {
    string(rules(env, false))
}

/// Where the router keeps the sealed ruleset as a second, inert file. The runner's seal step
/// applies it live to the already-booted (unsealed) router: updating the host-side cloud-init
/// snippet alone never reaches a running guest, unlike the clouds' control-plane seal.
pub(crate) const SEALED_NFT: &str = "/etc/nftables.sealed.nft";

/// The sealed ruleset as the router's `SEALED_NFT` file, when the run has something to seal (left
/// out otherwise, so an unsealed lab's router is unchanged). Written unconditionally so the seal's
/// second apply never changes it (and so never recreates the router).
pub(crate) fn sealed_file(env: &Environment) -> Option<Expression> {
    seal::any(env).then(|| crate::machine::cloud_init::file(SEALED_NFT, None, string(rules(env, true))))
}

/// The command the seal step runs on the router over SSH: apply the sealed ruleset (fail-closed —
/// an invalid ruleset isn't kept), then keep it as `/etc/nftables.conf` so a reboot stays sealed.
#[must_use]
pub(crate) fn sealed_reload() -> String {
    format!("sudo sh -c 'nft -f {SEALED_NFT} && cp {SEALED_NFT} /etc/nftables.conf'")
}

/// The networks the router forwards and masquerades to the internet: every one, or, once
/// `sealed`, only those with internet (`internet: false` networks lose their egress and NAT at
/// the router). `None` leaves the forward and masquerade rules out (every network is offline).
fn forwarded(env: &Environment, sealed: bool) -> Option<String> {
    let cidrs: Vec<&str> = env
        .networks
        .iter()
        .filter(|(n, _)| !(sealed && seal::closed(env, n)))
        .map(|(_, p)| p.cidr.as_str())
        .collect();
    (!cidrs.is_empty()).then(|| format!("{{ {} }}", cidrs.join(", ")))
}

/// The ruleset, sealed or not (see [`ruleset_content`]). The lab's whole set is
/// the DNAT's source exclusion (published ports reach a machine whatever its network); once
/// sealed, traffic to an `internet: false` network from elsewhere is dropped (so it keeps only
/// its `reach` rules), and that network is dropped from the internet forward and masquerade.
fn rules(env: &Environment, sealed: bool) -> String {
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
    if sealed {
        let closed: Vec<&str> = env
            .networks
            .iter()
            .filter(|(n, _)| seal::closed(env, n))
            .map(|(_, p)| p.cidr.as_str())
            .collect();
        if !closed.is_empty() {
            forward.push(format!("ip daddr {{ {} }} drop", closed.join(", ")));
        }
    }
    let out = forwarded(env, sealed);
    if let Some(out) = &out {
        forward.push(format!("ip saddr {out} ip daddr != {out} accept"));
    }
    let forward = forward.iter().map(|r| format!("    {r}\n")).collect::<Vec<_>>().concat();
    let dnat = dnat.concat();
    let masquerade = out.map(|out| format!("    ip saddr {out} ip daddr != {out} masquerade\n")).unwrap_or_default();
    format!(
        "flush ruleset\ntable inet isoloom {{\n  chain forward {{\n    type filter hook forward priority 0; policy drop;\n{forward}  }}\n  chain prerouting {{\n    type nat hook prerouting priority -100;\n{dnat}  }}\n  chain postrouting {{\n    type nat hook postrouting priority 100;\n{masquerade}  }}\n}}\n"
    )
}

/// The `lifecycle` block failing the apply with `message` unless `condition` holds: what a VM
/// reported of its uplink (its address).
pub(crate) fn postcondition(condition: Expression, message: Template) -> Block {
    let check = Block::builder("postcondition")
        .add_attribute(("condition", condition))
        .add_attribute(("error_message", message.into_expression()))
        .build();
    Block::builder("lifecycle").add_block(check).build()
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
