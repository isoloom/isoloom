//! The `docker-vm` target's run manifest on a VM Vagrant makes (`.isoloom/docker-vm/run.json`):
//! Vagrant brings the VM up with Docker and the project in it (every provisioner but the environment's
//! start), then the environment's containers start in it through `vagrant ssh`, as Compose's backend says
//! ([`GuestCompose`]). The VM forwards each published port from this computer's loopback at its
//! fixed number, so a start first checks none is taken (`vagrant.check_forwarded_ports`). Parking
//! suspends the VM with everything in it; checks and the containers' states are read inside it.

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use isoloom_manifest::{
    Action, Argv, Driver, Error as ManifestError, HostArgv, Lifecycle, Manifest, Name, Native, NonEmpty, Park, PathTemplate, Provider, PublishMode, RelPath,
    Run, Step, Template,
};
use isoloom_plan::Environment;
use isoloom_render::container::DOCKER_HOST as VM;
use isoloom_render::run_manifest::guest::{GuestCommand, GuestCompose};
use isoloom_render::run_manifest::{self, act, noted, outputs_on_their_hosts, run};

use super::lifecycle::{back_up, down, recovery};
use super::{DOTFILE, on_provider, providers, slots, vagrant_argv};
use crate::docker_vm::{DEFINE, prepares};
use crate::error::{Error, Result};
use crate::vms::{label, vm_name};

/// Where forwarded ports listen on this computer.
const LOOPBACK: &str = "127.0.0.1";

/// How long the VM may take to come up with Docker in it.
const UP_TIMEOUT_S: u32 = 3600;

/// How long reading the VM's state may take on ESXi, which asks the host (one VM).
const ESXI_STATUS_S: u32 = 90;

/// The containers' machines of `env`, each published port answering on this computer's loopback:
/// the VM forwards it from the host port the runner picks for it (the Vagrantfile's
/// `ISOLOOM_HOSTPORTS`, by `<machine>/<port>`), as the `vagrant` target's VMs do. Each port takes
/// the mode the plan gave it (`fixed` only when the spec fixed it), so a preferred one that another
/// environment's VM or any program holds moves to a free port instead of refusing the start.
/// Compose in the VM publishes every port at its own number, fixed (`guest`'s ports), and
/// `machines_at` fixes them all, for a VM whose own address serves them (Proxmox, the clouds).
fn forwarded_here(env: &Environment, guest: &GuestCompose) -> Vec<isoloom_manifest::Machine> {
    let mut machines = guest.machines.clone();
    for m in &mut machines {
        let planned = env.machines.get(m.name.as_str()).map(|mp| mp.ports.as_slice()).unwrap_or_default();
        for p in &mut m.ports {
            let fixed = planned.iter().find(|x| x.name == p.name.as_str()).is_none_or(|x| x.fixed);
            if let Some(publish) = &mut p.publish {
                publish.mode = if fixed { PublishMode::Fixed } else { PublishMode::Ephemeral };
                publish.address = Some(Template::text(LOOPBACK));
            }
        }
    }
    machines
}

/// The `docker-vm` manifest of `env` on Vagrant, Compose running in the VM as `guest` says; its
/// message is left empty.
pub(crate) fn manifest(env: &Environment, guest: &GuestCompose) -> Result<Manifest> {
    let way_in = |command: &Template| vagrant_ssh(command);
    let mut host = guest.docker_host(env, (UP, "the VM booted with Docker and the project in it"), None, &way_in)?;
    host.machine.native = Some(native(env, guest)?);
    let mut machines = vec![host.machine];
    let forwarded = forwarded_here(env, guest);
    let mut params = slots::remote_params()?;
    // Set, a published port keeps its own `publish` number when it is free, as on the `vagrant`
    // target; unset, the runner picks a free one.
    params.extend(run_manifest::publish_fixed(&forwarded)?);
    machines.extend(forwarded);
    params.extend(guest.params.iter().cloned());
    let mut secrets = guest.slots.clone();
    secrets.push(slots::esxi_password()?);
    Ok(run_manifest::Body {
        // Its one VM is on no private network under QEMU: no cloud-init seed, no `mkisofs`.
        tools: providers::tools(false)?,
        identity: providers::docker_vm_identity(env, &PathTemplate::in_state_dir(&RelPath::parse(DOTFILE)?))?,
        params,
        secrets,
        networks: guest.networks.clone(),
        outputs: outputs_on_their_hosts(&machines)?,
        machines,
        lifecycle: lifecycle(env, guest, &way_in)?,
        ready: host.ready,
        checks: guest.checks(&way_in)?,
        recovery: Some(recovery()?),
        expiry: None,
        guest: Some(host.guest),
    }
    .manifest(env, isoloom_manifest::Target::DockerVm, None, Driver::Vagrant)?)
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

/// The VM as Vagrant has it: its define, named as each provider it is offered on names it, with
/// the digest of what Vagrant applies to it only at boot (its Vagrantfile's, `guest` running in
/// it).
fn native(env: &Environment, guest: &GuestCompose) -> Result<Native> {
    let mut vm_names = super::machines::vm_names(label(env, VM), vm_name(env, VM), format!("{}_{DEFINE}", env.name));
    vm_names.retain(|p, _| providers::docker_vm_offers(*p));
    let boot_digest = crate::boot_digest::of(&crate::docker_vm::boot(env, guest)?)
        .remove(DEFINE)
        .ok_or_else(|| Error::NoBootDigest {
            target: isoloom_spec::model::Target::DockerVm,
            machine: DEFINE.to_owned(),
        })?;
    let debian = crate::boxes::debian_12();
    let choice = crate::docker_vm::host_boxes(&debian);
    let offered = crate::vms::provider::Provider::ALL
        .iter()
        .map(|k| k.manifest)
        .filter(|p| providers::docker_vm_offers(*p));
    let boxes = offered.flat_map(|p| choice.on(p).into_iter().map(move |b| (p, b))).collect();
    Ok(Native::Vagrant {
        vagrant: Name::parse(DEFINE)?,
        vm_names,
        boot_digest: Some(boot_digest),
        boxes: super::listed(boxes)?,
    })
}

/// Up (the VM, then the environment in it), down, park (suspend only: the containers keep their memory),
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
    up.extend(GuestCommand::steps(&guest.start, way_in)?);
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
/// the project, not the environment's start, which the next steps run.
fn up_step(env: &Environment, guest: &GuestCompose) -> Result<Step> {
    let with = prepares(env, guest)?.join(",");
    let mut s = run(UP, on_provider(&["up"], &["--provision-with", &with])?, UP_TIMEOUT_S)?;
    s.note = Some("Building the environment's VM with Docker in it…".into());
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
