//! The `docker-vm` target's run manifest on a VM Vagrant makes (`.isoloom/docker-vm/run.json`):
//! Vagrant brings the VM up with Docker and the project in it (every provisioner but the lab's
//! start), then the lab's containers start in it through `vagrant ssh`, as Compose's backend says
//! ([`GuestCompose`]). The VM forwards each published port from this computer's loopback at its
//! fixed number, so a start first checks none is taken (`vagrant.check_forwarded_ports`). Parking
//! suspends the VM with everything in it; checks and the containers' states are read inside it.

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use isoloom_manifest::{
    Action, Argv, Checks, Condition, Driver, Error as ManifestError, Guest, HostArgv, HostOs, Lifecycle, Machine, Manifest, ManifestVersion, Name, Native,
    NonEmpty, Park, PathTemplate, Provider, RelPath, Resources, Role, Run, SCHEMA_URL, Step, StepId, Template,
};
use isoloom_plan::Environment;
use isoloom_render::run_manifest::guest::{GuestCommand, GuestCompose};
use isoloom_render::run_manifest::{self, act, noted, outputs_on_their_hosts, run};

use super::lifecycle::{back_up, down, recovery};
use super::{DOTFILE, on_provider, providers, slots, vagrant_argv};
use crate::docker_vm::{DEFINE, VM, host_size, prepares};
use crate::error::Result;
use crate::vms::{label, vm_name};

/// Where forwarded ports listen on this computer.
const LOOPBACK: &str = "127.0.0.1";

/// How long the VM may take to come up with Docker in it.
const UP_TIMEOUT_S: u32 = 3600;

/// How long reading the VM's state may take on ESXi, which asks the host (one VM).
const ESXI_STATUS_S: u32 = 90;

/// The `docker-vm` manifest of `env` on Vagrant, Compose running in the VM as `guest` says; its
/// message is left empty.
pub(crate) fn manifest(env: &Environment, guest: &GuestCompose) -> Result<Manifest> {
    let way_in = |command: &Template| vagrant_ssh(command);
    let mut machines = vec![vm(env)?];
    machines.extend(guest.machines_at(&Template::text(LOOPBACK)));
    let runners = guest.runners.iter().map(|r| r.runner(&way_in)).collect::<Result<Vec<_>, _>>()?;
    let mut params = slots::remote_params()?;
    params.extend(guest.params.iter().cloned());
    let mut secrets = guest.slots.clone();
    secrets.push(slots::esxi_password()?);
    let mut ready = vec![Condition::StepOk {
        step: StepId::parse(UP)?,
        means: Some("the VM booted with Docker and the project in it".into()),
    }];
    ready.extend(guest.ready.iter().cloned());
    Ok(Manifest {
        schema: Some(SCHEMA_URL.to_owned()),
        manifest_version: ManifestVersion,
        generator: run_manifest::generator()?,
        environment: run_manifest::environment(env)?,
        target: isoloom_manifest::Target::DockerVm,
        cloud: None,
        driver: Driver::Vagrant,
        hosts: BTreeSet::from([HostOs::Linux, HostOs::Macos, HostOs::Windows]),
        tools: providers::tools()?,
        identity: providers::docker_vm_identity(&PathTemplate::in_state_dir(&RelPath::parse(DOTFILE)?))?,
        params,
        secrets,
        networks: guest.networks.clone(),
        outputs: outputs_on_their_hosts(&machines)?,
        machines,
        lifecycle: lifecycle(env, guest, &way_in)?,
        ready,
        checks: Checks {
            protocol: isoloom_manifest::CheckProtocol::V1,
            setup: steps(&guest.check_setup, &way_in)?,
            runners,
            teardown: Vec::new(),
        },
        recovery: Some(recovery()?),
        expiry: None,
        guest: Some(Guest {
            machine: Name::parse(VM)?,
            host: None,
            status: guest.status.step(&way_in)?,
        }),
    })
}

/// The VM up step.
const UP: &str = "up";

/// `command` run in the VM by `vagrant ssh --no-tty --command <command> default`. Vagrant runs it
/// as `bash -l -c '<command>'` without escaping, so each single quote of the command's text is
/// closed, escaped and reopened (`'\''`) first.
fn vagrant_ssh(command: &Template) -> Result<Run, ManifestError> {
    let escaped = Template::from_parts(command.parts().iter().map(|p| match p {
        isoloom_manifest::Part::Text(t) => isoloom_manifest::Part::Text(t.replace('\'', r"'\''")),
        isoloom_manifest::Part::Placeholder(_) => p.clone(),
    }));
    let args = vec![
        Template::text("ssh"),
        Template::text("--no-tty"),
        Template::text("--command"),
        escaped,
        Template::text(DEFINE),
    ];
    Ok(Run::Argv(HostArgv::Every(Argv::new(run_manifest::program("vagrant")?, args))))
}

/// `commands` as steps through `way_in`.
fn steps(commands: &[GuestCommand], way_in: &dyn Fn(&Template) -> Result<Run, ManifestError>) -> Result<Vec<Step>, ManifestError> {
    commands.iter().map(|c| c.step(way_in)).collect()
}

/// The VM: Isoloom's own (`infra`), named as each provider names it.
fn vm(env: &Environment) -> Result<Machine> {
    let (cpus, memory_mb) = host_size(env);
    let vm_names = super::machines::vm_names(label(env, "docker"), vm_name(env, "docker"), format!("{}_{DEFINE}", env.name));
    let mut vm = run_manifest::machine(VM, Role::Infra)?;
    vm.resources = Some(Resources {
        cpus: Some(cpus),
        memory_mb: Some(memory_mb),
    });
    vm.native = Some(Native::Vagrant {
        vagrant: Name::parse(DEFINE)?,
        vm_names,
    });
    Ok(vm)
}

/// Up (the VM, then the lab in it), down, park (suspend only: the containers keep their memory),
/// resume and status.
fn lifecycle(env: &Environment, guest: &GuestCompose, way_in: &dyn Fn(&Template) -> Result<Run, ManifestError>) -> Result<Lifecycle> {
    let mut discard = run_manifest::tolerated(act("discard-saved", Action::VagrantDiscardAbortedSaved)?);
    discard.providers.insert(Provider::Virtualbox);
    // ESXi forwards nothing: the ports answer at the VM's own address.
    let mut ports = act("forwarded-ports", Action::VagrantCheckForwardedPorts)?;
    ports.providers = BTreeSet::from([
        Provider::Virtualbox,
        Provider::VmwareDesktop,
        Provider::Parallels,
        Provider::Utm,
        Provider::Qemu,
        Provider::Libvirt,
    ]);
    let mut up = vec![
        noted(
            act("reconcile", Action::VagrantReconcileProvider)?,
            "Clearing the state of a run on another provider, if any…",
        ),
        discard,
        ports,
        up_step(env, guest)?,
    ];
    up.extend(steps(&guest.start, way_in)?);
    Ok(Lifecycle {
        up: NonEmpty::new(up, "lifecycle.up")?,
        down: NonEmpty::new(down()?, "lifecycle.down")?,
        park: Some(Park {
            pause: Some(vec![run("suspend", vagrant_argv(&["suspend"])?, 1800)?]),
            shutdown: None,
        }),
        resume: back_up(&[])?,
        provision: None,
        // internet:false is sealed inside the VM by the containers' route sidecars (the docker
        // target's), as on this machine: no network-level seal step.
        seal: None,
        status: status()?,
    })
}

/// `vagrant up --provider ${run.provider} --provision-with <the VM's own>`: the VM with Docker and
/// the project, not the lab's start, which the next steps run.
fn up_step(env: &Environment, guest: &GuestCompose) -> Result<Step> {
    let with = prepares(env, guest).join(",");
    let mut s = run(UP, on_provider(&["up"], &["--provision-with", &with])?, UP_TIMEOUT_S)?;
    s.note = Some("Building the lab's VM with Docker in it…".into());
    Ok(s)
}

/// `vagrant status --machine-readable`, with more time on ESXi, which asks the host.
fn status() -> Result<Step> {
    let mut s = run("status", vagrant_argv(&["status", "--machine-readable"])?, 25)?;
    s.provider_timeout_s
        .insert(Provider::VmwareEsxi, NonZeroU32::new(ESXI_STATUS_S).unwrap_or(NonZeroU32::MIN));
    Ok(s)
}

#[cfg(test)]
mod tests;
