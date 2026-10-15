//! The `hybrid` target: containers and VMs together on the same networks, on this machine, in
//! `.isoloom/hybrid/` (a Vagrantfile and a Compose file).
//!
//! - A machine with `docker:` is a container (the light option); a machine with only `vm:` is a
//!   VM. The VMs, the router and the controller are what the `vagrant` output makes of them.
//! - The containers run on one more VM, `isoloom-docker`, plugged into the networks they're on
//!   with promiscuous network interfaces: Docker attaches each container to its network directly
//!   (`macvlan`), at its own address, so VMs and containers reach each other as on one network
//!   (`host`). Their Compose file is the Compose backend's, plugged into those networks
//!   (`isoloom_compose::hybrid`); `isoloom-targets` renders it and hands it to [`render`].
//! - Published ports: macvlan can't publish, so a container with published ports also joins
//!   Docker's bridge network (its default route stays on its lab network), and the Docker host
//!   VM forwards each port to this machine's loopback.
//! - VirtualBox only for now (promiscuous network interfaces are set per provider).
//! - Refused, from the plan's requirements: gateways; a shared volume mounted by containers and
//!   VMs; a VM that depends on a container (VMs start first); network appliances. Containers
//!   don't reach the internet while running (the Docker host pulls their images).

mod host;

use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::backend::Files;
use isoloom_render::container::HybridContainers;
use isoloom_spec::model::{Spec, Target};

use crate::error::Result;
use crate::vms::{self, Layout};

const DIR: &str = "hybrid";

/// What the hybrid target can render. The VM side's controller runs provisioning and every
/// kind of check. Gateways, volumes shared across containers and VMs, VMs waiting for containers
/// and network appliances aren't wired yet.
#[must_use]
pub fn capabilities() -> Capabilities {
    Capabilities::of([
        Feature::EnvironmentProvisioning,
        Feature::PlaybookChecks,
        Feature::ExecWithoutContainer,
        Feature::MachineAliases,
        Feature::Ulimits,
        Feature::TrafficControl,
        Feature::Tpm,
        // Delivered by `isoloom run` over the machine's SSH, owners included.
        Feature::Secrets,
        Feature::SecretOwners,
        // Read into Compose's environment by the containers' host before `up`.
        Feature::OwnedProjectFiles,
        Feature::MachineProcess,
        Feature::NestedVirtualization,
        // Docker's DNS for the containers, each VM's hosts file for the VMs.
        Feature::PrivateDns,
        // A VM's set-up adds them; a container has one address per network (refused).
        Feature::VmMoreAddresses,
        Feature::DataDisks,
        Feature::TmpfsModes,
    ])
}

/// The Vagrantfile (the VMs, then the containers' host) and the checks next to it, with the
/// containers' Compose file (`containers`) second. The environment was checked against
/// [`capabilities`] first.
///
/// # Errors
///
/// [`Error`](crate::Error) when the VMs need what Vagrant can't build (a machine with no box,
/// a step it can't run), or a network the containers are on isn't a block of addresses; each
/// reported as the `hybrid` target's.
pub fn render(env: &Environment, containers: HybridContainers) -> Result<Files> {
    let host = host::block(env, &containers)?;
    let layout = Layout { dir: DIR, extra: &host };
    let mut files = vms::render_spec_in(&vms_only(env.spec()), &layout).map_err(|e| e.on(Target::Hybrid))?;
    files.insert(1.min(files.len()), containers.file);
    Ok(files)
}

/// The spec with the containers' VM forms left out: the VMs, as the `vagrant` output makes them.
fn vms_only(spec: &Spec) -> Spec {
    let mut vms = spec.clone();
    for m in vms.machines.values_mut().filter(|m| m.docker.is_some()) {
        m.vm = None;
    }
    vms
}
