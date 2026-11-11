//! The access machine a spec leaves to the runner (`access: true`, no `os`, nothing to run): its
//! forms come from the runner's image table (`access:`). A target the table gives none of the
//! form it runs can't produce it, so it refuses the environment rather than leave out a machine the others
//! still name (in their hosts files, as their checks' seat).

use isoloom_spec::model::{Shape, Target};
use isoloom_spec::syntax::{self, Code, FieldPath, Form, Problem, Warning};

use super::Lab;

/// Where the image table comes from, for a fix to name it.
const TABLE: &str = "`isoloom generate --images`, or the defaults' `images:`";

impl Lab {
    /// The machine the spec leaves to the runner, when it has one: an access machine with no
    /// `os` and nothing to run.
    fn supplied(&self) -> Option<&str> {
        self.source
            .machines
            .iter()
            .find(|(_, m)| matches!(syntax::vm_form(m), Ok(Form::Supplied)))
            .map(|(name, _)| name.as_str())
    }

    /// Whether the image table gives the access machine a container (`access.docker`) and a VM
    /// (`access.vm`).
    fn given(&self) -> (bool, bool) {
        let access = self.images.access.as_ref();
        (access.is_some_and(|a| a.docker.is_some()), access.is_some_and(|a| a.vm.is_some()))
    }

    /// Why `target` can't produce the access machine the spec leaves to the runner: the image
    /// table gives it none of the form the target runs, as a `refused` problem at its `os`.
    pub(super) fn unsupplied(&self, target: Target) -> Option<Problem> {
        let (container, vm) = self.given();
        let (has, reason, fix) = match target.needs() {
            Shape::Docker => (
                container,
                "without `os`, the access machine's container comes from the image table's `access.docker`, which gives none",
                "access: { docker: <image> }",
            ),
            Shape::Vm | Shape::VmOrLxc => (
                vm,
                "without `os`, the access machine's VM comes from the image table's `access.vm`, which gives none",
                "access: { vm: debian@12 }",
            ),
            Shape::Either => (
                container || vm,
                "without `os`, the access machine comes from the image table's `access:`, which gives neither a container nor a VM",
                "access: { vm: debian@12 }",
            ),
            Shape::External | Shape::Cloud => return None,
        };
        if has {
            return None;
        }
        let name = self.supplied()?;
        let at = FieldPath::new("machines").key(name).key("os");
        Some(Problem::new(Code::Refused, at, reason).fix(format!("`os: debian@12`, or the image table's `{fix}` ({TABLE})")))
    }

    /// A warning when the image table gives the access machine the spec leaves to the runner no
    /// form at all: every target that creates the machines (all but `external` and
    /// `cloud-services`) refuses it.
    pub(super) fn unsupplied_warning(&self) -> Option<Warning> {
        if self.given() != (false, false) {
            return None;
        }
        let name = self.supplied()?;
        let mut w = Warning::new(
            FieldPath::new("machines").key(name).key("access"),
            "without `os`, the access machine comes from the image table's `access:`, and none is given here: every target that creates the machines (all but `external` and `cloud-services`) refuses it unless `isoloom generate --images` gives one",
        )
        .fix("`os: debian@12`");
        w.span = self.source.spans.find(&w.path);
        Some(w)
    }
}

#[cfg(test)]
mod tests {
    use isoloom_spec::images::Table;
    use isoloom_spec::model::Target;

    use crate::Lab;

    /// `user` is left to the runner; `web` is an ordinary VM.
    const LAB: &str = "version: 1\nname: t\nnetworks: { lab: { cidr: 10.61.0.0/24 } }\nmachines:\n  web: { os: debian@12, networks: { lab: 10 } }\n  user: { access: true, networks: { lab: 99 } }\n";

    fn lab(table: &str) -> Lab {
        Lab::parse(LAB).unwrap().with_images(Table::parse(table).unwrap())
    }

    fn refused(lab: &Lab, t: Target) -> Option<String> {
        lab.refusal(t).map(|p| format!("{}: {}", p.path, p.reason))
    }

    #[test]
    fn a_target_the_image_table_gives_no_form_for_the_supplied_access_machine_refuses_it() {
        let none = lab("{}");
        for t in [Target::Docker, Target::Vagrant, Target::Proxmox, Target::Esxi, Target::CloudVm, Target::Hybrid] {
            let why = refused(&none, t).unwrap_or_default();
            assert!(why.starts_with("machines.user.os: without `os`, the access machine"), "{t:?}: {why}");
            assert!(!none.possible().contains(&t), "{t:?}");
        }
        for t in [Target::External, Target::CloudServices] {
            assert_eq!(refused(&none, t).filter(|w| w.contains("machines.user")), None, "{t:?}");
        }
        let warned = none.warnings();
        assert_eq!(warned.iter().map(|w| w.path.to_string()).collect::<Vec<_>>(), ["machines.user.access"]);
        assert!(warned[0].reason.contains("all but `external` and `cloud-services`"), "{}", warned[0].reason);

        let vm = lab("access: { vm: debian@12 }");
        assert_eq!(refused(&vm, Target::Vagrant), None);
        assert_eq!(refused(&vm, Target::Hybrid), None);
        assert!(refused(&vm, Target::Docker).is_some_and(|w| w.contains("`access.docker`")));
        assert_eq!(vm.warnings().len(), 0, "{:?}", vm.warnings());

        let container = lab("access: { docker: registry.example.com/workstation:1 }");
        assert_eq!(refused(&container, Target::Docker), None);
        assert!(refused(&container, Target::Proxmox).is_some_and(|w| w.contains("`access.vm`")));
    }

    #[test]
    fn an_access_machine_with_its_own_os_needs_no_table() {
        let own = Lab::parse(&LAB.replace("access: true,", "access: true, os: debian@12,")).unwrap();
        assert_eq!(refused(&own, Target::Vagrant), None);
        assert_eq!(own.warnings().len(), 0, "{:?}", own.warnings());
    }
}
