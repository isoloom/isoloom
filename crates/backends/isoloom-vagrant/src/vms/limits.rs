//! What the `vagrant` target can't render besides the plan's requirements: a machine with no
//! Vagrant box, a provisioning step Vagrant has no way to run, and what a Windows machine can't
//! have yet (the set-up steps it has no PowerShell for). Found from the plan, without rendering.

use isoloom_plan::lifecycle::has_extension;
use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::images;
use isoloom_spec::model::{Machine, is_reboot};

/// Why Vagrant can't render `env`, if it can't: the first machine it can't build.
pub(crate) fn refusal(env: &Environment) -> Option<String> {
    let spec = env.spec();
    env.machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Vm)
        .find_map(|(name, mp)| machine(env, name, spec.machines.get(name)?).or_else(|| super::bridged::refusal(env, name, mp)))
}

fn machine(env: &Environment, name: &str, m: &Machine) -> Option<String> {
    let vm = m.vm.as_ref()?;
    if images::vagrant(vm).is_none() {
        return Some(format!(
            "machine `{name}`: no Vagrant box for `{}` yet; give one with `vm.image.vagrant`",
            vm.os
        ));
    }
    if images::is_windows(&vm.os) {
        return windows(env, name, m);
    }
    vm.provision
        .iter()
        .find(|s| !(["sh", "yml", "yaml"].iter().any(|e| has_extension(s, e)) || is_reboot(s)))
        .map(|step| format!("machine `{name}`: provisioning step `{step}` isn't a .sh, .yml or .yaml file (or `reboot`)"))
}

/// What a Windows machine can't have on Vagrant yet.
fn windows(env: &Environment, name: &str, m: &Machine) -> Option<String> {
    let mp = env.machines.get(name)?;
    if name.len() > 15 {
        return Some(format!("machine `{name}`: Windows computer names are at most 15 characters"));
    }
    if let Some(step) = m.vm.as_ref()?.provision.iter().find(|s| !has_extension(s, "ps1")) {
        return Some(format!(
            "machine `{name}`: Windows provisioning steps are PowerShell scripts (.ps1); `{step}` isn't (Ansible runs from outside Windows: environment-level provisioning comes next)"
        ));
    }
    if mp.default_gateway.is_some() || mp.is_gateway {
        return Some(format!("machine `{name}`: a gateway machine's routes on Windows come later"));
    }
    if mp.addresses.keys().any(|n| env.networks.get(n).is_some_and(|p| !p.internet)) {
        return Some(format!("machine `{name}`: `internet: false` on Windows comes later"));
    }
    if !mp.volumes.is_empty() {
        return Some(format!("machine `{name}`: volumes on Windows come later"));
    }
    None
}
