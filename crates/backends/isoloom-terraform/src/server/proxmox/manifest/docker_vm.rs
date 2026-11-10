//! The `docker-vm` target's run manifest on a Proxmox server (`.isoloom/docker-vm/proxmox/run.json`):
//! Terraform makes the Docker host VM (`init`, `apply`, as for the `proxmox` target), then every
//! step inside it runs over SSH with the runner's key (`ssh.run` to `${outputs.ip}` as `isoloom`):
//! `setup.sh` (Docker) and the project's archive ([`into_vm`]), then the Compose file started as
//! Compose's backend says ([`GuestCompose`]), the values given at launch carried into the session. The containers' published ports answer at the VM's address; checks
//! and the containers' states are read inside it. A server lab isn't parked: it is made or
//! destroyed.

use isoloom_manifest::{Driver, Error as ManifestError, Lifecycle, Manifest, ManifestVersion, Placeholder, RelPath, Run, SCHEMA_URL, SnakeName, Template};
use isoloom_plan::Environment;
use isoloom_render::run_manifest::guest::{GuestCommand, GuestCompose};

use crate::docker_host::{docker_host_outputs, into_vm, ssh_run};
use isoloom_render::run_manifest::{self, environment, generator};
use isoloom_spec::model::Target;

use super::super::docker_host::{SETUP, USER, variables};
use super::{slots, steps, tools};
use crate::Error;

/// The VM's address on the uplink: where SSH reaches it and the published ports answer.
const IP: &str = "ip";

/// The `docker-vm` manifest of `env` on Proxmox, Compose running in the VM as `guest` says (the
/// Compose backend's); its message is left to the caller.
///
/// # Errors
///
/// [`Error::Manifest`] for a value the manifest's types refuse (a bug of this backend).
pub fn docker_vm(env: &Environment, guest: &GuestCompose) -> crate::Result<Manifest> {
    build(env, guest).map_err(|source| Error::Manifest {
        target: Target::DockerVm,
        source,
    })
}

fn build(env: &Environment, guest: &GuestCompose) -> Result<Manifest, ManifestError> {
    let ip = Template::from(Placeholder::Output(SnakeName::parse(IP)?));
    let way_in = ssh_run(ip.clone(), USER);
    let host = guest.docker_host(env, ("apply", "the Docker host VM exists and booted"), Some(ip.clone()), &way_in)?;
    let mut machines = vec![host.machine];
    machines.extend(guest.machines_at(&ip));
    let vars = variables();
    // docker-vm is a single Docker host VM with no router to seal.
    let mut params = slots::params(&vars, &[], false)?;
    params.extend(guest.params.iter().cloned());
    let mut secrets = slots::connection(&vars)?;
    secrets.extend(guest.slots.iter().cloned());
    let lifecycle = lifecycle(guest, &way_in)?;
    Ok(Manifest {
        schema: Some(SCHEMA_URL.to_owned()),
        manifest_version: ManifestVersion,
        generator: generator()?,
        environment: environment(env)?,
        target: isoloom_manifest::Target::DockerVm,
        cloud: None,
        driver: Driver::Terraform,
        hosts: run_manifest::every_host(),
        tools: tools()?,
        identity: super::runner_identity()?,
        params,
        secrets,
        networks: guest.networks.clone(),
        outputs: docker_host_outputs(&machines)?,
        machines,
        lifecycle,
        ready: host.ready,
        checks: guest.checks(&way_in)?,
        recovery: None,
        expiry: None,
        guest: Some(host.guest),
    })
}

/// Terraform makes the VM (`init`, `apply`), then it is readied over SSH ([`into_vm`]) and Compose
/// starts in it (`guest.start`).
fn lifecycle(guest: &GuestCompose, way_in: &dyn Fn(&Template) -> Result<Run, ManifestError>) -> Result<Lifecycle, ManifestError> {
    // docker-vm is a single Docker host VM with no router to seal; the internet seal is the
    // Compose layer's (the guest), not a second apply here.
    let mut lifecycle = steps::lifecycle(false)?;
    let mut up = lifecycle.up.as_slice().to_vec();
    up.extend(into_vm(way_in, &RelPath::parse(SETUP)?, "../../..")?);
    up.extend(GuestCommand::steps(&guest.start, way_in)?);
    lifecycle.up = isoloom_manifest::NonEmpty::new(up, "lifecycle.up")?;
    Ok(lifecycle)
}

#[cfg(test)]
mod tests;
