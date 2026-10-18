//! What the set-up of each form carries of a recipe's steps (an app's, or a catalogue entry's),
//! read by lowering and the refusals alike, so a step is either carried or refused with its
//! reason, never dropped:
//!
//! - **a container built from a recipe** carries every step with a container form: its scripts,
//!   copies and templates as the Dockerfile's own instructions, every other step as the shell
//!   library's, and its services under Isoloom's runner;
//! - **a new VM** carries every step, through the shell library (the PowerShell library on
//!   Windows);
//! - **a host that already exists** (the `external` target) runs scripts, playbooks and reboots
//!   from the project, and every other step with the Ansible CM tool (Phase 4.5: a generated role
//!   per machine, run over the inventory). A Windows host that already exists still waits for
//!   Ansible's Windows modules, so its non-script steps are refused until that leg lands.

use isoloom_spec::syntax::{self, FieldPath, Problem, Step};

/// The VM a recipe runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Host {
    /// A VM the target creates: its set-up renders every step.
    New,
    /// A host that already exists (the `external` target).
    Existing,
}

/// Why a container can't carry step `s`, and what to write instead.
pub(super) fn in_container(s: &Step) -> Option<(String, String)> {
    s.container_refusal().map(|why| (why.to_owned(), "a VM target".to_owned()))
}

/// Why a VM on `host` can't carry step `s` yet, and what to write meanwhile. The Ansible CM tool
/// renders every step on a Linux host that already exists (the refusal is lifted); a Windows one
/// (`windows`) still runs only its scripts, reboots and playbooks until the Ansible Windows leg.
pub(super) fn on_vm(s: &Step, host: Host, windows: bool) -> Option<(String, String)> {
    if host == Host::New || !windows || matches!(s, Step::Run { .. } | Step::Reboot | Step::Playbook(_)) {
        return None;
    }
    Some((
        format!(
            "a `{}` step on a Windows host that already exists waits for Ansible's Windows modules",
            s.kind()
        ),
        "a `run:` PowerShell script for now, or a VM target".to_owned(),
    ))
}

/// The steps of machine `name` its VM on `host` (its OS `windows` or not) can't carry yet, as
/// `refused` problems at each step.
pub(super) fn vm_refusals(name: &str, m: &syntax::Machine, host: Host, windows: bool) -> Vec<Problem> {
    let Some(app) = m.app() else { return Vec::new() };
    app.steps
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let (reason, fix) = on_vm(s, host, windows)?;
            let at = FieldPath::new("machines").key(name).key("app").key("steps").index(i);
            Some(Problem::new(syntax::Code::Refused, at, reason).fix(fix))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(yaml: &str) -> syntax::Machine {
        serde_yaml_ng::from_str(yaml).unwrap_or_else(|e| panic!("{yaml}: {e}"))
    }

    #[test]
    fn a_new_vm_carries_every_step() {
        let m = machine("{ os: debian@12, app: { steps: [{ run: a.sh }, { packages: [nginx] }, { copy: { from: site/, to: /s/ } }, { service: nginx }] } }");
        assert_eq!(vm_refusals("m", &m, Host::New, false), []);
    }

    #[test]
    fn a_linux_host_that_exists_runs_every_step_through_ansible() {
        let m = machine("{ os: debian@12, app: { steps: [{ run: a.sh }, reboot, { playbook: p.yml }, { packages: [nginx] }, { service: nginx }] } }");
        assert_eq!(
            vm_refusals("m", &m, Host::Existing, false),
            [],
            "the Ansible CM tool carries every step on a Linux existing host"
        );
    }

    #[test]
    fn a_windows_host_that_exists_runs_its_scripts_and_waits_for_every_other_kind() {
        let m = machine("{ os: windows-server@2022, app: { steps: [{ run: a.ps1 }, reboot, { packages: [Web-Server] }, { user: { name: a } }] } }");
        let found = vm_refusals("m", &m, Host::Existing, true);
        let paths: Vec<&str> = found.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, ["machines.m.app.steps[2]", "machines.m.app.steps[3]"]);
        assert_eq!(
            found[0].reason,
            "a `packages` step on a Windows host that already exists waits for Ansible's Windows modules"
        );
    }

    #[test]
    fn a_container_carries_every_step_with_a_container_form() {
        let steps: Vec<Step> = serde_yaml_ng::from_str(
            "[{ run: a.sh }, { packages: [{ name: nginx, version: \"1.2\" }] }, { user: { name: a } }, { service: nginx }, { firewall: { allow: [80] } }]",
        )
        .unwrap();
        assert!(steps.iter().all(|s| in_container(s).is_none()));
        assert!(in_container(&Step::Reboot).is_some());
    }
}
