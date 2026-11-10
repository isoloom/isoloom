//! The `cloud-vm` run manifest of one cloud (`.isoloom/cloud-vm/<cloud>/run.json`), on the
//! frame every cloud run shares (`crate::cloud::manifest`): each Linux machine, and the
//! controller, a guest the runner delivers the project to and waits for; each published port at
//! its machine's own public address; each check runner on the machine it stands for, from the
//! project's copy there. A Windows machine is still set up by Terraform over WinRM (its module
//! uploads the project itself), so the runner neither delivers to it nor reads its ready file.

use crate::ssh::Login;
use std::net::IpAddr;

use isoloom_manifest::contract::OUTPUT_DIR;
use isoloom_manifest::{CheckRunner, Error as ManifestError, Machine, Name, Output, OutputRole, Placeholder, Role, SecretSlot, SnakeName, Template};
use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::run_manifest::{outputs_with, timeout_s};

use crate::module::manifest::terraform_output as state;
use isoloom_spec::model::Target;

use super::{Driver, vms};
use crate::cloud::manifest::{Frame, Guest, connection, steps};
use crate::cloud::{Cloud, operator};
use crate::machine::controller;
use crate::module::{ident, manifest};

/// The controller, as the manifest names it.
use crate::machine::controller::NAME as CONTROLLER;

/// The output holding `machine`'s public address.
pub(crate) fn address_output(machine: &str) -> String {
    format!("address_{}", ident(machine).to_ascii_lowercase())
}

/// The output holding the SSH user the runner's key lets in on `machine` (its guest delivery
/// signs in as it).
pub(crate) fn ssh_user_output(machine: &str) -> String {
    format!("ssh_user_{}", ident(machine).to_ascii_lowercase())
}

fn at(machine: &str) -> Result<Template, ManifestError> {
    Ok(Template::from(Placeholder::Output(SnakeName::parse(&address_output(machine))?)))
}

/// The run manifest of `env` on `cloud`, whose module `driver` writes.
pub(crate) fn manifest(env: &Environment, cloud: Cloud, driver: &Driver) -> Result<isoloom_manifest::Manifest, ManifestError> {
    let user = |mp: &MachinePlan| (driver.base.user)(mp.os.as_deref().unwrap_or_default());
    let mut guests: Vec<Guest> = vms(env)
        .filter(|(_, mp)| !mp.is_windows())
        .map(|(name, mp)| {
            Ok(Guest {
                machine: name.to_owned(),
                login: Login::new(at(name)?, user(mp)),
                setup_s: manifest::SETUP_S,
            })
        })
        .collect::<Result<_, ManifestError>>()?;
    if controller::needed(env) {
        guests.push(Guest {
            machine: CONTROLLER.to_owned(),
            login: Login::new(at(CONTROLLER)?, (driver.base.user)(controller::OS)),
            setup_s: manifest::PLAYBOOKS_S,
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
        out.push(manifest::lab_machine(name, mp, spec, &at(name)?)?);
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
        out.push(state(
            &ssh_user_output(name),
            OutputRole::Info,
            &format!("The user the runner's key lets in on {name}."),
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

/// A runner per position, on the machine `crate::machine::check_machine` says (one of the
/// guests: the Linux VMs and the controller). An environment with no Linux machine and no
/// controller has no runner here.
fn runners(env: &Environment, guests: &[Guest]) -> Result<Vec<CheckRunner>, ManifestError> {
    manifest::check_runners(env, |pos, machine, group| {
        let Some(guest) = guests.iter().find(|g| g.machine == machine) else {
            return Ok(None);
        };
        let script = format!("{OUTPUT_DIR}/{}/checks/{}.sh", Target::CloudVm.id(), pos.id());
        steps::check_step(pos.id(), guest, &script, timeout_s(group)).map(Some)
    })
}

/// The launch's values (inputs), each one slot: an entry of the module's `inputs` for the
/// machines reading it in their environment, delivered over SSH to those reading it as a file;
/// then each secret a machine reads.
fn inputs(env: &Environment) -> Result<Vec<SecretSlot>, ManifestError> {
    let spec = env.spec();
    let mut out = Vec::new();
    for input in &spec.inputs {
        let env_readers = manifest::input_readers(env, input)?;
        let file_readers: Vec<Name> = vms(env)
            .filter(|(_, mp)| mp.reads_input_file(input))
            .map(|(n, _)| Name::parse(n))
            .collect::<Result<_, _>>()?;
        let sensitive = !spec.plain_inputs.contains(input);
        out.extend(connection::input(input, sensitive, &env_readers, &file_readers)?);
    }
    out.extend(machine_secrets(env)?);
    Ok(out)
}

/// Each secret a machine reads (`secrets:`), from its store: delivered over SSH, so the value
/// stays out of the state. Owners are the machine's set-up's business (its `Secrets` step places
/// the file). An input's file is the input's own slot.
fn machine_secrets(env: &Environment) -> Result<Vec<SecretSlot>, ManifestError> {
    let spec = env.spec();
    let mut out = Vec::new();
    for (name, source) in &spec.secrets {
        let Some(source) = isoloom_render::run_manifest::secret_source(source) else {
            continue;
        };
        let readers: Vec<Name> = vms(env)
            .filter(|(_, mp)| mp.secrets.iter().any(|s| &s.name == name))
            .map(|(n, _)| Name::parse(n))
            .collect::<Result<_, _>>()?;
        if readers.is_empty() {
            continue;
        }
        out.push(connection::guest_secret(name, source, &readers)?);
    }
    Ok(out)
}
