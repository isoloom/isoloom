//! What no cloud does yet for a Windows machine.

use isoloom_spec::model::{Machine, Spec};

/// What no cloud does yet for a Windows machine: several networks, steps other than `.ps1`.
pub(crate) fn refusal(name: &str, m: &Machine) -> Option<String> {
    let vm = m.vm.as_ref().filter(|v| isoloom_spec::images::is_windows(&v.os))?;
    if m.networks.len() > 1 {
        return Some(format!("machine `{name}`: Windows machines on several networks in the cloud come later"));
    }
    vm.provision
        .iter()
        .any(|p| !isoloom_plan::lifecycle::has_extension(p, "ps1"))
        .then(|| format!("machine `{name}`: Windows steps are .ps1 scripts"))
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
