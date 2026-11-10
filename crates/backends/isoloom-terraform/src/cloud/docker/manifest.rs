//! The `cloud-docker` run manifest (`.isoloom/cloud-docker/<cloud>/run.json`): Terraform makes
//! one VM, and the runner runs the environment's containers inside it, as `docker-vm`
//! does on a server. The containers and the commands that start, check and read them are the
//! shared Compose-in-a-VM pieces (`isoloom_render::run_manifest::guest`); this module adds the
//! Terraform lifecycle, the cloud's credentials and the VM.
//!
//! Terraform keeps only the infrastructure: no SSH from it, no project in the module. Once the
//! apply is done, the runner copies the project into the VM and installs Docker over SSH
//! (`into_vm`), then starts the Compose file there, the launch's values and the containers'
//! secrets carried in over SSH (`SendEnv`). The published ports answer at the VM's address, and
//! the runner reads the containers inside it (`Manifest.guest`).

use isoloom_manifest::{Error as ManifestError, Manifest, Placeholder, RelPath, SnakeName, Step, Template};
use isoloom_plan::Environment;
use isoloom_render::run_manifest::guest::GuestCompose;

use crate::docker_host::{accept_env, docker_host_outputs, into_vm};
use crate::ssh::Login;

use super::super::Cloud;
use super::super::manifest::{self, connection, steps};

/// The host's address (`${outputs.ip}`): where SSH reaches it and its containers' ports answer.
const IP: &str = "ip";

/// The way into `driver`'s host: its address output ([`IP`]), as the user its image signs in as.
pub(crate) fn login(driver: &super::Driver) -> Result<Login, ManifestError> {
    Ok(Login::new(Template::from(Placeholder::Output(SnakeName::parse(IP)?)), driver.user()))
}

/// How long the runner retries the first SSH while the fresh VM is still booting, in seconds.
const SSH_BOOT_WAIT_S: u32 = 300;

/// The run manifest of `env`'s containers on `cloud`, run in one VM whose module `driver` writes,
/// entered as `login` (the host's address and user); `guest` is the Compose-in-a-VM plan
/// (`isoloom_compose::guest`) as that user gets root. Its message is left to the caller.
pub(crate) fn manifest(env: &Environment, cloud: Cloud, driver: &super::Driver, login: &Login, guest: &GuestCompose) -> Result<Manifest, ManifestError> {
    let ip = login.host.clone();
    let way_in = login.way_in();
    let up = up_steps(login, guest)?;
    let guest_machines = guest.machines_at(&ip);
    let host = guest.docker_host(env, ("apply", "the Docker host exists and booted"), Some(ip.clone()), &way_in)?;
    let mut machines = vec![host.machine];
    machines.extend(guest_machines.iter().cloned());
    let mut params = connection::params(&driver_variables(env, driver), cloud)?;
    params.extend(guest.params.iter().cloned());
    let mut secrets = connection::slots(cloud, connection::terraform_steps(false))?;
    secrets.extend(guest.slots.iter().cloned());
    let outputs = docker_host_outputs(&guest_machines)?;
    isoloom_render::run_manifest::Body {
        tools: manifest::tools()?,
        identity: crate::module::manifest::runner_identity(Vec::new())?,
        params,
        secrets,
        // The lab's networks as Compose names them in the VM, as docker-vm lists them: what the
        // runner reads and draws.
        networks: guest.networks.clone(),
        machines,
        outputs,
        lifecycle: steps::terraform_lifecycle(up)?,
        ready: host.ready,
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

/// The `up` steps: Terraform's lock, init and apply, then the runner installs Docker and copies
/// the project into the VM over SSH (`into_vm`) and starts the Compose file there.
fn up_steps(login: &Login, guest: &GuestCompose) -> Result<Vec<Step>, ManifestError> {
    let way_in = login.way_in();
    let mut up = vec![
        steps::clear_lock()?,
        steps::init_step()?,
        steps::apply_step("apply", "Creating the Docker host in the cloud…")?,
    ];
    let mut into = into_vm(login, &RelPath::parse("setup.sh")?, "../../..")?;
    boot_wait(&mut into)?;
    up.extend(into);
    // The VM is readied bare over SSH (no cloud-init `AcceptEnv`, unlike docker-vm on Proxmox), so
    // its sshd must be told to accept the launch's values the runner forwards (`SendEnv`) before
    // the start steps run; otherwise the containers' env vars arrive unset.
    if let Some(accept) = accept_env(guest, login.as_root())? {
        up.push(accept.step(&way_in)?);
    }
    for command in &guest.start {
        up.push(command.step(&way_in)?);
    }
    Ok(up)
}

/// The first SSH step into the fresh VM retries while ssh can't connect yet (the VM is still
/// booting): `with.wait_s`. Later steps run once it answered.
fn boot_wait(steps: &mut [isoloom_manifest::Step]) -> Result<(), ManifestError> {
    if let Some(first) = steps.first_mut()
        && let isoloom_manifest::Run::Action { with, .. } = &mut first.run
    {
        with.insert(SnakeName::parse("wait_s")?, Template::text(&SSH_BOOT_WAIT_S.to_string()));
    }
    Ok(())
}

/// The module's variables, for the params: the cloud's own and its size, then the operator's
/// (who may reach the host, the runner's key, the expiry). The launch's values reach the
/// containers over SSH, not a module variable, so `inputs` is left out; a cloud-docker host has
/// internet, so there is no seal.
fn driver_variables(env: &Environment, driver: &super::Driver) -> Vec<crate::module::variable::Variable> {
    let mut v = driver.variables(env);
    v.extend(super::super::operator::host_reaching());
    v
}
