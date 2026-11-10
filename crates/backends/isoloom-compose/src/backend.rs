//! The Compose backend: what Compose can render, on the user's Docker host and on a VM that runs
//! the Compose file.

use isoloom_manifest::Manifest;
use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::Error as RenderError;
use isoloom_render::backend::{Backend, Files};
use isoloom_render::run_manifest::guest::GuestCompose;
use isoloom_render::steps::AsRoot;
use isoloom_spec::model::Target;

use crate::render;

/// The Compose backend of the `docker` and `hosted` targets (a hosting service runs the same
/// Compose file): the target it renders for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComposeBackend(pub Target);

impl Backend for ComposeBackend {
    fn target(&self) -> Target {
        self.0
    }

    fn capabilities(&self) -> Capabilities {
        capabilities(self.0)
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        Ok(render::render(self.0, env)?)
    }

    /// The `docker` target's manifest; a hosting service runs the file its own way.
    fn run_manifest(&self, env: &Environment) -> Result<Option<Manifest>, RenderError> {
        if self.0 != Target::Docker {
            return Ok(None);
        }
        let file = render::compose_file(self.0, env)?.file;
        Ok(Some(crate::manifest::docker(env, &file)?))
    }
}

/// What runs inside a Docker host VM (`docker-vm`, `cloud-docker`) for `env` (planned for the
/// `docker` target): the `docker` target's Compose file, its containers, and the commands that
/// start, check and read them there, as root as the way in gets it (`as_root`: the backend that
/// makes the VM runs them through its own way in).
///
/// # Errors
///
/// [`crate::Error::Refused`] when `env` needs what Compose can't do; a manifest error for a name
/// the run manifest refuses (a bug: the spec checks them).
pub fn guest(env: &Environment, as_root: AsRoot) -> crate::Result<GuestCompose> {
    let file = render::compose_file(Target::Docker, env)?.file;
    crate::manifest::guest(env, &file, as_root)
}

/// What Compose can render: network appliances (wired the way Docker attaches networks), aliases,
/// gateway machines, ulimits, `tc` (in a sidecar), IPv6 networks of every kind, secrets
/// (Compose's own, owners included), project files with their owner and mode (copied in from
/// Compose's environment), existing Docker networks, and the host's GPUs and devices
/// (the containers run on the user's own Docker host, which has them). No controller: no
/// `provision:`, no Ansible checks; `exec` checks run inside a container. No bridged
/// networks: a container reaches the host's LAN through a macvlan network made beforehand
/// (`external`). Data disks are named volumes, a tmpfs takes a mode, and a volume can be kept
/// past `down` or exist already; no raw disk (a container gets no block device). Only on the
/// `docker` target, folders of the computer that runs the lab: its containers run there (a
/// hosting service's and the hybrid VM's don't).
pub(crate) fn capabilities(target: Target) -> Capabilities {
    let caps = Capabilities::of([
        Feature::NetworkAppliances,
        Feature::Gateways,
        Feature::MachineAliases,
        Feature::Ulimits,
        Feature::TrafficControl,
        Feature::Ipv6,
        Feature::Ipv6Only,
        Feature::Ipv6Auto,
        Feature::Secrets,
        Feature::SecretOwners,
        Feature::OwnedProjectFiles,
        Feature::ExternalNetworks,
        Feature::MachineProcess,
        Feature::ContainerGpus,
        Feature::ContainerDevices,
        // Docker's DNS answers the names under the private zone's domain (network aliases).
        Feature::PrivateDns,
        Feature::DataDisks,
        Feature::TmpfsModes,
        Feature::PersistentVolumes,
        Feature::ExternalVolumes,
    ]);
    if target == Target::Docker { caps.with(Feature::HostFolders) } else { caps }
}

/// What a Compose file run on a VM can render (`docker-vm`, `cloud-docker`): Compose's, but
/// existing networks, which a fresh Docker host VM has none of, and the host's GPUs and
/// devices, which a VM Isoloom creates doesn't have, and volumes kept past `down` or existing
/// already (the storage goes with the VM). Secrets reach the VM over its own
/// connection (the runner), and Compose takes them from there.
#[must_use]
pub fn capabilities_on_a_vm() -> Capabilities {
    Capabilities::of([
        Feature::NetworkAppliances,
        Feature::Gateways,
        Feature::MachineAliases,
        Feature::Ulimits,
        Feature::TrafficControl,
        Feature::DataDisks,
        Feature::TmpfsModes,
        Feature::Ipv6,
        Feature::Ipv6Only,
        Feature::Ipv6Auto,
        Feature::Secrets,
        Feature::SecretOwners,
        Feature::OwnedProjectFiles,
        Feature::MachineProcess,
        Feature::PrivateDns,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vm_runs_what_compose_does_but_what_only_the_users_own_host_has() {
        let own_host_only = [
            Feature::ExternalNetworks,
            Feature::ContainerGpus,
            Feature::ContainerDevices,
            Feature::PersistentVolumes,
            Feature::ExternalVolumes,
        ];
        for f in own_host_only {
            assert!(capabilities(Target::Docker).has(f) && !capabilities_on_a_vm().has(f), "{f:?}");
        }
        assert!(capabilities_on_a_vm().has(Feature::Secrets));
    }

    #[test]
    fn only_the_docker_targets_containers_run_on_the_computer_whose_folders_they_mount() {
        let with: Vec<Target> = [Target::Docker, Target::Hosted, Target::Hybrid]
            .into_iter()
            .filter(|t| capabilities(*t).has(Feature::HostFolders))
            .collect();
        assert_eq!(with, [Target::Docker]);
        assert!(!capabilities_on_a_vm().has(Feature::HostFolders), "the VM's backend shares them, when it can");
    }
}
