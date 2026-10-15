//! What a target can't run, found while lowering: a machine without the form the target needs
//! (named by the field that rules it out), a VM step a target doesn't render yet, a machine with
//! no existing host on `external`. Each is a `refused` problem; the spec itself stays valid.

use isoloom_spec::model::{Shape, Target};
use isoloom_spec::syntax::{self, Code, FieldPath, NoForm, Problem, Step};

use super::forms::{self, Vm};
use super::overrides;

/// The machines `target` has no form for, in machine order.
pub(super) fn forms(source: &syntax::Spec, target: Target) -> Vec<Problem> {
    if source.cloud.is_some() {
        return Vec::new();
    }
    source.machines.iter().filter_map(|(name, m)| form(source, name, m, target)).collect()
}

/// Everything `target` refuses in `source`, in machine order.
pub(super) fn of(source: &syntax::Spec, target: Target) -> Vec<Problem> {
    if source.cloud.is_some() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (name, m) in &source.machines {
        out.extend(form(source, name, m, target));
        if runs_as_vm(source, name, m, target) {
            out.extend(vm_steps(name, m));
        }
    }
    out
}

/// A refusal at `no`'s field.
fn refused(name: &str, no: &NoForm) -> Problem {
    Problem::new(Code::Refused, no.path(name), no.reason.clone()).fix(no.fix.clone())
}

/// The form `target` needs that the machine lacks, if it lacks it.
fn form(source: &syntax::Spec, name: &str, m: &syntax::Machine, target: Target) -> Option<Problem> {
    let container = forms::container(m);
    let vm = forms::vm(m);
    match target.needs() {
        Shape::Docker => container.err().map(|no| refused(name, &no)),
        Shape::Vm => vm.err().map(|no| refused(name, &no)),
        Shape::VmOrLxc if overrides::lxc(source, name, super::Scope::Target(Target::Proxmox)) => container.err().map(|no| refused(name, &no)),
        Shape::VmOrLxc => vm.err().map(|no| refused(name, &no)),
        Shape::Either => match (container, vm) {
            (Err(_), Err(no)) => Some(refused(name, &no)),
            _ => None,
        },
        Shape::External => (overrides::external(source, name).is_none() && !(m.access && m.os.is_none())).then(|| {
            Problem::new(
                Code::Refused,
                FieldPath::new("overrides.external.machines").key(name),
                format!("the external target reaches machines that exist already, and `{name}` names no host"),
            )
            .fix(format!(
                "`overrides: {{ external: {{ machines: {{ {name}: {{ ssh: {{ address: <host> }} }} }} }} }}`"
            ))
        }),
        Shape::Cloud => None,
    }
}

/// Whether `target` runs machine `name` as a VM.
fn runs_as_vm(source: &syntax::Spec, name: &str, m: &syntax::Machine, target: Target) -> bool {
    let vm = matches!(forms::vm(m), Ok(Vm::Os(_)));
    match target.needs() {
        Shape::Vm | Shape::External => vm,
        Shape::VmOrLxc => vm && !overrides::lxc(source, name, super::Scope::Target(Target::Proxmox)),
        Shape::Either => vm && forms::container(m).is_err(),
        Shape::Docker | Shape::Cloud => false,
    }
}

/// The configuration steps a VM doesn't run yet: the configuration backends that render them as
/// shell and PowerShell come with the resource model (PLAN, Phase 4).
fn vm_steps(name: &str, m: &syntax::Machine) -> Vec<Problem> {
    let Some(app) = m.app() else { return Vec::new() };
    app.steps
        .iter()
        .enumerate()
        .filter(|(_, s)| matches!(s, Step::Copy(_) | Step::Packages(_)))
        .map(|(i, s)| {
            let what = if matches!(s, Step::Copy(_)) { "copy" } else { "packages" };
            let at = FieldPath::new("machines").key(name).key("app").key("steps").index(i);
            Problem::new(
                Code::Refused,
                at,
                format!("a `{what}` step on a VM comes with the configuration backends (Phase 4); containers build it today"),
            )
            .fix("write it as a `run:` script for now, or use a container target")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    /// `img` (an image only), `vm` (steps a container can't run), `both` (both forms, with a
    /// `copy` step), `ct` (an LXC container on Proxmox, an image only), `user` (supplied).
    const LAB: &str = "version: 1
name: t
machines:
  img: { app: { image: nginx } }
  vm: { os: debian@12, app: { steps: [{ run: a.sh }, reboot] } }
  both: { os: debian@12, app: { steps: [{ copy: { from: f, to: /f } }, { run: a.sh }] } }
  ct: { app: { image: alpine } }
  user: { access: true }
overrides:
  proxmox: { machines: { ct: { lxc: true } } }
  external: { machines: { vm: { ssh: { address: 192.0.2.1 } } } }
";

    /// What `target` refuses, as (path, code).
    fn refused_on(target: Target) -> Vec<String> {
        of(testing::lab(LAB).source(), target).iter().map(|p| p.path.to_string()).collect()
    }

    #[test]
    fn a_container_target_refuses_what_has_no_container_form() {
        assert_eq!(refused_on(Target::Docker), ["machines.vm.app.steps[1]"]);
    }

    #[test]
    fn a_vm_target_refuses_what_has_no_vm_form_and_the_vm_steps_it_doesnt_run_yet() {
        assert_eq!(
            refused_on(Target::Vagrant),
            ["machines.img.app.image", "machines.both.app.steps[0]", "machines.ct.app.image"]
        );
    }

    #[test]
    fn proxmox_runs_an_lxc_machine_as_its_container_form() {
        assert_eq!(refused_on(Target::Proxmox), ["machines.img.app.image", "machines.both.app.steps[0]"]);
    }

    #[test]
    fn hybrid_refuses_only_a_machine_with_neither_form_and_runs_a_vm_only_where_it_must() {
        let l = testing::lab(
            "version: 1\nname: t\nmachines:\n  web: { app: { image: nginx } }\n  db: { os: windows-server@2022, app: { steps: [{ packages: [x] }] } }\n  svc: { os: debian@12, service: postgres }\n",
        );
        let found: Vec<String> = of(l.source(), Target::Hybrid).iter().map(|p| p.path.to_string()).collect();
        assert_eq!(found, ["machines.db.app.steps[0]", "machines.svc.service"]);
        assert_eq!(
            refused_on(Target::Hybrid),
            Vec::<String>::new(),
            "both has a container form: no VM steps to refuse"
        );
    }

    #[test]
    fn external_needs_a_host_for_every_machine_but_the_supplied_one() {
        let found = of(testing::lab(LAB).source(), Target::External);
        let paths: Vec<&str> = found.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "overrides.external.machines.img",
                "overrides.external.machines.both",
                "machines.both.app.steps[0]",
                "overrides.external.machines.ct"
            ]
        );
        assert!(found.iter().all(|p| p.code == Code::Refused));
    }

    #[test]
    fn a_cloud_services_lab_refuses_nothing() {
        let l = testing::lab("version: 1\nname: t\ncloud: { provider: aws, terraform: infra }\n");
        assert_eq!(of(l.source(), Target::CloudServices), []);
        assert_eq!(forms(l.source(), Target::Vagrant), []);
    }
}
