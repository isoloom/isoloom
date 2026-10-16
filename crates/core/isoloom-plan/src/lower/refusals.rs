//! What a target can't run, found while lowering: a machine without the form the target needs
//! (named by the field that rules it out), a step its family can't carry at all (a repository on
//! Windows), a step an existing host doesn't run yet ([`super::carry`]), a machine with no
//! existing host on `external`. Each is a `refused` problem; the spec itself stays valid.

use isoloom_spec::model::{Shape, Target};
use isoloom_spec::syntax::{self, Code, FieldPath, Problem};

use super::carry;
use super::carry::Host;
use super::forms::{self, Container, Service, Vm};
use super::overrides;
use super::service::Services;
use crate::configure::{self, DaemonTable};

/// The machines `target` has no form for, in machine order (`services`: the lab's catalogue
/// services, resolved).
pub(super) fn forms(source: &syntax::Spec, services: &Services, target: Target) -> Vec<Problem> {
    if source.cloud.is_some() {
        return Vec::new();
    }
    source
        .machines
        .iter()
        .filter_map(|(name, m)| form(source, name, m, services.get(name), target))
        .collect()
}

/// Everything `target` refuses in `source`, in machine order.
pub(super) fn of(source: &syntax::Spec, services: &Services, target: Target) -> Vec<Problem> {
    if source.cloud.is_some() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (name, m) in &source.machines {
        let service = services.get(name);
        out.extend(form(source, name, m, service, target));
        if runs_as_vm(source, name, m, service, target) {
            let host = if target.needs() == Shape::External { Host::Existing } else { Host::New };
            out.extend(vm_steps(name, m, host));
            out.extend(service_steps(name, service, Carrier::Vm(host)));
        } else if let Some(carrier) = built(source, name, m, service, target) {
            out.extend(service_steps(name, service, carrier));
        }
    }
    out
}

/// What renders a service machine's recipe on the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Carrier {
    /// The VM's set-up: every phase.
    Vm(Host),
    /// A build from the OS's image: every phase.
    Built,
    /// A thin build over the entry's official image: `configure`.
    Image,
}

/// The first step of a service machine's recipe that what renders it can't carry yet, as a
/// `refused` problem at its `service`.
fn service_steps(name: &str, service: Service<'_>, carrier: Carrier) -> Option<Problem> {
    let Some(Ok(resolved)) = service else { return None };
    resolved.iter().find_map(|r| {
        // Over the official image, `configure` alone: the image is `install`, and its own
        // entrypoint prepares each start.
        let (install, start) = if carrier == Carrier::Image {
            (&[][..], &[][..])
        } else {
            (&r.install[..], &r.start[..])
        };
        let mut steps = install.iter().chain(&r.configure).chain(start);
        let (reason, fix) = steps.find_map(|s| match carrier {
            Carrier::Vm(host) => carry::on_vm(s, host),
            Carrier::Built | Carrier::Image => carry::in_container(s),
        })?;
        let at = FieldPath::new("machines").key(name).key("service");
        Some(Problem::new(Code::Refused, at, format!("{}'s recipe: {reason}", r.entry)).fix(fix))
    })
}

/// The form `target` needs that the machine lacks, if it lacks it.
fn form(source: &syntax::Spec, name: &str, m: &syntax::Machine, service: Service<'_>, target: Target) -> Option<Problem> {
    let container = forms::container(m, service);
    let vm = forms::vm(m, service);
    match target.needs() {
        Shape::Docker => container.err().map(|no| no.refusal(name)),
        Shape::Vm => vm.err().map(|no| no.refusal(name)),
        Shape::VmOrLxc if overrides::lxc(source, name, super::Scope::Target(Target::Proxmox)) => container.err().map(|no| no.refusal(name)),
        Shape::VmOrLxc => vm.err().map(|no| no.refusal(name)),
        Shape::Either => match (container, vm) {
            (Err(_), Err(no)) => Some(no.refusal(name)),
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
fn runs_as_vm(source: &syntax::Spec, name: &str, m: &syntax::Machine, service: Service<'_>, target: Target) -> bool {
    let vm = matches!(forms::vm(m, service), Ok(Vm::Os(_)));
    match target.needs() {
        Shape::Vm | Shape::External => vm,
        Shape::VmOrLxc => vm && !overrides::lxc(source, name, super::Scope::Target(Target::Proxmox)),
        Shape::Either => vm && forms::container(m, service).is_err(),
        Shape::Docker | Shape::Cloud => false,
    }
}

/// How `target` builds machine `name`'s container from a recipe, when it does: from the OS's
/// image (its steps, or its entry's recipe), or over its entry's official image.
fn built(source: &syntax::Spec, name: &str, m: &syntax::Machine, service: Service<'_>, target: Target) -> Option<Carrier> {
    let carrier = match forms::container(m, service) {
        Ok(Container::Steps { .. }) => Carrier::Built,
        Ok(Container::Entry { .. }) => Carrier::Image,
        _ => return None,
    };
    let container = match target.needs() {
        Shape::Docker | Shape::Either => true,
        Shape::VmOrLxc => overrides::lxc(source, name, super::Scope::Target(Target::Proxmox)),
        Shape::Vm | Shape::External | Shape::Cloud => false,
    };
    container.then_some(carrier)
}

/// The steps machine `name`'s VM on `host` can't run: those its family can't carry at all, then
/// (on hosts that already exist) those the configuration renderers don't reach there yet.
fn vm_steps(name: &str, m: &syntax::Machine, host: Host) -> Vec<Problem> {
    let Some(os) = m.os else { return Vec::new() };
    let at = |i: usize| FieldPath::new("machines").key(name).key("app").key("steps").index(i);
    let unportable = configure::recipe(os, m, &DaemonTable::common()).err().unwrap_or_default();
    let mut later = carry::vm_refusals(name, m, host);
    let steps = m.app().map(|a| a.steps.as_slice()).unwrap_or_default();
    // In step order, each step once: what its family can't carry at all says it best.
    let mut out = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        if let Some(u) = unportable.iter().find(|u| u.step == i) {
            let path = u.field.map_or_else(|| at(i), |f| at(i).key(step.kind()).key(f));
            out.push(Problem::new(Code::Refused, path, u.reason.clone()).fix(u.fix.clone()));
        } else if let Some(k) = later.iter().position(|p| p.path == at(i)) {
            out.push(later.swap_remove(k));
        }
    }
    out
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
        on(&testing::lab(LAB), target).iter().map(|p| p.path.to_string()).collect()
    }

    /// What `target` refuses in `lab`.
    fn on(lab: &crate::Lab, target: Target) -> Vec<Problem> {
        of(lab.source(), &Services::of(lab.catalogue(), lab.source()), target)
    }

    #[test]
    fn a_container_target_refuses_what_has_no_container_form() {
        assert_eq!(refused_on(Target::Docker), ["machines.vm.app.steps[1]"]);
    }

    #[test]
    fn a_vm_target_refuses_what_has_no_vm_form_and_renders_every_step() {
        assert_eq!(refused_on(Target::Vagrant), ["machines.img.app.image", "machines.ct.app.image"]);
    }

    #[test]
    fn proxmox_runs_an_lxc_machine_as_its_container_form() {
        assert_eq!(refused_on(Target::Proxmox), ["machines.img.app.image"]);
    }

    #[test]
    fn an_lxc_container_builds_its_steps_as_a_container_does() {
        let l = testing::lab(
            "version: 1\nname: t\nmachines:\n  ct: { os: debian@12, app: { image: alpine, steps: [{ copy: { from: f, to: /f } }] } }\noverrides: { proxmox: { machines: { ct: { lxc: true } } } }\n",
        );
        assert_eq!(on(&l, Target::Proxmox), []);
        assert_eq!(on(&l, Target::Vagrant), [], "a VM renders its copy step");
    }

    #[test]
    fn hybrid_refuses_only_a_machine_with_neither_form_and_runs_a_vm_only_where_it_must() {
        let l = testing::lab(
            "version: 1\nname: t\nmachines:\n  web: { app: { image: nginx } }\n  db: { os: windows-server@2022, app: { steps: [{ packages: [x] }] } }\n  svc: { os: debian@12, service: postgres }\n",
        );
        let found: Vec<String> = on(&l, Target::Hybrid).iter().map(|p| p.path.to_string()).collect();
        assert_eq!(found, ["machines.svc.service"], "a Windows feature is the PowerShell library's");
        assert_eq!(
            refused_on(Target::Hybrid),
            Vec::<String>::new(),
            "both has a container form: no VM steps to refuse"
        );
    }

    #[test]
    fn external_needs_a_host_for_every_machine_but_the_supplied_one() {
        let found = on(&testing::lab(LAB), Target::External);
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
        assert_eq!(on(&l, Target::CloudServices), []);
        assert_eq!(forms(l.source(), &Services::default(), Target::Vagrant), []);
    }
}
