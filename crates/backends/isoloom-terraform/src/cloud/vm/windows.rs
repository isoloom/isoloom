//! What no cloud does for a Windows machine.

use isoloom_plan::Environment;
use isoloom_spec::model::{Machine, Spec};

/// What no cloud does for a Windows machine: what no Windows VM has
/// (`isoloom_render::windows`), and several networks.
pub(crate) fn refusal(env: &Environment, name: &str, m: &Machine) -> Option<String> {
    m.vm.as_ref().filter(|v| isoloom_spec::images::is_windows(&v.os))?;
    isoloom_render::windows::guest_refusal(env, name)
        .or_else(|| (m.networks.len() > 1).then(|| format!("machine `{name}`: a Windows machine in the cloud has one network")))
}

/// A Windows machine's UDP service published on another port: Windows' port proxy (`netsh
/// interface portproxy`) forwards TCP only.
pub(crate) fn udp_redirect(spec: &Spec) -> Option<String> {
    spec.machines.iter().find_map(|(name, m)| {
        let windows = m.vm.as_ref().is_some_and(|v| isoloom_spec::images::is_windows(&v.os));
        let s = m.ports.iter().find(|s| !s.protocol.is_tcp() && s.publish.is_some_and(|h| h != s.port))?;
        windows.then(|| {
            format!(
                "machine `{name}`: UDP port {} published as {} on a Windows VM in the cloud (its port proxy is TCP only): publish it on its own port",
                s.port,
                s.publish.unwrap_or_default()
            )
        })
    })
}
