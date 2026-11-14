//! The `cloud-docker` run manifest (`.isoloom/cloud-docker/<cloud>/run.json`): Terraform makes
//! one VM, and the runner runs the environment's containers inside it. The containers and the commands that start, check and read them are the
//! shared Compose-in-a-VM pieces (`isoloom_render::run_manifest::guest`); this module adds the
//! Terraform lifecycle, the cloud's credentials and the VM.
//!
//! The apply does it all: the module brings the project in, the host installs Docker and starts
//! the Compose file from its user data, the launch's values and the containers' secrets in the
//! module's variables, and the module waits for it. The published ports answer at the VM's
//! address, and the runner reads the containers inside it (`Manifest.guest`).

use isoloom_manifest::{Delivery, Error as ManifestError, Manifest, NonEmpty, Placeholder, SecretSlot, SnakeName, Step, Template};
use isoloom_plan::Environment;
use isoloom_render::run_manifest::guest::GuestCompose;

use crate::docker_host::docker_host_outputs;
use crate::ssh::Login;

use super::super::Cloud;
use super::super::manifest::{self, connection, steps};

/// The host's address (`${outputs.ip}`): where SSH reaches it and its containers' ports answer.
const IP: &str = "ip";

/// The way into `driver`'s host: its address output ([`IP`]), as the user its image signs in as.
pub(crate) fn login(driver: &super::DockerDriver) -> Result<Login, ManifestError> {
    Ok(Login::new(Template::from(Placeholder::Output(SnakeName::parse(IP)?)), driver.user()))
}

/// The run manifest of `env`'s containers on `cloud`, run in one VM whose module `driver` writes,
/// entered as `login` (the host's address and user); `guest` is the Compose-in-a-VM plan
/// (`isoloom_compose::guest`) as that user gets root. Its message is left to the caller.
pub(crate) fn manifest(env: &Environment, cloud: Cloud, driver: &super::DockerDriver, login: &Login, guest: &GuestCompose) -> Result<Manifest, ManifestError> {
    let ip = login.host.clone();
    let way_in = login.way_in();
    let up = up_steps()?;
    let guest_machines = guest.machines_at(&ip);
    let host = guest.docker_host(env, ("apply", "the Docker host exists and booted"), Some(ip.clone()), &way_in)?;
    let mut machines = vec![host.machine];
    machines.extend(guest_machines.iter().cloned());
    let mut params = connection::params(&driver_variables(env, driver), cloud)?;
    params.extend(guest.params.iter().cloned());
    let mut secrets = connection::slots(cloud, connection::TERRAFORM_STEPS)?;
    for slot in &guest.slots {
        secrets.push(on_terraform(slot.clone())?);
    }
    let outputs = docker_host_outputs(&guest_machines)?;
    isoloom_render::run_manifest::Body {
        tools: manifest::tools()?,
        identity: crate::module::manifest::runner_identity(Vec::new())?,
        params,
        secrets,
        // The environment's networks as Compose names them in the VM: what the
        // runner reads and draws.
        networks: guest.networks.clone(),
        machines,
        outputs,
        lifecycle: steps::terraform_lifecycle(up)?,
        // The apply ends once the containers are up (the module waits for the host's set-up): the
        // conditions on start steps it no longer has are left out.
        ready: host
            .ready
            .into_iter()
            .filter(|c| !matches!(c, isoloom_manifest::Condition::StepOk { step, .. } if step.as_str() != "apply"))
            .collect(),
        checks: guest.checks(&way_in)?,
        recovery: manifest::recovery()?,
        expiry: manifest::expiry()?,
        guest: Some(host.guest),
    }
    .manifest(
        env,
        isoloom_manifest::Target::CloudDocker,
        Some(manifest::cloud_of(cloud)),
        isoloom_manifest::Driver::Terraform,
    )
}

/// The `up` steps: Terraform's lock, init and apply, which ends once the host has set itself up
/// and started the Compose file (its user data, waited for by the module).
fn up_steps() -> Result<Vec<Step>, ManifestError> {
    Ok(vec![
        steps::clear_lock()?,
        steps::init_step()?,
        steps::apply_step("apply", "Creating the Docker host in the cloud and starting the containers…")?,
    ])
}

/// `slot`, a value the containers read, given to the module's variable of its name instead
/// (`TF_VAR_<name>` on the Terraform steps): the host's user data starts the containers with it.
fn on_terraform(mut slot: SecretSlot) -> Result<SecretSlot, ManifestError> {
    let vars: Vec<String> = slot
        .deliver
        .iter()
        .filter_map(|d| match d {
            Delivery::Env { var, .. } => Some(var.as_str().to_owned()),
            _ => None,
        })
        .collect();
    let mut deliver = Vec::new();
    for var in vars {
        deliver.extend(connection::on_terraform(&format!("TF_VAR_{var}"), connection::TERRAFORM_STEPS)?.iter().cloned());
    }
    slot.deliver = NonEmpty::new(deliver, "the module's variables")?;
    Ok(slot)
}

/// The module's variables, for the params: the cloud's own and its size, then the operator's
/// (who may reach the host, the runner's key, the expiry). The launch's values are slots of their
/// own; a cloud-docker host has internet, so there is no seal.
fn driver_variables(env: &Environment, driver: &super::DockerDriver) -> Vec<crate::module::variable::Variable> {
    let mut v = driver.variables(env);
    v.extend(super::super::operator::host_reaching());
    v
}
