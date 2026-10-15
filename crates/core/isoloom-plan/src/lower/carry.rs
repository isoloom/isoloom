//! What today's set-up carries of an app's steps, until the configuration renderers render every
//! step of the model in its place (Phase 4.2). A VM runs its scripts, playbooks and reboots, and
//! places the files of its first `copy` steps through its `Files` step, exactly as a container's
//! `COPY` does when the copy is one file with its `mode`, owned by root, before any other kind of
//! step. A container built from steps renders `run`, `copy` and unpinned `packages`, and needs
//! nothing for `firewall` (a container has no host firewall; `reach` governs). Lowering and the
//! refusals both read these rules, so a step is either carried or refused with its reason, never
//! dropped.

use isoloom_spec::model::MachineFile;
use isoloom_spec::syntax::{self, Copy, FieldPath, Problem, Step};

/// Why the step can't be carried yet, and what to write meanwhile.
const LATER: &str = "is rendered by the configuration renderers, in Phase 4.2";
/// The fix while a step waits for its renderer.
const MEANWHILE: &str = "a `run:` script for now";

/// The files machine `m`'s first `copy` steps place on its VM, in step order.
pub(super) fn copies(m: &syntax::Machine) -> Vec<MachineFile> {
    let windows = m.os.is_some_and(syntax::OsRef::is_windows);
    let Some(app) = m.app() else { return Vec::new() };
    app.steps
        .iter()
        .map_while(|s| match s {
            Step::Copy(c) if placed(c, windows) => Some(MachineFile {
                path: c.to.to_string(),
                source: Some(c.from.to_string()),
                content: None,
                uid: None,
                gid: None,
                mode: c.mode.map(|mode| mode.to_string()),
            }),
            _ => None,
        })
        .collect()
}

/// Whether a VM's `Files` step places this copy as a container's `COPY` would: one file, owned
/// by root, with its mode (a Windows file has neither owner nor mode).
fn placed(c: &Copy, windows: bool) -> bool {
    let root = c.owner.as_ref().is_none_or(|o| o.as_str() == "root");
    !c.is_folder()
        && if windows {
            c.owner.is_none() && c.mode.is_none()
        } else {
            root && c.mode.is_some()
        }
}

/// The steps of machine `name` its VM can't carry yet, as `refused` problems at each step.
pub(super) fn vm_refusals(name: &str, m: &syntax::Machine) -> Vec<Problem> {
    let Some(app) = m.app() else { return Vec::new() };
    let carried = copies(m).len();
    app.steps
        .iter()
        .enumerate()
        .filter(|(i, s)| match s {
            Step::Run { .. } | Step::Reboot | Step::Playbook(_) => false,
            Step::Copy(_) => *i >= carried,
            _ => true,
        })
        .map(|(i, s)| {
            let reason = match s {
                Step::Copy(_) => format!(
                    "a `copy` step on a VM {LATER}; until then a VM places a copy of one file, with its `mode` and owned by root, before any other kind of step"
                ),
                _ => format!("a `{}` step on a VM {LATER}", s.kind()),
            };
            Problem::new(syntax::Code::Refused, step(name, i), reason).fix(MEANWHILE)
        })
        .collect()
}

/// The steps of machine `name` a container built from them can't carry yet, as `refused`
/// problems at each step.
pub(super) fn container_refusals(name: &str, m: &syntax::Machine) -> Vec<Problem> {
    let Some(app) = m.app() else { return Vec::new() };
    let refused = |(i, s): (usize, &Step)| {
        let (reason, fix) = match s {
            Step::Run { .. } | Step::Copy(_) | Step::Firewall(_) => return None,
            Step::Packages(p) if p.iter().all(|p| p.version.is_none()) => return None,
            Step::Packages(_) => (format!("a pinned package in a container {LATER}"), "leave the version out for now".to_owned()),
            Step::Service(_) => (
                format!("a service in a container runs under Isoloom's runner, which {LATER}"),
                "`command:` (the service in the foreground) for now".to_owned(),
            ),
            _ => (format!("a `{}` step in a container {LATER}", s.kind()), MEANWHILE.to_owned()),
        };
        Some(Problem::new(syntax::Code::Refused, step(name, i), reason).fix(fix))
    };
    app.steps.iter().enumerate().filter_map(refused).collect()
}

/// Step `i` of machine `name`.
fn step(name: &str, i: usize) -> FieldPath {
    FieldPath::new("machines").key(name).key("app").key("steps").index(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(yaml: &str) -> syntax::Machine {
        serde_yaml_ng::from_str(yaml).unwrap_or_else(|e| panic!("{yaml}: {e}"))
    }

    fn refused(found: &[Problem]) -> Vec<&str> {
        found.iter().map(|p| p.path.as_str()).collect()
    }

    #[test]
    fn a_vms_first_copies_of_one_file_with_its_mode_are_placed_as_files() {
        let m = machine(
            "{ os: debian@12, app: { steps: [{ copy: { from: conf/a.ini, to: /etc/a.ini, mode: \"0600\" } }, { run: a.sh }, { copy: { from: b, to: /b, mode: \"0644\" } }] } }",
        );
        let placed = copies(&m);
        assert_eq!(placed.len(), 1);
        assert_eq!(
            (placed[0].path.as_str(), placed[0].source.as_deref(), placed[0].mode.as_deref()),
            ("/etc/a.ini", Some("conf/a.ini"), Some("0600"))
        );
        // After a script, a copy keeps its place only with the renderers.
        assert_eq!(refused(&vm_refusals("m", &m)), ["machines.m.app.steps[2]"]);
    }

    #[test]
    fn a_copy_today_s_vm_set_up_cant_place_waits_for_the_renderers() {
        for step in [
            "{ copy: { from: site/, to: /var/www/, mode: \"0755\" } }",
            "{ copy: { from: a, to: /a } }",
            "{ copy: { from: a, to: /a, mode: \"0600\", owner: www-data } }",
        ] {
            let m = machine(&format!("{{ os: debian@12, app: {{ steps: [{step}] }} }}"));
            assert_eq!(copies(&m), [], "{step}");
            let found = vm_refusals("m", &m);
            assert!(found[0].reason.contains("Phase 4.2"), "{step}: {found:?}");
        }
        let windows = machine("{ os: windows-server@2022, app: { steps: [{ copy: { from: a.ini, to: 'C:\\a.ini' } }] } }");
        assert_eq!(copies(&windows).len(), 1);
    }

    #[test]
    fn a_vm_runs_its_scripts_and_waits_for_every_other_kind() {
        let m = machine("{ os: debian@12, app: { steps: [{ run: a.sh }, reboot, { playbook: p.yml }, { packages: [nginx] }, { service: nginx }] } }");
        let found = vm_refusals("m", &m);
        assert_eq!(refused(&found), ["machines.m.app.steps[3]", "machines.m.app.steps[4]"]);
        assert_eq!(
            found[0].reason,
            "a `packages` step on a VM is rendered by the configuration renderers, in Phase 4.2"
        );
    }

    #[test]
    fn a_container_renders_its_scripts_copies_and_packages_and_waits_for_the_rest() {
        let m = machine(
            "{ os: debian@12, app: { steps: [{ run: a.sh }, { copy: { from: site/, to: /s/ } }, { packages: [curl] }, { firewall: { allow: [80] } }, { packages: [{ name: nginx, version: \"1.2\" }] }, { user: { name: a } }, { service: nginx }] } }",
        );
        let found = container_refusals("m", &m);
        assert_eq!(
            refused(&found),
            ["machines.m.app.steps[4]", "machines.m.app.steps[5]", "machines.m.app.steps[6]"]
        );
        assert!(found[2].reason.contains("Isoloom's runner"), "{found:?}");
    }
}
