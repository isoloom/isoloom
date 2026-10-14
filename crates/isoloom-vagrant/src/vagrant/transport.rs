//! The Vagrant transport of a VM's set-up: each planned [`Step`], rendered once by
//! `isoloom_render::steps` (shell on Linux, PowerShell on Windows), becomes a `vm.provision`
//! that runs it as root. What only Vagrant can do is done the Vagrant way:
//!
//! - the project goes in through Isoloom's own `isoloom_project` provisioner (a tar.gz built
//!   in Vagrant's Ruby when the step runs);
//! - the inputs are read from the host's environment when Vagrant runs, written to the file
//!   the steps read (on Windows, given to each step as its environment);
//! - a reboot is Vagrant's own (`reboot: true`, which waits until the machine is back);
//! - routes, trunks and link impairment are re-applied at every boot (`isoloom_render::boot`,
//!   which the egress step uses itself): a reboot step and `vagrant reload` would otherwise drop them;
//! - the default route through a gateway comes once the machine is provisioned: until then it
//!   installs through Vagrant's NAT interface (a gateway may not let it out).

use std::net::Ipv4Addr;

use isoloom_plan::lifecycle::Step;
use isoloom_plan::{Environment, Route};
use isoloom_render::{boot, steps};

use crate::ruby::{Call, Ruby, Value};
use crate::vagrantfile;

/// The operating system family of a VM: which rendering of a step it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// Shell, as root, over SSH.
    Linux,
    /// PowerShell, over WinRM.
    Windows,
}

/// The `vm.provision` lines of lab machine `machine`, from its planned steps.
pub fn provisioners(r: &mut Ruby, env: &Environment, machine: &str, os: Os) {
    let Some(mp) = env.machines.get(machine) else { return };
    let mut default: Option<(Vec<Route>, Ipv4Addr)> = None;
    for step in &mp.lifecycle {
        if matches!(step, Step::Egress | Step::Tc | Step::Ready)
            && let Some((routes, via)) = default.take()
        {
            default_route(r, env, machine, routes, via);
        }
        step_provisioners(r, env, machine, step, os);
        if let Step::Routes { routes, default: Some(via) } = step
            && os == Os::Linux
        {
            default = Some((routes.clone(), *via));
        }
        if matches!(step, Step::Routes { .. }) && mp.is_gateway && os == Os::Linux {
            shell(r, "forwarding", FORWARDING);
        }
    }
}

/// A gateway forwards between its networks; its own provisioning sets the rules. (The plan has
/// no step for it yet.)
const FORWARDING: &str = "echo 'net.ipv4.ip_forward=1' > /etc/sysctl.d/90-isoloom-forwarding.conf && sysctl -q -p /etc/sysctl.d/90-isoloom-forwarding.conf";

/// The provisioners of one step, `machine` being a lab machine or one of Isoloom's own VMs.
pub fn step_provisioners(r: &mut Ruby, env: &Environment, machine: &str, step: &Step, os: Os) {
    match (os, step) {
        (_, Step::Project) => {
            r.call(Call::new("m.vm.provision").arg(Value::str("isoloom_project")).kw("name", Value::str("project")));
        }
        (Os::Linux, Step::Prelude) => vagrantfile::prelude(r, env, "m"),
        (Os::Linux, Step::Inputs { .. }) => inputs(r, env, machine, step),
        (Os::Linux, Step::Reboot) => reboot(r),
        (Os::Linux, Step::Routes { routes, .. }) => {
            let early = Step::Routes {
                routes: routes.clone(),
                default: None,
            };
            persisted(r, env, machine, &early);
        }
        (Os::Linux, Step::Trunks | Step::Tc) => persisted(r, env, machine, step),
        (Os::Linux, Step::WaitFor { machine: dep, ports }) => wait_for(r, env, machine, dep, ports),
        (Os::Windows, Step::Run { path, .. }) => {
            if let Some(script) = steps::windows(env, machine, step) {
                let call = vagrantfile::shell("m", path).kw_opt("env", inputs_env(env, machine));
                r.script(call, "inline", &script);
            }
        }
        _ => {
            if let Some(script) = rendered(env, machine, step, os) {
                shell(r, &label(step), &script);
            }
        }
    }
}

/// The step's script for the machine's OS, if it has one there.
fn rendered(env: &Environment, machine: &str, step: &Step, os: Os) -> Option<String> {
    match os {
        Os::Linux => steps::linux(env, machine, step),
        Os::Windows => steps::windows(env, machine, step),
    }
}

/// The provisioner's name: what `vagrant provision --provision-with` and Vagrant's output show.
fn label(step: &Step) -> String {
    match step {
        Step::Prelude => "constraints".into(),
        Step::DiskGrow => "disk".into(),
        Step::Settings => "settings".into(),
        Step::Hosts { .. } => "hosts".into(),
        Step::Routes { .. } => "routes".into(),
        Step::Trunks => "trunks".into(),
        Step::Volumes { .. } => "volumes".into(),
        Step::SharedVolume { volume, .. } => format!("volume {volume}"),
        Step::Project => "project".into(),
        Step::Inputs { .. } => "inputs".into(),
        Step::WaitFor { machine, .. } => format!("wait for {machine}"),
        Step::Run { path, .. } => path.clone(),
        Step::Reboot => "reboot".into(),
        Step::Egress => "no internet".into(),
        Step::Tc => "tc".into(),
        Step::Ready => "ready".into(),
    }
}

/// `m.vm.provision "shell", name: ..., inline: ...`.
pub fn shell(r: &mut Ruby, name: &str, script: &str) {
    r.script(vagrantfile::shell("m", name), "inline", script);
}

/// The machine's inputs from the host's environment (`INPUTS`), when it reads some.
fn inputs_env(env: &Environment, machine: &str) -> Option<Value> {
    let names = &env.machines.get(machine)?.inputs;
    let list = names.iter().map(|n| Value::str(n).to_string()).collect::<Vec<_>>().join(", ");
    (!names.is_empty()).then(|| Value::raw(format!("INPUTS.slice({list})")))
}

/// The inputs, from the host's environment, written as `KEY='value'` lines (readable by root
/// alone) to the file the steps read; then the step itself (kept on disk when a reboot comes).
fn inputs(r: &mut Ruby, env: &Environment, machine: &str, step: &Step) {
    let Some(mp) = env.machines.get(machine) else { return };
    let file = steps::INPUTS_FILE;
    let mut script = format!(
        "umask 077\n: > {file}\nfor k in {}; do printf \"%s='%s'\\n\" \"$k\" \"$(printenv \"$k\" | sed \"s/'/'\\\\\\\\''/g\")\" >> {file}; done\n",
        mp.inputs.join(" ")
    );
    script.push_str(&steps::linux(env, machine, step).unwrap_or_default());
    let call = vagrantfile::shell("m", "inputs").kw_opt("env", inputs_env(env, machine));
    r.script(call, "inline", &script);
}

/// Vagrant restarts the machine and waits until it answers; the boot is recorded before and
/// checked after, so a machine that didn't restart fails the step.
fn reboot(r: &mut Ruby) {
    shell(r, "before reboot", &steps::record_boot());
    r.call(vagrantfile::shell("m", "reboot").kw("reboot", true));
    shell(r, "after reboot", &steps::reboot_check());
}

/// A step whose effect a reboot forgets, applied now and at every boot.
fn persisted(r: &mut Ruby, env: &Environment, machine: &str, step: &Step) {
    let Some(script) = steps::linux(env, machine, step) else { return };
    let (name, what, on_link_up): (&str, &str, bool) = match step {
        Step::Routes { .. } => ("routes", ROUTES, true),
        Step::Trunks => ("trunks", "802.1Q interfaces of the isoloom trunks", true),
        _ => ("tc", "Link impairment (networks.*.tc) on this machine's interfaces", false),
    };
    let after: &[&str] = if name == "routes" { &["trunks"] } else { &[] };
    shell(r, &label(step), &boot::at_boot(name, what, &script, on_link_up, after));
}

const ROUTES: &str = "Routes to the other isoloom networks";

/// Once provisioned, the routes again with the default route through the gateway.
fn default_route(r: &mut Ruby, env: &Environment, machine: &str, routes: Vec<Route>, via: Ipv4Addr) {
    let step = Step::Routes { routes, default: Some(via) };
    if let Some(script) = steps::linux(env, machine, &step) {
        shell(r, "default route", &boot::at_boot("routes", ROUTES, &script, true, &["trunks"]));
    }
}

/// Waits for a dependency by name, or by address when it is a Windows machine (left out of a
/// Linux machine's /etc/hosts, see the plan's hosts step).
fn wait_for(r: &mut Ruby, env: &Environment, machine: &str, dep: &str, ports: &[u16]) {
    let windows = env
        .spec()
        .machines
        .get(dep)
        .and_then(|m| m.vm.as_ref())
        .is_some_and(|vm| isoloom_spec::images::is_windows(&vm.os));
    let target = if windows {
        env.address_for(machine, dep).to_string()
    } else {
        dep.to_string()
    };
    let step = Step::WaitFor {
        machine: target,
        ports: ports.to_vec(),
    };
    if let Some(script) = steps::linux(env, machine, &step) {
        shell(r, &format!("wait for {dep}"), &script);
    }
}

#[cfg(test)]
mod tests;
