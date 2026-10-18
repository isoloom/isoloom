//! The `docker-vm` target's run manifest on a Proxmox server (`.isoloom/docker-vm/proxmox/run.json`):
//! Terraform makes the Docker host VM (`init`, `apply`, as for the `proxmox` target), then every
//! step inside it runs over SSH with the runner's key (`ssh.run` to `${outputs.ip}` as `isoloom`):
//! `setup.sh` (Docker) and the project's archive ([`into_vm`]), then the Compose file started as
//! Compose's backend says ([`GuestCompose`]), the values given at launch carried into the session. The containers' published ports answer at the VM's address; checks
//! and the containers' states are read inside it. A server lab isn't parked: it is made or
//! destroyed.

use isoloom_manifest::{
    Checks, Condition, Driver, Error as ManifestError, Guest, Lifecycle, Machine, Manifest, ManifestVersion, Name, Output, OutputRole, Placeholder, RelPath,
    Resources, Role, Run, SCHEMA_URL, SnakeName, Step, StepId, Template,
};
use isoloom_plan::Environment;
use isoloom_render::container::HostSize;
use isoloom_render::run_manifest::guest::{GuestCommand, GuestCompose, into_vm, ssh_run};
use isoloom_render::run_manifest::{self, environment, generator, outputs_on_their_hosts};
use isoloom_spec::model::Target;

use super::super::docker_host::{SETUP, USER, variables};
use super::{slots, steps, tools};
use crate::Error;

/// The VM, as the manifest names it.
const VM: &str = "docker";
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
    let mut machines = vec![vm(env)?];
    machines.extend(guest.machines_at(&ip));
    let vars = variables();
    // docker-vm is a single Docker host VM with no router to seal.
    let mut params = slots::params(&vars, &[], false)?;
    params.extend(guest.params.iter().cloned());
    let mut secrets = slots::connection(&vars)?;
    secrets.extend(guest.slots.iter().cloned());
    let lifecycle = lifecycle(guest, &way_in)?;
    let mut ready = vec![Condition::StepOk {
        step: StepId::parse("apply")?,
        means: Some("the Docker host VM exists and booted".into()),
    }];
    ready.extend(guest.ready.iter().cloned());
    Ok(Manifest {
        schema: Some(SCHEMA_URL.to_owned()),
        manifest_version: ManifestVersion,
        generator: generator()?,
        environment: environment(env)?,
        target: isoloom_manifest::Target::DockerVm,
        cloud: None,
        driver: Driver::Terraform,
        hosts: super::every_host(),
        tools: tools()?,
        identity: super::runner_identity()?,
        params,
        secrets,
        networks: guest.networks.clone(),
        outputs: outputs(&machines)?,
        machines,
        lifecycle,
        ready,
        checks: Checks {
            protocol: isoloom_manifest::CheckProtocol::V1,
            setup: steps_of(&guest.check_setup, &way_in)?,
            runners: guest.runners.iter().map(|r| r.runner(&way_in)).collect::<Result<_, _>>()?,
            teardown: Vec::new(),
        },
        recovery: None,
        expiry: None,
        guest: Some(Guest {
            machine: Name::parse(VM)?,
            host: Some(ip),
            status: guest.status.step(&way_in)?,
        }),
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
    up.extend(steps_of(&guest.start, way_in)?);
    lifecycle.up = isoloom_manifest::NonEmpty::new(up, "lifecycle.up")?;
    Ok(lifecycle)
}

/// The Docker host's address (`ip`), the SSH user, then each published port at the host's address.
fn outputs(machines: &[Machine]) -> Result<Vec<Output>, ManifestError> {
    let mut out = vec![
        run_manifest::terraform_output(
            IP,
            OutputRole::Address,
            "The Docker host's address: SSH reaches it there, and the published ports answer there.",
        )?,
        run_manifest::terraform_output("ssh_user", OutputRole::Info, "The user the runner's key lets in on the Docker host.")?,
    ];
    out.extend(outputs_on_their_hosts(machines)?);
    Ok(out)
}

/// `commands` as steps through `way_in`.
fn steps_of(commands: &[GuestCommand], way_in: &dyn Fn(&Template) -> Result<Run, ManifestError>) -> Result<Vec<Step>, ManifestError> {
    commands.iter().map(|c| c.step(way_in)).collect()
}

/// The VM: Isoloom's own (`infra`), sized for the containers.
fn vm(env: &Environment) -> Result<Machine, ManifestError> {
    let size = HostSize::of(env);
    let mut vm = run_manifest::machine(VM, Role::Infra)?;
    vm.resources = Some(Resources {
        cpus: Some(size.cpus),
        memory_mb: Some(size.memory_mb),
    });
    Ok(vm)
}

#[cfg(test)]
mod tests;
