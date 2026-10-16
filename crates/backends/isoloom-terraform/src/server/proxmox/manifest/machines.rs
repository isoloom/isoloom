//! The machines a Proxmox run creates (the lab's VMs and LXC containers, the router, the
//! controller), its networks, and what it gives back: the router's uplink address (`ip`, the way
//! in), the SSH user, the ready file, and each published port at the router's address.

use std::collections::BTreeMap;
use std::net::IpAddr;

use isoloom_manifest::{Error as ManifestError, Machine, Name, Network, Output, OutputRole, Part, Placeholder, Role, RunVar, SnakeName, Template};
use isoloom_plan::Environment;
use isoloom_render::router;
use isoloom_render::run_manifest::outputs_at;
use isoloom_spec::validate::Cidr as SpecCidr;

use crate::machine::controller;
use crate::module::manifest;

/// The router, as the manifest names it.
const ROUTER: &str = "isoloom-router";
/// The controller, as the manifest names it.
const CONTROLLER: &str = "isoloom-controller";
/// The router's uplink address: every published port answers there.
const IP: &str = "ip";

/// The lab's networks, each named after the VNet (the node's bridge) it is, or whose VLAN it is:
/// `i<slot>n<i>`, a LAN's `i<slot>l<j>`, as the module names them.
pub(super) fn networks(env: &Environment) -> Result<Vec<Network>, ManifestError> {
    let mut vnets: BTreeMap<&str, String> = BTreeMap::new();
    for (i, (net, _)) in env.networks.iter().enumerate().filter(|(_, (_, p))| p.vlan.is_none()) {
        vnets.insert(net, format!("n{i}"));
    }
    let mut lans: Vec<&str> = env.networks.values().filter_map(|n| n.vlan.as_ref().map(|(l, _)| l.as_str())).collect();
    lans.dedup();
    for (j, lan) in lans.into_iter().filter(|l| !env.networks.contains_key(*l)).enumerate() {
        vnets.insert(lan, format!("l{j}"));
    }
    let native = |net: &str| {
        let on = env.networks.get(net).and_then(|n| n.vlan.as_ref().map(|(l, _)| l.as_str())).unwrap_or(net);
        let id = vnets.get(on).cloned().unwrap_or_default();
        Template::from_parts([Part::Text("i".into()), Part::Placeholder(Placeholder::Run(RunVar::Slot)), Part::Text(id)])
    };
    isoloom_render::run_manifest::networks(env, native)
}

/// The lab's machines in start order, then the router and the controller. A published port is
/// forwarded by the router on its own `publish` number, at the router's address.
pub(super) fn machines(env: &Environment) -> Result<Vec<Machine>, ManifestError> {
    let at = Template::from(Placeholder::Output(SnakeName::parse(IP)?));
    let mut out = Vec::new();
    for name in &env.start_order {
        let (Some(mp), Some(spec)) = (env.machines.get(name), env.spec().machines.get(name)) else {
            continue;
        };
        out.push(manifest::target_machine(name, mp, spec, &at)?);
    }
    let router_at = |net: &str| IpAddr::V4(router::address(env.spec(), net));
    out.push(manifest::machine(ROUTER, Role::Infra, by_network(env, router_at)?)?);
    if controller::needed(env) {
        let controller_at = |net: &str| {
            let c = env.networks.get(net).and_then(|n| SpecCidr::parse(&n.cidr));
            c.map_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), |c| IpAddr::V4(c.controller()))
        };
        out.push(manifest::machine(CONTROLLER, Role::Infra, by_network(env, controller_at)?)?);
    }
    Ok(out)
}

/// `at` on each network of the lab.
fn by_network(env: &Environment, at: impl Fn(&str) -> IpAddr) -> Result<BTreeMap<Name, IpAddr>, ManifestError> {
    env.networks.keys().map(|n| Ok((Name::parse(n)?, at(n)))).collect()
}

/// The router's address (`ip`), the SSH user, the ready file, then each published port at the
/// router's address.
pub(super) fn outputs(machines: &[Machine]) -> Result<Vec<Output>, ManifestError> {
    let state = isoloom_render::run_manifest::terraform_output;
    let mut out = vec![
        state(
            IP,
            OutputRole::Address,
            "The router's uplink address: the published ports answer there, and SSH reaches the machines through it.",
        )?,
        state(
            "ssh_user",
            OutputRole::Info,
            "The user every Linux VM and the router let in with the runner's key.",
        )?,
        state(
            "ready_file",
            OutputRole::Info,
            "Where each Linux machine's set-up says how far it got (isoloom-ready/1).",
        )?,
    ];
    out.extend(outputs_at(machines, &Template::from(Placeholder::Output(SnakeName::parse(IP)?)))?);
    Ok(out)
}
