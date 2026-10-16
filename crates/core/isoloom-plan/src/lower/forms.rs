//! A machine's two forms as lowering builds them: the container form with the image it runs
//! from, the VM form with its OS, or why the machine has none, at the field that rules it out.
//! The spec says which forms a machine's fields allow ([`syntax::container_form`],
//! [`syntax::vm_form`]); here they are resolved against what Isoloom can produce today (an OS's
//! official image, the catalogue's entries). Lowering and the refusals both read them, so a
//! machine a target lowers without its form is always a refusal at that field, never a silent
//! stand-in.

use isoloom_catalogue::{ImageStart, Resolved};
use isoloom_spec::syntax::{self, Form, NoForm, OsRef, Problems, Recipe, Step};

use crate::configure::{self, ContainerProcess, DaemonTable, NoProcess, Recipe as Resolution};

/// A machine's catalogue services, as lowering resolved them (`None`: it runs none).
pub(super) type Service<'a> = Option<&'a Result<Vec<Resolved>, Problems>>;

/// What a machine's container runs from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Container<'a> {
    /// The runner supplies it: an access machine with no `os`.
    Supplied,
    /// The app's own recipe: an image, a build, an appliance.
    Recipe(&'a Recipe),
    /// An image Isoloom builds from the OS's image (`base`) and the app's steps.
    Steps {
        /// The OS the steps install onto.
        os: OsRef,
        /// The OS's official image, pinned by digest.
        base: &'static str,
    },
    /// The OS's official image, kept running idle.
    Os {
        /// The image, pinned by digest.
        image: &'static str,
    },
    /// A catalogue entry's official image, its OS the machine's: a thin build of the entry's
    /// `configure` over it, and the image's own process.
    Entry {
        /// The OS.
        os: OsRef,
        /// The image, pinned by digest.
        image: &'a str,
    },
}

/// What a machine's VM is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Vm {
    /// The runner supplies it: an access machine with no `os`.
    Supplied,
    /// A fresh VM of its OS, set up by the app's steps when it has any.
    Os(OsRef),
}

/// The machine's container form, or why it has none (`service`: its catalogue services).
pub(super) fn container<'a>(m: &'a syntax::Machine, service: Service<'a>) -> Result<Container<'a>, NoForm> {
    match syntax::container_form(m)? {
        Form::Supplied => Ok(Container::Supplied),
        Form::Recipe(recipe) => Ok(Container::Recipe(recipe)),
        Form::Service => service_container(m, resolved(service)?),
        Form::Steps => {
            let (os, base) = official_image(m)?;
            process(os, m)?;
            Ok(Container::Steps { os, base })
        }
        Form::Os => official_image(m).map(|(_, image)| Container::Os { image }),
    }
}

/// The machine's VM form, or why it has none (`service`: its catalogue services).
pub(super) fn vm(m: &syntax::Machine, service: Service<'_>) -> Result<Vm, NoForm> {
    match syntax::vm_form(m)? {
        Form::Supplied => Ok(Vm::Supplied),
        Form::Service => resolved(service).and_then(|_| m.os.map(Vm::Os).ok_or_else(|| no_os("a VM is a fresh install of an OS"))),
        Form::Recipe(_) | Form::Steps | Form::Os => m.os.map(Vm::Os).ok_or_else(|| no_os("a VM is a fresh install of an OS")),
    }
}

/// The OS and its official image, for the container forms built from the OS.
fn official_image(m: &syntax::Machine) -> Result<(OsRef, &'static str), NoForm> {
    let os = m.os.ok_or_else(|| no_os("the container is built from the OS's image"))?;
    let image = isoloom_spec::os::container_image(os.name()).ok_or_else(|| NoForm {
        field: "os".to_owned(),
        reason: format!("`{}` has no official container image", os.os().id()),
        fix: "run it on a VM target, or give the app its own `app.image` or `app.build`".to_owned(),
    })?;
    Ok((os, image))
}

/// What the container built from the machine's steps runs (the configuration model's rule,
/// design §3): refused when a service has no foreground form, or when it has ports and nothing
/// serves them.
fn process(os: OsRef, m: &syntax::Machine) -> Result<(), NoForm> {
    // A step no form of the OS carries is the VM's refusal to report; a container of a Windows
    // OS doesn't exist.
    let Ok(recipe) = configure::recipe(os, m, &DaemonTable::common()) else {
        return Ok(());
    };
    match ContainerProcess::of(&recipe, m) {
        Ok(_) => Ok(()),
        Err(NoProcess::NoForeground { unit }) => {
            let steps = m.app().map(|a| a.steps.as_slice()).unwrap_or_default();
            let i = steps
                .iter()
                .position(|s| matches!(s, Step::Service(svc) if *svc.name() == unit))
                .unwrap_or_default();
            Err(NoForm {
                field: format!("app.steps[{i}].service"),
                reason: format!("`{unit}` isn't in the daemon table, so a container can't run it"),
                fix: format!("`{{ service: {{ name: {unit}, run: [<program>, <args>] }} }}`: the service in the foreground"),
            })
        }
        Err(_) => Err(NoForm {
            field: "command".to_owned(),
            reason: "its steps install services, but no `service` step or `command` runs one in a container".to_owned(),
            fix: "a `service` step for what the steps install, or `command:` (the service in the foreground)".to_owned(),
        }),
    }
}

/// A form that needs the machine's OS, which it doesn't name.
fn no_os(reason: &str) -> NoForm {
    NoForm {
        field: "os".to_owned(),
        reason: reason.to_owned(),
        fix: "`os: debian@12`".to_owned(),
    }
}

/// The services `service` resolved, or the first problem the catalogue found, as the field that
/// rules every form out.
fn resolved(service: Service<'_>) -> Result<&[Resolved], NoForm> {
    match service {
        Some(Ok(r)) => Ok(r),
        Some(Err(problems)) => Err(problems.first().map_or_else(
            || no_os("a catalogue service runs on an OS"),
            |p| NoForm {
                field: p.path.as_str().splitn(3, '.').nth(2).unwrap_or("service").to_owned(),
                reason: p.reason.clone(),
                fix: p.fix.clone().unwrap_or_default(),
            },
        )),
        None => Err(no_os("a catalogue service runs on an OS")),
    }
}

/// A service machine's container: the entry's official image where the release publishes one
/// for the machine's OS (and its architecture), else built from the OS's image with the entry's
/// recipe; never one whose `start` restarts it, nor one whose services have no foreground form.
fn service_container<'a>(m: &syntax::Machine, resolved: &'a [Resolved]) -> Result<Container<'a>, NoForm> {
    // The spec refuses several services on a container before this.
    let Some(r) = resolved.first() else {
        return Err(no_os("a catalogue service runs on an OS"));
    };
    if r.start.contains(&Step::Reboot) {
        return Err(NoForm {
            field: "service".to_owned(),
            reason: format!("{}'s `start` restarts the machine, and a container can't reboot", r.entry),
            fix: "a VM target".to_owned(),
        });
    }
    if let Some(image) = &r.image {
        if image.start == ImageStart::Run && !r.start.is_empty() {
            return Err(NoForm {
                field: "service".to_owned(),
                reason: format!(
                    "{}'s image runs its `start` steps through Isoloom's runner (`image.start: run`), which no container renders yet",
                    r.entry
                ),
                fix: "a VM target".to_owned(),
            });
        }
        if !image.platforms.contains(&m.hardware.arch) {
            return Err(NoForm {
                field: "hardware.arch".to_owned(),
                reason: format!("{} {} publishes no `{}` image", r.entry, r.release, arch(m.hardware.arch)),
                fix: format!("another release, or `{}`", arch(isoloom_spec::model::Arch::Amd64)),
            });
        }
        return Ok(Container::Entry {
            os: r.os,
            image: &image.reference,
        });
    }
    let (os, base) = official_image(m)?;
    let config = super::service::configuration(resolved).ok_or_else(|| no_os("a catalogue service runs on an OS"))?;
    let ports: Vec<isoloom_spec::model::Port> = Vec::new();
    let process = Resolution::of(&config, &ports, &DaemonTable::common())
        .ok()
        .map(|recipe| ContainerProcess::resolve(&recipe, None, false));
    if let Some(Err(NoProcess::NoForeground { unit })) = process {
        return Err(NoForm {
            field: "service".to_owned(),
            reason: format!("{}'s `{unit}` isn't in the daemon table, so a container can't run it", r.entry),
            fix: "a VM target".to_owned(),
        });
    }
    Ok(Container::Steps { os, base })
}

/// How a spec writes an architecture.
fn arch(a: isoloom_spec::model::Arch) -> &'static str {
    match a {
        isoloom_spec::model::Arch::Amd64 => "amd64",
        isoloom_spec::model::Arch::Arm64 => "arm64",
    }
}

#[cfg(test)]
mod tests {
    use isoloom_spec::syntax::Machine;

    use super::*;
    use crate::testing;

    /// Machine `m` of a lab that has only it, written as `yaml`.
    fn machine(yaml: &str) -> Machine {
        let lab = testing::lab(&format!("version: 1\nname: t\nmachines:\n  m: {yaml}\n"));
        lab.source().machines.get("m").cloned().expect("the test's machine")
    }

    #[test]
    fn an_os_runs_as_its_official_image_or_a_fresh_vm() {
        let os = machine("{ os: debian@12 }");
        assert!(matches!(container(&os, None), Ok(Container::Os { image }) if image.starts_with("debian:12@sha256:")));
        assert!(matches!(vm(&os, None), Ok(Vm::Os(o)) if o.name() == "debian-12"));
        let steps = machine("{ os: alpine, app: { steps: [{ run: a.sh }] } }");
        assert!(
            matches!(container(&steps, None), Ok(Container::Steps { os: alpine, base }) if alpine.name().starts_with("alpine") && base.starts_with("alpine:"))
        );
        assert!(matches!(vm(&steps, None), Ok(Vm::Os(_))));
    }

    #[test]
    fn an_access_machine_without_an_os_is_supplied_in_both_forms() {
        let access = machine("{ access: true }");
        assert_eq!((container(&access, None), vm(&access, None)), (Ok(Container::Supplied), Ok(Vm::Supplied)));
    }

    #[test]
    fn a_recipe_is_the_containers_own() {
        let built = machine("{ app: { build: build/web } }");
        assert!(matches!(container(&built, None), Ok(Container::Recipe(Recipe::Build(_)))));
        assert_eq!(vm(&built, None).map_err(|no| no.field), Err("app.build".to_owned()));
    }

    #[test]
    fn a_catalogue_service_with_no_entry_has_no_form_at_its_service() {
        let db = machine("{ os: debian@12, service: postgres }");
        let service = Some(Err(vec![syntax::Problem::new(
            syntax::Code::UnknownReference,
            syntax::FieldPath::new("machines.m.service"),
            "the catalogue has no entry `postgres`",
        )]));
        for no in [container(&db, service.as_ref()).map(|_| ()), vm(&db, service.as_ref()).map(|_| ())] {
            let no = no.expect_err("no form");
            assert_eq!((no.field.as_str(), no.path("m").as_str()), ("service", "machines.m.service"));
            assert_eq!(no.reason, "the catalogue has no entry `postgres`");
        }
    }

    #[test]
    fn windows_has_no_container_form() {
        let dc = machine("{ os: windows-server@2022 }");
        assert_eq!(container(&dc, None).map_err(|no| no.field), Err("os".to_owned()));
        assert!(matches!(vm(&dc, None), Ok(Vm::Os(_))));
    }
}
