//! The `hybrid` target: containers and VMs together on the same networks, on this machine, in
//! `.isoloom/hybrid/` (a Vagrantfile and a Compose file).
//!
//! - A machine with `docker:` is a container (the light option); a machine with only `vm:` is a
//!   VM. The VMs, the router and the controller are what the `vagrant` output makes of them.
//! - The containers run on one more VM, `isoloom-docker`, plugged into the networks they're on
//!   with promiscuous network interfaces: Docker attaches each container to its network directly
//!   (`macvlan`), at its own address, so VMs and containers reach each other as on one network
//!   (`host`). Their Compose file is the Compose backend's, plugged into those networks
//!   (`containers`).
//! - Published ports: macvlan can't publish, so a container with published ports also joins
//!   Docker's bridge network (its default route stays on its lab network), and the Docker host
//!   VM forwards each port to this machine's loopback.
//! - VirtualBox only for now (promiscuous network interfaces are set per provider).
//! - Refused, from the plan's requirements: gateways; a shared volume mounted by containers and
//!   VMs; a VM that depends on a container (VMs start first); network appliances. Containers
//!   don't reach the internet while running (the Docker host pulls their images).

mod containers;
mod host;

use isoloom_plan::{Capabilities, Environment, Feature, MachineShape};
use isoloom_render::backend::{self, Backend, Files, RenderError};
use isoloom_render::{GenerateError, GeneratedFile};
use isoloom_spec::model::{Spec, Target};

use crate::vagrant::{self, Layout};

const DIR: &str = "hybrid";

/// The `hybrid` backend.
#[derive(Debug, Clone, Copy)]
pub struct HybridBackend;

impl Backend for HybridBackend {
    fn target(&self) -> Target {
        Target::Hybrid
    }

    /// The VM side's controller runs provisioning and every kind of check. Gateways, volumes
    /// shared across containers and VMs, VMs waiting for containers and network appliances
    /// aren't wired yet.
    fn capabilities(&self) -> Capabilities {
        Capabilities::of([Feature::EnvironmentProvisioning, Feature::PlaybookChecks, Feature::ExecWithoutContainer])
    }

    /// The Vagrantfile (the VMs, then the containers' host) and the checks next to it, and the
    /// containers' Compose file.
    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        backend::check(Target::Hybrid, env, &self.capabilities())?;
        let containers = containers::render(env)?;
        let host = host::block(env, &containers)?;
        let layout = Layout { dir: DIR, extra: &host };
        let mut files = vagrant::generate_in(&vms_only(env.spec()), &layout).map_err(as_hybrid)?;
        files.insert(1.min(files.len()), containers.file);
        Ok(files)
    }

    /// The plan's refusal; else, since the VMs are still the `vagrant` generator's, what it
    /// refuses on rendering (a box it doesn't know, a Windows feature it lacks).
    fn refusal(&self, env: &Environment) -> Option<String> {
        if let Some(r) = isoloom_plan::refusal(&env.requirements, &self.capabilities()) {
            return Some(r.to_string());
        }
        match self.render(env) {
            Err(e) => match GenerateError::from(e) {
                GenerateError::Unsupported { what, .. } => Some(what),
                _ => None,
            },
            Ok(_) => None,
        }
    }
}

/// The spec with the containers' VM forms left out: the VMs, as the `vagrant` output makes them.
fn vms_only(spec: &Spec) -> Spec {
    let mut vms = spec.clone();
    for m in vms.machines.values_mut().filter(|m| m.docker.is_some()) {
        m.vm = None;
    }
    vms
}

/// Whether `machine` is a container in `env`.
fn is_container(env: &Environment, machine: &str) -> bool {
    env.machines.get(machine).is_some_and(|m| m.shape == MachineShape::Container)
}

/// A refusal of the parts hybrid is made of (the `vagrant` generator, the Compose backend),
/// reported as hybrid's.
fn as_hybrid(e: impl Into<RenderError>) -> RenderError {
    match e.into() {
        RenderError::Refused { refusal, .. } => RenderError::Refused {
            target: Target::Hybrid,
            refusal,
        },
        RenderError::Generate(GenerateError::Unsupported { what, .. }) => RenderError::Generate(GenerateError::Unsupported { target: Target::Hybrid, what }),
        other => other,
    }
}

/// A file of the hybrid output.
fn file(name: &str, contents: String) -> GeneratedFile {
    GeneratedFile {
        path: format!("{}/{DIR}/{name}", isoloom_render::OUTPUT_DIR),
        contents,
    }
}
