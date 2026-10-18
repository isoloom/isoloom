//! A Linux machine's steps from the plan, each as a root command, in one of two transports:
//!
//! - **Over SSH by Terraform** ([`provisioner`], `remote-exec`): the project's archive and the
//!   inputs file uploaded, then the steps. A `reboot` step ends a provisioner; Terraform restarts
//!   the machine and the next one checks that it did before going on (ESXi).
//! - **From its own user data** ([`set_up`], the clouds): the machine waits for the project the
//!   Isothrum runner delivers, then runs the same steps itself (`super::user_data`), a `reboot`
//!   step ending a part.

use isoloom_plan::{Environment, MachinePlan, MachineShape, Step};
use isoloom_render::hcl_builder::{Expression, Template, reference};
use isoloom_render::steps;

use super::provisioner::{Connection, Provisioner, Upload};
use super::user_data::{SetUp, Tracked, expiry_shutdown, project_wait};
use super::{Remote, Shell, at_every_boot, cloud_init, controller};
use crate::module::{ident, inputs};

pub(crate) use isoloom_render::steps::CLOUD_INIT_WAIT;

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

/// A Linux machine, as Terraform reaches it over SSH.
#[derive(Debug, Clone)]
pub(crate) struct Machine<'a> {
    /// The machine.
    pub(crate) remote: Remote<'a>,
    /// The SSH user (it has `sudo`).
    pub(crate) user: String,
    /// The target's additions.
    pub(crate) hooks: Hooks,
}

/// The machine's set-up: `terraform_data.<machine>`.
pub(crate) fn provisioner(m: Machine) -> Provisioner {
    let Remote { env, name, plan: mp, .. } = m.remote;
    let mut uploads = vec![Upload::Project {
        id: ident(name),
        destination: steps::PROJECT_ARCHIVE,
    }];
    if !mp.inputs.is_empty() {
        uploads.push(Upload::Content {
            content: inputs::file(&mp.inputs),
            destination: steps::INPUTS_FILE.to_owned(),
        });
    }
    Provisioner {
        name: ident(name),
        replace_on: m.remote.replace_on.clone(),
        uploads,
        parts: parts(&m),
        depends_on: depends_on(env, mp),
        connection: Connection::Ssh {
            host: m.remote.host,
            user: m.user,
        },
        announce: (!mp.secrets.is_empty()).then(|| name.to_owned()),
    }
}

/// The set-up provisioners of the VMs machine `mp` depends on.
pub(crate) fn depends_on(env: &Environment, mp: &MachinePlan) -> Vec<Expression> {
    mp.depends_on
        .iter()
        .filter(|d| env.machines.get(*d).is_some_and(|dp| dp.shape == MachineShape::Vm))
        .map(|d| reference(&format!("terraform_data.{}", ident(d))))
        .collect()
}

/// The commands, one list per part between `reboot` steps.
fn parts(m: &Machine) -> Vec<Vec<Expression>> {
    let (Remote { env, name, plan, .. }, hooks) = (&m.remote, &m.hooks);
    let mut first: Vec<Template> = vec!["set -e".into(), CLOUD_INIT_WAIT.into()];
    // The ready file's folder, before the first step's progress mark (`tracked_step`).
    first.push(Shell::Sudo.command(&format!("mkdir -p {}", cloud_init::READY_DIR)).into());
    if controller::needed(env) {
        first.push(controller::authorize());
    }
    first.extend(hooks.at_boot.iter().cloned());
    let (mut done, mut part) = (Vec::new(), first);
    let mut again: Vec<Template> = Vec::new();
    let mut closing = false;
    for step in plan.lifecycle.iter().filter(|s| !(hooks.skip)(s)) {
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
        } else if let Some(c) = Shell::Sudo.tracked_step(env, name, step) {
            if at_every_boot(step)
                && let Some(raw) = Shell::Sudo.step(env, name, step)
            {
                // Re-run after a reboot (the interfaces), without a repeated progress mark.
                again.push(raw.into());
            }
            part.push(c.into());
        }
    }
    done.push(part);
    done.into_iter().map(|p| p.into_iter().map(Template::into_expression).collect()).collect()
}

/// Machine `name`'s set-up from its own user data, logging in as `user`: the project the runner
/// delivers, the controller's key, the launch inputs, `hooks.at_boot`, then the steps the
/// platform doesn't do itself (`hooks.skip`), `hooks.before_egress` before the machine closes
/// itself off. A `reboot` step ends a part; the next one starts with what a restart undid.
pub(crate) fn set_up(env: &Environment, (name, mp): (&str, &MachinePlan), user: &str, hooks: &Hooks) -> SetUp {
    let mut first = vec![project_wait()];
    if controller::needed(env) {
        first.push(Tracked::new("the controller's key", controller::authorize_as(user)));
    }
    let inputs: Vec<String> = mp.inputs.clone();
    let mut again: Vec<Template> = Vec::new();
    if !inputs.is_empty() {
        let in_place = Template::text(format!("install -m 600 {} {}", steps::KEPT_INPUTS_FILE, steps::INPUTS_FILE));
        first.push(Tracked::new("inputs", inputs::written(&inputs)));
        first.push(Tracked::new("inputs in place", in_place.clone()));
        again.push(in_place);
    }
    first.extend(hooks.at_boot.iter().map(|c| Tracked::new("the machine's interfaces", c.clone())));
    again.extend(hooks.at_boot.iter().cloned());
    let (mut parts, mut part) = (Vec::new(), first);
    let mut closing = false;
    for step in mp
        .lifecycle
        .iter()
        .filter(|s| !(hooks.skip)(s) && !matches!(s, Step::Project | Step::Inputs { .. }))
    {
        if !closing && matches!(step, Step::Egress | Step::Tc | Step::Ready) {
            part.extend(hooks.before_egress.iter().map(|c| Tracked::new("published ports", c.clone())));
            closing = true;
        }
        if *step == Step::Reboot {
            parts.push(std::mem::take(&mut part));
        } else if let Some(c) = Shell::Root.step(env, name, step) {
            if at_every_boot(step) {
                again.push(c.clone().into());
            }
            part.push(Tracked::new(cloud_init::label(step), c));
        }
    }
    parts.push(part);
    SetUp {
        each_boot: vec![expiry_shutdown()],
        parts,
        again,
    }
}
