//! The forms a machine takes, derived from what it is: a container (its image, its build, a
//! Dockerfile generated from its steps, its OS's image) and a VM (a fresh VM of its OS, set up by
//! its steps). A machine without one of them says why, naming the field that rules it out, so a
//! target that needs that form refuses the machine with the reason. What a container built from
//! steps runs (its services, its command) is the configuration model's rule, in the plan.

use super::app::Recipe;
use super::machine::{Kind, Machine};
use super::problem::{Code, FieldPath, Problem};
use super::step::Step;

/// How a machine is produced in one form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Form<'a> {
    /// The runner supplies it (an access machine with no `os`).
    Supplied,
    /// A container from the app's own recipe.
    Recipe(&'a Recipe),
    /// A container Isoloom builds from the OS's image and the steps.
    Steps,
    /// The OS alone: its image kept running idle, or a fresh VM.
    Os,
    /// A catalogue service.
    Service,
}

/// Why a machine has no form of a kind: the field that rules it out, why, and what to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoForm {
    /// The field, under the machine (`app.build`, `os`, `app.steps[2]`).
    pub field: String,
    /// Why that field rules the form out.
    pub reason: String,
    /// What would give the machine the form.
    pub fix: String,
}

impl NoForm {
    fn new(field: impl Into<String>, reason: impl Into<String>, fix: impl Into<String>) -> NoForm {
        NoForm {
            field: field.into(),
            reason: reason.into(),
            fix: fix.into(),
        }
    }

    /// The `refused` problem of a target that needs the form, at the field for machine `name`.
    #[must_use]
    pub fn refusal(&self, name: &str) -> Problem {
        Problem::new(Code::Refused, self.path(name), self.reason.clone()).fix(self.fix.clone())
    }

    /// The field's full path for machine `name`.
    #[must_use]
    pub fn path(&self, name: &str) -> FieldPath {
        let machine = FieldPath::new("machines").key(name);
        match self.field.split_once('[') {
            Some((key, rest)) => FieldPath::new(format!("{}[{rest}", machine.key(key))),
            None => machine.key(&self.field),
        }
    }
}

/// The machine's container form, or why it has none.
///
/// # Errors
///
/// [`NoForm`]: the field that rules the form out, why, and the fix.
pub fn container(m: &Machine) -> Result<Form<'_>, NoForm> {
    if m.access && m.os.is_none() && matches!(m.kind, Kind::Bare) {
        return Ok(Form::Supplied);
    }
    if let Some(os) = m.os
        && os.is_windows()
        && !matches!(&m.kind, Kind::App(a) if a.container.is_some())
    {
        return Err(NoForm::new(
            "os",
            "Windows has no container form in 1.0",
            "run it on a VM target (`vagrant`, `proxmox`, `cloud-vm`)",
        ));
    }
    if let Some(field) = m.hardware.vm_only() {
        return Err(NoForm::new(
            format!("hardware.{field}"),
            "only a VM gives it: a container has no kernel or firmware of its own",
            "use a VM target, or leave it out for the container form",
        ));
    }
    match &m.kind {
        Kind::Service(s) if s.services.len() > 1 => Err(NoForm::new(
            "service",
            "several services share a VM, but a container runs one",
            "one machine per service, or a VM target",
        )),
        Kind::Service(_) => Ok(Form::Service),
        Kind::App(a) => match &a.container {
            Some(recipe) => Ok(Form::Recipe(recipe)),
            None => match a.steps.iter().enumerate().find_map(|(i, s)| s.container_refusal().map(|why| (i, why))) {
                Some((i, why)) => Err(NoForm::new(
                    format!("app.steps[{i}]"),
                    why,
                    "give the container form its own recipe (`app.image` or `app.build`), or use a VM target",
                )),
                None if m.os.is_some() => Ok(Form::Steps),
                None => Err(NoForm::new("app.steps", "steps install onto an OS", "`os: debian@12`")),
            },
        },
        Kind::Bare if m.os.is_some() => Ok(Form::Os),
        Kind::Bare => Err(NoForm::new("os", "a machine says what it is", "`os: debian@12`, plus `app:` or `service:`")),
    }
}

/// The machine's VM form, or why it has none.
///
/// # Errors
///
/// [`NoForm`]: the field that rules the form out, why, and the fix.
pub fn vm(m: &Machine) -> Result<Form<'_>, NoForm> {
    if m.access && m.os.is_none() && matches!(m.kind, Kind::Bare) {
        return Ok(Form::Supplied);
    }
    if !m.hardware.devices.is_empty() {
        return Err(NoForm::new(
            "hardware.devices",
            "the host's devices go to a container on the host's own Docker; a VM has its own",
            "use a container target, or leave them out for the VM form",
        ));
    }
    match &m.kind {
        Kind::App(a) if a.steps.is_empty() => match &a.container {
            Some(Recipe::Image(_)) => Err(NoForm::new(
                "app.image",
                "an image's filesystem can't be reproduced on a fresh VM",
                "add `os` and `app.steps` for a VM form, or use `docker`, `docker-vm` or `cloud-docker`",
            )),
            Some(Recipe::Build(_)) => Err(NoForm::new(
                "app.build",
                "a Dockerfile builds a container only",
                "add `os` and `app.steps`, or use `docker`, `docker-vm` or `cloud-docker`",
            )),
            Some(Recipe::Appliance(_)) => Err(NoForm::new(
                "app.appliance",
                "an appliance's packaging exists only as a container",
                "use `docker`, `docker-vm` or `cloud-docker`",
            )),
            None => Err(NoForm::new("app", "an app needs a recipe", "`app: { steps: [...] }` with `os`")),
        },
        _ if m.os.is_none() => Err(NoForm::new("os", "a VM is a fresh install of an OS", "`os: debian@12`")),
        Kind::Service(_) => Ok(Form::Service),
        Kind::App(_) => Ok(Form::Steps),
        Kind::Bare => Ok(Form::Os),
    }
}

/// Whether one of the machine's steps restarts it (its inputs must outlive the boot).
#[must_use]
pub fn reboots(m: &Machine) -> bool {
    m.app().is_some_and(|a| a.steps.contains(&Step::Reboot))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(yaml: &str) -> Machine {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    #[test]
    fn services_and_bare_machines_take_the_forms_their_os_allows() {
        let one = machine("{ os: debian@12, service: postgres@16 }");
        assert_eq!((container(&one), vm(&one)), (Ok(Form::Service), Ok(Form::Service)));
        let two = machine("{ os: debian@12, service: [nginx, php-fpm] }");
        assert_eq!(container(&two).unwrap_err().field, "service");
        assert_eq!(vm(&two), Ok(Form::Service));
        let no_os = machine("{ networks: { lab: 10 } }");
        assert_eq!(
            (container(&no_os).unwrap_err().field, vm(&no_os).unwrap_err().field),
            ("os".to_owned(), "os".to_owned())
        );
        let steps_no_os = machine("{ app: { steps: [{ run: a.sh }] } }");
        assert_eq!(container(&steps_no_os).unwrap_err().field, "app.steps");
        assert_eq!(vm(&steps_no_os).unwrap_err().field, "os");
        assert_eq!(container(&machine("{ os: debian@12, app: { steps: [{ run: a.sh }] } }")), Ok(Form::Steps));
    }

    #[test]
    fn only_a_reboot_step_reboots() {
        assert!(!reboots(&machine("{ os: debian@12, app: { steps: [{ run: a.sh }] } }")));
        assert!(!reboots(&machine("{ os: debian@12 }")));
    }

    #[test]
    fn a_refusal_is_placed_at_its_field() {
        let built = machine("{ app: { build: build/web } }");
        let refusal = vm(&built).unwrap_err().refusal("web");
        assert_eq!((refusal.code, refusal.path.as_str()), (Code::Refused, "machines.web.app.build"));
        assert_eq!(
            refusal.fix.as_deref(),
            Some("add `os` and `app.steps`, or use `docker`, `docker-vm` or `cloud-docker`")
        );
    }

    #[test]
    fn each_recipe_gives_its_forms() {
        let os_only = machine("{ os: debian@12 }");
        assert_eq!((container(&os_only), vm(&os_only)), (Ok(Form::Os), Ok(Form::Os)));
        let built = machine("{ app: { build: build/web } }");
        assert!(matches!(container(&built), Ok(Form::Recipe(_))));
        assert_eq!(vm(&built).unwrap_err().path("web").as_str(), "machines.web.app.build");
        let rebooting = machine("{ os: debian@12, app: { steps: [{ run: a.sh }, reboot] } }");
        assert_eq!(container(&rebooting).unwrap_err().path("web").as_str(), "machines.web.app.steps[1]");
        assert_eq!(vm(&rebooting), Ok(Form::Steps));
        assert!(reboots(&rebooting));
        let both = machine("{ os: debian@12, app: { image: redis:7, steps: [reboot] } }");
        assert!(matches!(container(&both), Ok(Form::Recipe(_))) && vm(&both).is_ok());
    }

    #[test]
    fn hardware_only_one_form_has_rules_out_the_other() {
        let tpm = machine("{ os: debian@12, hardware: { tpm: required } }");
        assert_eq!(container(&tpm).unwrap_err().field, "hardware.tpm");
        assert_eq!(vm(&tpm), Ok(Form::Os));
        let fuse = machine("{ os: debian@12, hardware: { devices: [/dev/fuse] } }");
        assert_eq!(vm(&fuse).unwrap_err().field, "hardware.devices");
        assert_eq!(container(&fuse), Ok(Form::Os));
    }

    #[test]
    fn windows_and_access_machines() {
        let windows = machine("{ os: windows-server@2019, app: { steps: [{ run: iis.ps1 }] } }");
        assert_eq!(container(&windows).unwrap_err().field, "os");
        let supplied = machine("{ access: true }");
        assert_eq!((container(&supplied), vm(&supplied)), (Ok(Form::Supplied), Ok(Form::Supplied)));
        let kali = machine("{ access: true, os: kali }");
        assert_eq!(container(&kali), Ok(Form::Os));
    }
}
