//! The machines a server's run creates (the lab's machines, the router, the controller), and what
//! it gives back: the router's uplink address (`ip`, the way in), the SSH user, the ready file,
//! and each published port at the router's address.

use std::collections::BTreeMap;
use std::net::IpAddr;

use isoloom_manifest::{Error as ManifestError, Machine, Name, Output, OutputRole, Placeholder, Role, SnakeName, Template};
use isoloom_plan::Environment;
use isoloom_render::router;
use isoloom_render::run_manifest::outputs_with;

use crate::machine::controller;
use crate::module::manifest;

/// The controller, as the manifest names it.
use crate::machine::controller::NAME as CONTROLLER;
use crate::server::router::NAME as ROUTER;
/// The router's uplink address: every published port answers there.
const IP: &str = "ip";

/// The lab's machines in start order, then the router and the controller. A published port is
/// forwarded by the router on its own `publish` number, at the router's address.
pub(super) fn machines(env: &Environment) -> Result<Vec<Machine>, ManifestError> {
    let at = Template::from(Placeholder::Output(SnakeName::parse(IP)?));
    let mut out = Vec::new();
    for name in &env.start_order {
        let (Some(mp), Some(spec)) = (env.machines.get(name), env.spec().machines.get(name)) else {
            continue;
        };
        out.push(manifest::environment_machine(name, mp, spec, &at)?);
    }
    let router_at = |net: &str| IpAddr::V4(router::address(env.spec(), net));
    out.push(manifest::machine(ROUTER, Role::Infra, by_network(env, router_at)?)?);
    if controller::needed(env) {
        let controller_at = |net: &str| IpAddr::V4(env.block(net).controller());
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
    let state = crate::module::manifest::terraform_output;
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
    let router = Template::from(Placeholder::Output(SnakeName::parse(IP)?));
    out.extend(outputs_with(machines, |_| router.clone())?);
    Ok(out)
}
