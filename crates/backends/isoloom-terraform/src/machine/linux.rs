//! A Linux machine set up over SSH by Terraform (`remote-exec`): the project's archive and the
//! inputs file uploaded, then the machine's steps from the plan, each as a root command. A
//! `reboot` step ends a provisioner; Terraform restarts the machine and the next one checks
//! that it did before going on.

use isoloom_plan::{Environment, MachineShape, Step};
use isoloom_render::hcl_builder::{Expression, Template, reference};
use isoloom_render::steps;

use super::provisioner::{Connection, Provisioner, Upload};
use super::{Shell, at_every_boot, controller};
use crate::module::{ident, inputs};

/// Waits for cloud-init to finish its first boot (it may still be installing packages).
pub(crate) const CLOUD_INIT_WAIT: &str = "cloud-init status --wait >/dev/null 2>&1 || true";

/// What a target adds to the steps on its platform.
#[derive(Debug, Clone)]
pub(crate) struct Hooks {
    /// Run before the steps, and again after each restart (extra interfaces, found by MAC).
    pub(crate) at_boot: Vec<Template>,
    /// Run after each restart, before `at_boot` (the auto-stop, scheduled again).
    pub(crate) after_reboot: Vec<Template>,
    /// Run before the machine closes itself off: before its `Egress`, `Tc` or `Ready` step,
    /// whichever comes first (published-port redirects).
    pub(crate) before_egress: Vec<Template>,
    /// The steps the platform does itself, left out (a cloud's network routes between subnets).
    pub(crate) skip: fn(&Step) -> bool,
}

impl Default for Hooks {
    fn default() -> Hooks {
        Hooks {
            at_boot: Vec::new(),
            after_reboot: Vec::new(),
            before_egress: Vec::new(),
            skip: |_| false,
        }
    }
}

/// A Linux machine, as Terraform reaches it.
#[derive(Debug, Clone)]
pub(crate) struct Machine<'a> {
    /// The plan.
    pub(crate) env: &'a Environment,
    /// The machine.
    pub(crate) name: &'a str,
    /// What replaces its set-up when it changes (the instance's id).
    pub(crate) replace_on: Expression,
    /// Its address, from where Terraform runs.
    pub(crate) host: Expression,
    /// The SSH user (it has `sudo`).
    pub(crate) user: String,
    /// The target's additions.
    pub(crate) hooks: Hooks,
}

/// The machine's set-up: `terraform_data.<machine>`.
pub(crate) fn provisioner(m: Machine) -> Provisioner {
    let mp = &m.env.machines[m.name];
    let mut uploads = vec![Upload::Project {
        id: ident(m.name),
        destination: steps::PROJECT_ARCHIVE,
    }];
    if !mp.inputs.is_empty() {
        uploads.push(Upload::Content {
            content: inputs::file(&mp.inputs),
            destination: steps::INPUTS_FILE.to_owned(),
        });
    }
    Provisioner {
        name: ident(m.name),
        replace_on: m.replace_on.clone(),
        uploads,
        parts: parts(&m),
        depends_on: depends_on(m.env, m.name),
        connection: Connection::Ssh { host: m.host, user: m.user },
        announce: (!mp.secrets.is_empty()).then(|| m.name.to_owned()),
    }
}

/// The set-up provisioners of the VMs `machine` depends on.
pub(crate) fn depends_on(env: &Environment, machine: &str) -> Vec<Expression> {
    env.machines[machine]
        .depends_on
        .iter()
        .filter(|d| env.machines.get(*d).is_some_and(|dp| dp.shape == MachineShape::Vm))
        .map(|d| reference(&format!("terraform_data.{}", ident(d))))
        .collect()
}

/// The commands, one list per part between `reboot` steps.
fn parts(m: &Machine) -> Vec<Vec<Expression>> {
    let (env, hooks) = (m.env, &m.hooks);
    let mut first: Vec<Template> = vec!["set -e".into(), CLOUD_INIT_WAIT.into()];
    if controller::needed(env) {
        first.push(controller::authorize());
    }
    first.extend(hooks.at_boot.iter().cloned());
    let (mut done, mut part) = (Vec::new(), first);
    let mut again: Vec<Template> = Vec::new();
    let mut closing = false;
    for step in env.machines[m.name].lifecycle.iter().filter(|s| !(hooks.skip)(s)) {
        if !closing && matches!(step, Step::Egress | Step::Tc | Step::Ready) {
            part.extend(hooks.before_egress.iter().cloned());
            closing = true;
        }
        if *step == Step::Reboot {
            part.push(Shell::Sudo.command(&steps::record_boot()).into());
            let mut next: Vec<Template> = vec!["set -e".into(), Shell::Sudo.command(&steps::reboot_check()).into()];
            next.extend(hooks.after_reboot.iter().chain(&hooks.at_boot).cloned());
            next.extend(again.iter().cloned());
            done.push(std::mem::replace(&mut part, next));
        } else if let Some(c) = Shell::Sudo.step(env, m.name, step) {
            if at_every_boot(step) {
                again.push(c.clone().into());
            }
            part.push(c.into());
        }
    }
    done.push(part);
    done.into_iter().map(|p| p.into_iter().map(Template::into_expression).collect()).collect()
}
