//! A machine's `files` in the plan: the VM set-up step that places them, and what they ask of a
//! backend. A container gets them from its runtime (a Compose config, a ConfigMap copied into
//! place on Kubernetes), a VM from its [`Step::Files`] step, after the project is unpacked (a
//! file's `source` is read from there).

use isoloom_spec::model::{Machine, Spec};

use crate::environment::MachineShape;
use crate::lifecycle::Step;
use crate::requirements::Requirement;

/// The step placing `m`'s files, when it has any.
pub fn step(m: &Machine) -> Option<Step> {
    (!m.files.is_empty()).then(|| Step::Files { files: m.files.clone() })
}

/// Whether a VM's steps read the project: it runs scripts of its own (or `before` ones), or
/// copies a file from it.
/// A target that ships the project only when it is read asks this.
pub fn reads_project(lifecycle: &[Step]) -> bool {
    lifecycle.iter().any(|s| match s {
        Step::Run { .. } | Step::Before { .. } => true,
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

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use crate::environment::Environment;
    use crate::lifecycle::Step;
    use crate::requirements::Requirement;

    const SPEC: &str = "
version: 1
name: files
networks:
  lan: { cidr: 10.20.0.0/24 }
machines:
  a:
    networks: { lan: 10 }
    docker: { image: nginx }
    vm: { os: debian-12, provision: [setup.sh] }
    external: { address: 192.0.2.10 }
    files:
      - { path: /etc/a.ini, content: x }
      - { path: /etc/b.ini, source: conf/b.ini, mode: '0600' }
";

    fn env(target: Target) -> Environment {
        let spec = isoloom_spec::parse(SPEC).expect("parses");
        assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
        Environment::build(&spec, target, None)
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
