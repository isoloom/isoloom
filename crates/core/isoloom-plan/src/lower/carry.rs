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
//!   from the project, and the other steps with the Ansible CM tool (Phase 4.5): until then those
//!   are refused there.

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

/// Why a VM on `host` can't carry step `s` yet, and what to write meanwhile.
pub(super) fn on_vm(s: &Step, host: Host) -> Option<(String, String)> {
    if host == Host::New || matches!(s, Step::Run { .. } | Step::Reboot | Step::Playbook(_)) {
        return None;
    }
    Some((
        format!("a `{}` step on a host that already exists runs with the Ansible CM tool (Phase 4.5)", s.kind()),
        "a `run:` script for now, or a VM target".to_owned(),
    ))
}

/// The steps of machine `name` its VM on `host` can't carry yet, as `refused` problems at each
/// step.
pub(super) fn vm_refusals(name: &str, m: &syntax::Machine, host: Host) -> Vec<Problem> {
    let Some(app) = m.app() else { return Vec::new() };
    app.steps
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let (reason, fix) = on_vm(s, host)?;
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
        assert_eq!(vm_refusals("m", &m, Host::New), []);
    }

    #[test]
    fn an_existing_host_runs_its_scripts_and_waits_for_every_other_kind() {
        let m = machine("{ os: debian@12, app: { steps: [{ run: a.sh }, reboot, { playbook: p.yml }, { packages: [nginx] }, { service: nginx }] } }");
        let found = vm_refusals("m", &m, Host::Existing);
        let paths: Vec<&str> = found.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, ["machines.m.app.steps[3]", "machines.m.app.steps[4]"]);
        assert_eq!(
            found[0].reason,
            "a `packages` step on a host that already exists runs with the Ansible CM tool (Phase 4.5)"
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
