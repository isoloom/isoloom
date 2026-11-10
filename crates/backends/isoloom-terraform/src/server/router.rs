//! The router of a server's environment (Proxmox, ESXi): a VM on the server's own network
//! (DHCP) and on every network at its router address. It forwards between networks with the
//! `reach` rules and gives them the internet ([`ruleset_content`], [`forwarding`]), until the run
//! manifest's seal closes it to the `internet: false` networks ([`sealed_reload`]); and everything
//! is reached through it from outside ([`outputs`]): each published service at its uplink address,
//! each machine over SSH by it. Each platform makes the VM itself.

use crate::hcl_builder::{Block, Expression, Module, Template, object, reference, string};
use isoloom_plan::checks::Position;
use isoloom_plan::{Environment, Join, MachinePlan, MachineShape, OutsideAddress};
use isoloom_render::router;
use isoloom_spec::model::Target;

use crate::checks::{Invocation, Runner, output_block};
use crate::machine::cloud_init::{self, CloudConfig, file};
use crate::machine::controller;
use crate::module::output;
use crate::seal;

/// The router's name: its host name, its VM's and snippet's, and its machine in a run manifest.
/// A machine of the environment can't take it (`isoloom-` names are Isoloom's own).
pub(crate) const NAME: &str = "isoloom-router";

/// What a server's platform adds to its router's cloud-config ([`cloud_config`]).
#[derive(Default)]
pub(crate) struct Platform {
    /// Packages beside nftables (the hypervisor's guest agent).
    pub(crate) packages: &'static [&'static str],
    /// The uplink's interface file, written first.
    pub(crate) uplink: Option<Expression>,
    /// The networks' interface files, written last.
    pub(crate) networks: Vec<Expression>,
    /// The commands that bring the interfaces up: once forwarding is on, before the ruleset
    /// loads.
    pub(crate) up: &'static [&'static str],
}

/// The router's cloud-config, the same on every server: nftables with the ruleset
/// ([`ruleset_content`]) and, when the run seals, the sealed one beside it from the first boot
/// ([`sealed_file`]), forwarding on ([`forwarding`]), the site's certificate authorities and the
/// prelude, with what the `platform` adds.
pub(crate) fn cloud_config(env: &Environment, platform: Platform) -> CloudConfig {
    let mut config = CloudConfig::new(NAME, reference("local.users"));
    config.packages = std::iter::once("nftables").chain(platform.packages.iter().copied()).map(String::from).collect();
    config.ca_certs = cloud_init::site_authorities(env);
    config.bootcmd.extend(cloud_init::prelude(env));
    config.files.extend(platform.uplink);
    config.files.extend([
        file("/etc/nftables.conf", None, ruleset_content(env)),
        file("/etc/sysctl.d/90-isoloom.conf", None, string(forwarding(env))),
    ]);
    config.files.extend(sealed_file(env));
    config.files.extend(platform.networks);
    config.runcmd = std::iter::once("sysctl -p /etc/sysctl.d/90-isoloom.conf")
        .chain(platform.up.iter().copied())
        .chain(["systemctl enable --now nftables", "nft -f /etc/nftables.conf"])
        .map(String::from)
        .collect();
    config
}

/// Forwarding on, as a sysctl file: IPv4, and IPv6 when the environment routes IPv6
/// (`isoloom_plan::routing::forwarding`).
pub(crate) fn forwarding(env: &Environment) -> String {
    let lines: Vec<String> = isoloom_plan::routing::forwarding(!env.router6.is_empty())
        .into_iter()
        .map(|(k, v)| format!("{k}={v}\n"))
        .collect();
    lines.concat()
}

/// The `/etc/nftables.conf` content: always the unsealed ruleset (the `reach` rules, the
/// published ports forwarded from the uplink address, and the internet for every network). It
/// never depends on the seal, so a later apply leaves the router's cloud-init snippet untouched
/// (a changed snippet forces the VM to be recreated). The seal is applied live instead, from
/// [`SEALED_NFT`], by the runner's seal-reload step ([`sealed_reload`]).
pub(crate) fn ruleset_content(env: &Environment) -> Expression {
    string(rules(env, false))
}

/// Where the router keeps the sealed ruleset as a second, inert file. The runner's seal step
/// applies it live to the already-booted (unsealed) router: updating the host-side cloud-init
/// snippet alone never reaches a running guest, unlike the clouds' control-plane seal.
pub(crate) const SEALED_NFT: &str = "/etc/nftables.sealed.nft";

/// The sealed ruleset as the router's `SEALED_NFT` file, when the run has something to seal (left
/// out otherwise, so an unsealed environment's router is unchanged). Written from the first boot,
/// so sealing never changes the snippet (and so never recreates the router).
fn sealed_file(env: &Environment) -> Option<Expression> {
    seal::any(env).then(|| file(SEALED_NFT, None, string(rules(env, true))))
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
    let cidrs: Vec<String> = env
        .networks
        .iter()
        .filter(|(n, _)| !(sealed && seal::closed(env, n)))
        .map(|(_, p)| p.cidr.to_string())
        .collect();
    (!cidrs.is_empty()).then(|| format!("{{ {} }}", cidrs.join(", ")))
}

/// The ruleset, sealed or not (see [`ruleset_content`]). The environment's whole set is
/// the DNAT's source exclusion (published ports reach a machine whatever its network); once
/// sealed, traffic to an `internet: false` network from elsewhere is dropped (so it keeps only
/// its `reach` rules), and that network is dropped from the internet forward and masquerade.
fn rules(env: &Environment, sealed: bool) -> String {
    let blocks = format!("{{ {} }}", env.networks.values().map(|n| n.cidr.to_string()).collect::<Vec<_>>().join(", "));
    let mut forward: Vec<String> = router::forward_rules(env.spec());
    let mut dnat = Vec::new();
    for mp in env.machines.values() {
        let Some(addr) = mp.addresses.values().next() else {
            continue;
        };
        for p in &mp.ports {
            let (proto, port, host) = (p.protocol.id(), p.port, p.publish);
            forward.push(format!("ip daddr {addr} {proto} dport {port} ct status dnat accept"));
            dnat.push(format!("    ip saddr != {blocks} {proto} dport {host} dnat ip to {addr}:{port}\n"));
        }
    }
    if sealed {
        let closed: Vec<String> = env
            .networks
            .iter()
            .filter(|(n, _)| seal::closed(env, n))
            .map(|(_, p)| p.cidr.to_string())
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
        "The router's uplink address: the environment's way in.",
        output("ip", reference("local.router_address")),
    );
    m.commented(
        "Each Linux machine's set-up progress (`isoloom-ready/1`: `running: <step>`, `failed: <step>`, `ready`).",
        output("ready_file", string(isoloom_render::steps::READY_FILE)),
    );
    let on_networks = env.machines.iter().filter(|(_, mp)| !mp.addresses.is_empty()).map(|(n, mp)| (n.as_str(), mp));
    let published = crate::module::published(on_networks, |_| reference("local.router_address"));
    if !published.is_empty() {
        m.commented("Published services, from the router's uplink address.", output("published", object(published)));
    }
    let machines: Vec<(String, Expression)> = env
        .machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Vm)
        .filter_map(|(n, _)| first_address(env, n).map(|a| (n.clone(), string(a))))
        .collect();
    m.commented(
        "The machines, through the router: ssh -J isoloom@<ip> isoloom@<machine>.",
        output("machines", object(machines)),
    );
    m.block(output("ssh_user", string(crate::machine::USER)));
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

/// Where a position's checks run (`crate::machine::check_machine`), as the machine's name and
/// address: the controller at its address on the first network.
pub(crate) fn check_host(env: &Environment, p: &Position) -> Option<(String, String)> {
    let machine = crate::machine::check_machine(env, p)?;
    if machine == controller::NAME {
        let first = controller::addresses(env).into_values().next()?;
        return Some((machine.to_owned(), first.to_string()));
    }
    first_address(env, machine).map(|a| (machine.to_owned(), a))
}

/// [`check_host`], as the `checks` output's runner (the `isoloom` user).
fn runner(env: &Environment, p: &Position) -> Option<Runner> {
    check_host(env, p).map(|(machine, host)| Runner {
        machine,
        host: string(host),
        user: string(crate::machine::USER),
    })
}
