//! What the `vagrant` target can't render besides the plan's requirements: a machine with no
//! Vagrant box, a provisioning step Vagrant has no way to run, and what a Windows machine can't
//! have (`isoloom_render::windows`). Found from the plan, without rendering.

use crate::boxes;
use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::images;
use isoloom_spec::model::{Machine, is_reboot};
use isoloom_spec::syntax::{has_extension, is_yaml_file};

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
    if boxes::vagrant(vm).is_none() {
        return Some(format!(
            "machine `{name}`: no Vagrant box for `{}`; give one with `overrides.vagrant.machines.{name}.box`",
            vm.os
        ));
    }
    if images::is_windows(&vm.os) {
        return windows(env, name);
    }
    vm.provision
        .iter()
        .find(|s| !(has_extension(s, "sh") || is_yaml_file(s) || is_reboot(s)))
        .map(|step| format!("machine `{name}`: provisioning step `{step}` isn't a .sh, .yml or .yaml file (or `reboot`)"))
}

/// What a Windows machine can't have on Vagrant: what no Windows VM set up at its own boot has.
fn windows(env: &Environment, name: &str) -> Option<String> {
    isoloom_render::windows::set_up_at_boot_refusal(env, name)
}
