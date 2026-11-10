//! What no cloud does for a Windows machine.

use isoloom_plan::{Environment, Vm};

/// What no cloud does for Windows VM `vm`: what no Windows VM has (`isoloom_render::windows`),
/// several networks, and secrets: its set-up runs over WinRM during Terraform's apply, while the
/// runner delivers a machine's secrets once `up`'s steps are done, so the set-up would wait for
/// them in vain.
pub(crate) fn refusal(env: &Environment, vm: Vm) -> Option<String> {
    let name = vm.name;
    if !vm.plan.is_windows() {
        return None;
    }
    isoloom_render::windows::guest_refusal(env, name)
        .or_else(|| (vm.plan.addresses.len() > 1).then(|| format!("machine `{name}`: a Windows machine in the cloud has one network")))
        .or_else(|| {
            (!vm.plan.secrets.is_empty()).then(|| {
                format!("machine `{name}` gets `secrets`: a Windows machine in the cloud is set up during Terraform's apply, before the runner delivers them")
            })
        })
}

/// A Windows VM's UDP port published on another port: Windows' port proxy (`netsh interface
/// portproxy`) forwards TCP only.
pub(crate) fn udp_redirect(env: &Environment) -> Option<String> {
    env.vms().filter(|vm| vm.plan.is_windows()).find_map(|vm| {
        let p = vm.plan.ports.iter().find(|p| !p.protocol.is_tcp() && p.publish != p.port)?;
        Some(format!(
            "machine `{}`: UDP port {} published as {} on a Windows VM in the cloud (its port proxy is TCP only): publish it on its own port",
            vm.name, p.port, p.publish
        ))
    })
}
