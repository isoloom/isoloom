//! The `cloud-vm` run manifest of one cloud (`.isoloom/cloud-vm/<cloud>/run.json`), on the
//! frame every cloud run shares (`crate::cloud::manifest`): each Linux machine, and the
//! controller, a guest the runner delivers the project to and waits for; each published port at
//! its machine's own public address; each check runner on the machine it stands for, from the
//! project's copy there. A Windows machine is still set up by Terraform over WinRM (its module
//! uploads the project itself), so the runner neither delivers to it nor reads its ready file.

use std::net::IpAddr;

use isoloom_manifest::{CheckRunner, Error as ManifestError, Machine, Name, Output, OutputRole, Placeholder, Role, SecretSlot, SnakeName, Template};
use isoloom_plan::checks::{Position, by_position};
use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::OUTPUT_DIR;
use isoloom_render::run_manifest::{check, outputs_with, timeout_s};
use isoloom_spec::model::Target;

use super::{Driver, vms};
use crate::cloud::manifest::{Frame, Guest, connection, steps};
use crate::cloud::{Cloud, operator};
use crate::machine::controller;
use crate::module::{ident, manifest};

/// The controller, as the manifest names it.
const CONTROLLER: &str = "isoloom-controller";

/// How long a machine's set-up may take once the project is in, and the controller's playbooks.
const SETUP_S: u32 = 2400;
const PLAYBOOKS_S: u32 = 7200;

/// The output holding `machine`'s public address.
pub(crate) fn address_output(machine: &str) -> String {
    format!("address_{}", ident(machine).to_ascii_lowercase())
}

fn at(machine: &str) -> Result<Template, ManifestError> {
    Ok(Template::from(Placeholder::Output(SnakeName::parse(&address_output(machine))?)))
}

/// The run manifest of `env` on `cloud`, whose module `driver` writes.
pub(crate) fn manifest(env: &Environment, cloud: Cloud, driver: &Driver) -> Result<isoloom_manifest::Manifest, ManifestError> {
    let user = |mp: &MachinePlan| (driver.user)(mp.os.as_deref().unwrap_or_default()).to_owned();
    let mut guests: Vec<Guest> = vms(env)
        .filter(|(_, mp)| !mp.is_windows())
        .map(|(name, mp)| {
            Ok(Guest {
                machine: name.to_owned(),
                address: at(name)?,
                user: user(mp),
                setup_s: SETUP_S,
            })
        })
        .collect::<Result<_, ManifestError>>()?;
    if controller::needed(env) {
        guests.push(Guest {
            machine: CONTROLLER.to_owned(),
            address: at(CONTROLLER)?,
            user: (driver.user)(controller::OS).to_owned(),
            setup_s: PLAYBOOKS_S,
        });
    }
    let machines = machines(env)?;
    let mut variables = super::own_variables(env, driver);
    variables.extend(operator::host_reaching());
    let frame = Frame {
        target: isoloom_manifest::Target::CloudVm,
        cloud,
        variables,
        outputs: outputs(env, &machines)?,
        machines,
        runners: runners(env, &guests)?,
        slots: inputs(env)?,
        guests,
    };
    crate::cloud::manifest::build(env, frame)
}

/// The lab's VMs in start order, each published port fixed at its own number on the machine's
/// public address; then the controller.
fn machines(env: &Environment) -> Result<Vec<Machine>, ManifestError> {
    let mut out = Vec::new();
    for (name, mp) in vms(env) {
        let Some(spec) = env.spec().machines.get(name) else { continue };
        out.push(manifest::target_machine(name, mp, spec, &at(name)?)?);
    }
    if controller::needed(env) {
        let on = controller::addresses(env);
        let addresses = on
            .iter()
            .map(|(n, a)| Ok((Name::parse(n)?, IpAddr::V4(*a))))
            .collect::<Result<_, ManifestError>>()?;
        out.push(manifest::machine(CONTROLLER, Role::Infra, addresses)?);
    }
    Ok(out)
}

/// A Terraform output of the module.
fn state(name: &str, role: OutputRole, what: &str) -> Result<Output, ManifestError> {
    manifest::state_output(name, role, what)
}

/// Where to stand (`ip`), the ready file, each machine's public address (the controller's too),
/// then each published port at its machine's address.
fn outputs(env: &Environment, machines: &[Machine]) -> Result<Vec<Output>, ManifestError> {
    let mut out = vec![
        state(
            "ip",
            OutputRole::Address,
            "Where to stand: the access machine's public address (the first Linux machine's, else the controller's).",
        )?,
        state(
            "ready_file",
            OutputRole::Info,
            "Where each Linux machine's set-up says how far it got (isoloom-ready/1).",
        )?,
    ];
    let named = vms(env).map(|(n, _)| n).chain(controller::needed(env).then_some(CONTROLLER));
    for name in named {
        out.push(state(
            &address_output(name),
            OutputRole::Address,
            &format!("{name}'s public address: the runner's SSH and its published ports reach it there."),
        )?);
    }
    let shown_at = |m: &Machine| {
        m.ports
            .iter()
            .find_map(|p| p.publish.as_ref().and_then(|x| x.address.clone()))
            .unwrap_or_else(|| Template::text("127.0.0.1"))
    };
    out.extend(outputs_with(machines, shown_at)?);
    Ok(out)
}

/// A runner per position: on its machine when that is a Linux VM, else on the controller, else
/// on the first Linux machine, as on Proxmox (an environment with no Linux machine and no
/// controller has no runner here).
fn runners(env: &Environment, guests: &[Guest]) -> Result<Vec<CheckRunner>, ManifestError> {
    let mut out = Vec::new();
    for (pos, group) in by_position(env.spec(), &env.checks) {
        let on = match &pos {
            Position::Machine(m) => guests.iter().find(|g| &g.machine == m),
            Position::Networks => None,
        }
        .or_else(|| guests.iter().find(|g| g.machine == CONTROLLER))
        .or_else(|| guests.first());
        let Some(guest) = on else { continue };
        let script = format!("{OUTPUT_DIR}/{}/checks/{}.sh", Target::CloudVm.id(), pos.id());
        out.push(CheckRunner {
            id: Name::parse(pos.id())?,
            from: format!("from {}", guest.machine),
            checks: group.iter().map(|c| check(c)).collect(),
            step: steps::check_step(pos.id(), guest, &script, timeout_s(&group))?,
            on_timeout: Vec::new(),
        });
    }
    Ok(out)
}

/// The launch's plain values (`sensitive: false`), one entry each of the module's `inputs`,
/// written into the machines that read them by their own set-up.
fn inputs(env: &Environment) -> Result<Vec<SecretSlot>, ManifestError> {
    let spec = env.spec();
    let mut out = Vec::new();
    for input in &spec.inputs {
        let readers = manifest::input_readers(env, input)?;
        if readers.is_empty() {
            continue;
        }
        let sensitive = !spec.plain_inputs.contains(input);
        out.push(connection::input(input, sensitive, readers)?);
    }
    out.extend(machine_secrets(env)?);
    Ok(out)
}

/// Each secret a machine reads (`secrets:`): delivered over SSH, so the value stays out of the
/// state. Owners are the machine's set-up's business (its `Secrets` step places the file).
fn machine_secrets(env: &Environment) -> Result<Vec<SecretSlot>, ManifestError> {
    let spec = env.spec();
    let mut out = Vec::new();
    for name in spec.secrets.keys() {
        let readers: Vec<Name> = vms(env)
            .filter(|(_, mp)| mp.secrets.iter().any(|s| &s.name == name))
            .map(|(n, _)| Name::parse(n))
            .collect::<Result<_, _>>()?;
        if readers.is_empty() {
            continue;
        }
        out.push(connection::guest_secret(name, true, &readers)?);
    }
    Ok(out)
}
