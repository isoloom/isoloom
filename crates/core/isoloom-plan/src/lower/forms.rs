//! A machine's two forms as lowering builds them: the container form with the image it runs
//! from, the VM form with its OS, or why the machine has none, at the field that rules it out.
//! The spec says which forms a machine's fields allow ([`syntax::container_form`],
//! [`syntax::vm_form`]); here they are resolved against what Isoloom can produce today (an OS's
//! official image, the catalogue's entries). Lowering and the refusals both read them, so a
//! machine a target lowers without its form is always a refusal at that field, never a silent
//! stand-in.

use isoloom_spec::syntax::{self, Form, Kind, NoForm, OsRef, Recipe, Step};

use crate::configure::{self, ContainerProcess, DaemonTable, NoProcess};

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
}

/// What a machine's VM is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Vm {
    /// The runner supplies it: an access machine with no `os`.
    Supplied,
    /// A fresh VM of its OS, set up by the app's steps when it has any.
    Os(OsRef),
}

/// The machine's container form, or why it has none.
pub(super) fn container(m: &syntax::Machine) -> Result<Container<'_>, NoForm> {
    match syntax::container_form(m)? {
        Form::Supplied => Ok(Container::Supplied),
        Form::Recipe(recipe) => Ok(Container::Recipe(recipe)),
        Form::Service => Err(no_entry(m)),
        Form::Steps => {
            let (os, base) = official_image(m)?;
            process(os, m)?;
            Ok(Container::Steps { os, base })
        }
        Form::Os => official_image(m).map(|(_, image)| Container::Os { image }),
    }
}

/// The machine's VM form, or why it has none.
pub(super) fn vm(m: &syntax::Machine) -> Result<Vm, NoForm> {
    match syntax::vm_form(m)? {
        Form::Supplied => Ok(Vm::Supplied),
        Form::Service => Err(no_entry(m)),
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
            fix: "add `command:` (the service in the foreground); a `service` step runs one in containers from Phase 4.2".to_owned(),
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

/// A catalogue service, which no target runs until the catalogue has its entry (Phase 4).
fn no_entry(m: &syntax::Machine) -> NoForm {
    let names: Vec<String> = match &m.kind {
        Kind::Service(s) => s.services.iter().map(|r| format!("`{}`", r.name())).collect(),
        Kind::App(_) | Kind::Bare => Vec::new(),
    };
    NoForm {
        field: "service".to_owned(),
        reason: format!("no catalogue entry {} yet: the catalogue arrives in Phase 4", names.join(", ")),
        fix: "write it as an `app:` for now (`app: { image: <ref> }`, or `os` and `app.steps`)".to_owned(),
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
        assert!(matches!(container(&os), Ok(Container::Os { image }) if image.starts_with("debian:12@sha256:")));
        assert!(matches!(vm(&os), Ok(Vm::Os(o)) if o.name() == "debian-12"));
        let steps = machine("{ os: alpine, app: { steps: [{ run: a.sh }] } }");
        assert!(matches!(container(&steps), Ok(Container::Steps { os: alpine, base }) if alpine.name().starts_with("alpine") && base.starts_with("alpine:")));
        assert!(matches!(vm(&steps), Ok(Vm::Os(_))));
    }

    #[test]
    fn an_access_machine_without_an_os_is_supplied_in_both_forms() {
        let access = machine("{ access: true }");
        assert_eq!((container(&access), vm(&access)), (Ok(Container::Supplied), Ok(Vm::Supplied)));
    }

    #[test]
    fn a_recipe_is_the_containers_own() {
        let built = machine("{ app: { build: build/web } }");
        assert!(matches!(container(&built), Ok(Container::Recipe(Recipe::Build(_)))));
        assert_eq!(vm(&built).map_err(|no| no.field), Err("app.build".to_owned()));
    }

    #[test]
    fn a_catalogue_service_has_no_form_until_its_entry_exists() {
        let db = machine("{ os: debian@12, service: postgres }");
        for no in [container(&db).map(|_| ()), vm(&db).map(|_| ())] {
            let no = no.expect_err("no form");
            assert_eq!((no.field.as_str(), no.path("m").as_str()), ("service", "machines.m.service"));
            assert!(no.reason.contains("`postgres`") && no.reason.contains("Phase 4"), "{}", no.reason);
        }
    }

    #[test]
    fn windows_has_no_container_form() {
        let dc = machine("{ os: windows-server@2022 }");
        assert_eq!(container(&dc).map_err(|no| no.field), Err("os".to_owned()));
        assert!(matches!(vm(&dc), Ok(Vm::Os(_))));
    }
}
