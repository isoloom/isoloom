//! What the `vagrant` target can't render besides the plan's requirements: a machine with no
//! Vagrant box, a provisioning step Vagrant has no way to run, and what a Windows machine can't
//! have (`isoloom_render::windows`). Found from the plan, without rendering.

use crate::boxes;
use isoloom_plan::{Environment, Vm};
use isoloom_spec::model::is_reboot;
use isoloom_spec::syntax::{has_extension, is_yaml_file};

/// Why Vagrant can't render `env`, if it can't: the first VM it can't build, in spec order.
pub(crate) fn refusal(env: &Environment) -> Option<String> {
    env.machines
        .iter()
        .filter_map(|(name, mp)| Vm::of(name, mp))
        .find_map(|vm| machine(env, vm).or_else(|| super::bridged::refusal(env, vm.name, vm.plan)))
}

fn machine(env: &Environment, vm: Vm) -> Option<String> {
    let name = vm.name;
    if boxes::vagrant(vm).is_none() {
        return Some(format!(
            "machine `{name}`: no Vagrant box for `{}`; give one with `overrides.vagrant.machines.{name}.box`",
            vm.os
        ));
    }
    if vm.plan.is_windows() {
        return windows(env, name);
    }
    // The steps as the spec writes them: the plan keeps only those it can run.
    let provision = &env.spec().machines.get(name)?.vm.as_ref()?.provision;
    provision
        .iter()
        .find(|s| !(has_extension(s, "sh") || is_yaml_file(s) || is_reboot(s)))
        .map(|step| format!("machine `{name}`: provisioning step `{step}` isn't a .sh, .yml or .yaml file (or `reboot`)"))
}

/// What a Windows machine can't have on Vagrant: what no Windows VM set up at its own boot has.
fn windows(env: &Environment, name: &str) -> Option<String> {
    isoloom_render::windows::set_up_at_boot_refusal(env, name)
}
