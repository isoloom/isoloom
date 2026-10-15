//! A machine's `files` in the plan: the VM set-up step that places them, and what they ask of a
//! backend. A container gets them from its runtime (a Compose config, a `ConfigMap` copied into
//! place on Kubernetes), a VM from its [`Step::Files`] step, after the project is unpacked (a
//! file's `source` is read from there).

use std::fmt;

use isoloom_spec::model::{Machine, Spec};

use crate::environment::MachineShape;
use crate::lifecycle::Step;
use crate::requirements::Requirement;

/// The step placing `m`'s files, when it has any.
#[must_use]
pub fn step(m: &Machine) -> Option<Step> {
    (!m.files.is_empty()).then(|| Step::Files { files: m.files.clone() })
}

/// Whether a VM's steps read the project: it runs scripts of its own (its app's or `setup`), or
/// copies a file from it.
/// A target that ships the project only when it is read asks this.
#[must_use]
pub fn reads_project(lifecycle: &[Step]) -> bool {
    lifecycle.iter().any(|s| match s {
        Step::Run { .. } | Step::Setup { .. } => true,
        Step::Files { files } => files.iter().any(|f| f.source.is_some()),
        _ => false,
    })
}

/// What the machines' files ask of the backend, given each machine's shape on the target: a
/// container's project file with its own owner or mode (Compose mounts a project file as it
/// is), a file on an existing host (Isoloom writes nothing there). At most one of each kind,
/// the first machine that asks.
pub fn requirements(spec: &Spec, shape: impl Fn(&str) -> Option<MachineShape>) -> Vec<Requirement> {
    let mut out = Vec::new();
    let owned = spec
        .machines
        .iter()
        .filter(|(n, _)| shape(n) == Some(MachineShape::Container))
        .find_map(|(n, m)| m.files.iter().find(|f| f.source.is_some() && f.sets_owner_or_mode()).map(|f| (n, f)));
    if let Some((machine, f)) = owned {
        out.push(Requirement::OwnedProjectFile {
            machine: machine.clone(),
            path: f.path.clone(),
        });
    }
    let existing = spec
        .machines
        .iter()
        .find(|(n, m)| shape(n) == Some(MachineShape::External) && !m.files.is_empty());
    if let Some((machine, _)) = existing {
        out.push(Requirement::ExistingHostFiles { machine: machine.clone() });
    }
    out
}

/// Why a target that places a container's project file as it is refuses one with an owner or a
/// mode.
pub(crate) fn no_owned_project_file(f: &mut fmt::Formatter<'_>, machine: &str, path: &str) -> fmt::Result {
    write!(
        f,
        "machine `{machine}`'s file `{path}` comes from the project with a `uid`, `gid` or `mode`, and this target places a container's project file as it is: write it as `content` instead"
    )
}

/// Why the `external` target refuses an existing host's files.
pub(crate) fn no_existing_host_files(f: &mut fmt::Formatter<'_>, machine: &str) -> fmt::Result {
    write!(
        f,
        "machine `{machine}` has `files`: Isoloom doesn't write on an existing host, so place them with a `provision:` playbook"
    )
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::reads_project;
    use crate::environment::Environment;
    use crate::lifecycle::Step;
    use crate::requirements::Requirement;
    use crate::testing;

    const SPEC: &str = "
version: 1
name: files
networks:
  lan: { cidr: 10.20.0.0/24 }
machines:
  a:
    networks: { lan: 10 }
    os: debian@12
    app: { image: nginx, steps: [{ run: setup.sh }] }
    files:
      - { path: /etc/a.ini, content: x }
      - { path: /etc/b.ini, source: conf/b.ini, mode: '0600' }
overrides:
  external: { machines: { a: { ssh: { address: 192.0.2.10 } } } }
";

    fn env(target: Target) -> Environment {
        testing::planned(&testing::valid(SPEC), target)
    }

    #[test]
    fn a_vm_reads_the_project_for_its_scripts_and_its_files_from_it_only() {
        let steps = |yaml: &str| {
            let env = testing::environment(&format!("version: 1\nname: t\nmachines:\n  a: {{ os: debian@12, {yaml} }}\n"), Target::Vagrant);
            env.machines.get("a").map(|m| m.lifecycle.clone()).unwrap_or_default()
        };
        assert!(!reads_project(&steps("files: [{ path: /a, content: x }]")));
        assert!(reads_project(&steps("files: [{ path: /a, source: conf/a }]")));
        assert!(reads_project(&steps("app: { steps: [{ run: a.sh }] }")));
        assert!(reads_project(&steps("setup: [s.sh]")));
        assert!(!reads_project(&steps("command: [sleep, '9']")));
    }

    #[test]
    fn a_vm_places_its_files_after_the_project_and_before_its_scripts() {
        let steps = &env(Target::Vagrant).machines["a"].lifecycle;
        let at = |pick: fn(&Step) -> bool| steps.iter().position(pick).expect("planned");
        let files = at(|s| matches!(s, Step::Files { files } if files.len() == 2));
        assert!(at(|s| matches!(s, Step::Project)) < files && files < at(|s| matches!(s, Step::Run { .. })));
    }

    #[test]
    fn an_owned_project_file_asks_a_container_backend_and_an_existing_host_refuses_files() {
        let owned = Requirement::OwnedProjectFile {
            machine: "a".into(),
            path: "/etc/b.ini".into(),
        };
        assert!(env(Target::Docker).requirements.contains(&owned));
        assert!(!env(Target::Vagrant).requirements.contains(&owned));
        let existing = env(Target::External).requirements;
        assert!(existing.contains(&Requirement::ExistingHostFiles { machine: "a".into() }), "{existing:?}");
        assert_eq!(env(Target::Docker).machines["a"].files.len(), 2);
    }
}
