//! What a Windows VM can't be, written once for every target that makes one (Vagrant, Proxmox,
//! ESXi, the clouds): each a refusal reason after "machine `<name>`: ".

use isoloom_plan::Environment;
use isoloom_spec::syntax::has_extension;

/// The longest Windows computer name: NetBIOS keeps 15 characters, so two longer names could
/// end up one.
pub(crate) const COMPUTER_NAME_MAX: usize = 15;

/// Why Windows VM `name` of `env` can't be made on any target that makes it: a name over 15
/// characters (it is the machine's computer name), or a provisioning step
/// that isn't a PowerShell script (`.ps1`), the one kind a Windows machine runs (the spec's
/// validation says so first).
#[must_use]
pub fn guest_refusal(env: &Environment, name: &str) -> Option<String> {
    if name.chars().count() > COMPUTER_NAME_MAX {
        return Some(format!("machine `{name}`: Windows computer names are at most {COMPUTER_NAME_MAX} characters"));
    }
    let provision = &env.spec().machines.get(name)?.vm.as_ref()?.provision;
    let step = provision.iter().find(|s| !has_extension(s, "ps1"))?;
    Some(format!(
        "machine `{name}`: Windows provisioning steps are PowerShell scripts (.ps1); `{step}` isn't (Ansible playbooks run from the controller: `provision:`)"
    ))
}

/// [`guest_refusal`], then why Windows VM `name` can't be made where it sets its own network up at
/// its first boot (Vagrant, cloudbase-init on Proxmox and ESXi), with no cloud network around it:
/// a gateway machine's routes, an `internet: false` network, volumes.
#[must_use]
pub fn set_up_at_boot_refusal(env: &Environment, name: &str) -> Option<String> {
    guest_refusal(env, name).or_else(|| {
        let mp = env.machines.get(name)?;
        let why = if mp.default_gateway.is_some() || mp.is_gateway {
            "this target sets no gateway machine's routes on Windows"
        } else if mp.addresses.keys().any(|n| env.networks.get(n).is_some_and(|p| !p.internet)) {
            "this target cuts no Windows machine off the internet (`internet: false`)"
        } else if !mp.volumes.is_empty() {
            "this target mounts no volume on Windows"
        } else {
            return None;
        };
        Some(format!("machine `{name}`: {why}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use isoloom_spec::model::Target;

    fn env(machines: &str) -> Environment {
        crate::testing::env(
            &format!("version: 1\nname: w\nnetworks:\n  lan: {{ cidr: 10.9.0.0/24 }}\n  dark: {{ cidr: 10.8.0.0/24, internet: false }}\nmachines:\n{machines}"),
            Target::Proxmox,
        )
    }

    #[test]
    fn a_windows_name_over_15_characters_is_refused_not_cut() {
        let e = env(
            "  windows-server-01: { networks: { lan: 10 }, os: windows-server@2022 }\n  win-server-0001: { networks: { lan: 11 }, os: windows-server@2022 }\n",
        );
        assert_eq!(
            guest_refusal(&e, "windows-server-01").as_deref(),
            Some("machine `windows-server-01`: Windows computer names are at most 15 characters")
        );
        assert_eq!(guest_refusal(&e, "win-server-0001"), None);
    }

    #[test]
    fn a_windows_vm_setting_itself_up_has_no_routes_seal_or_volumes() {
        let e = env("  win: { networks: { dark: 10 }, os: windows-server@2022 }\n");
        assert_eq!(guest_refusal(&e, "win"), None);
        let why = set_up_at_boot_refusal(&e, "win").unwrap();
        assert!(why.contains("`internet: false`"), "{why}");
        assert!(!why.contains("later") && !why.contains("next"), "{why}");
    }
}
