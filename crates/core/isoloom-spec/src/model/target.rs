//! The targets an environment can be rendered for, and the shapes of machine they accept.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where an environment can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    /// Containers on the user's Docker.
    Docker,
    /// A hosting service runs the Docker shape for the user.
    Hosted,
    /// Docker on one VM: on this machine, an ESXi host or a Proxmox server.
    DockerVm,
    /// Docker on one cloud VM.
    CloudDocker,
    /// The containers on a Kubernetes cluster.
    Kubernetes,
    /// Containers and VMs together on the same networks (local VMs): each machine as a
    /// container when it can be one, else as a VM.
    Hybrid,
    /// One local VM per machine (Vagrant).
    Vagrant,
    /// One VM per machine on the user's Proxmox.
    Proxmox,
    /// One VM per machine on the user's standalone ESXi host (Terraform, no vCenter).
    Esxi,
    /// One cloud VM per machine.
    CloudVm,
    /// Machines that already exist, reached over SSH at the addresses the spec gives
    /// (`machines.*.external`): nothing is created, everything else applies.
    External,
    /// The spec's cloud services (`cloud:`): its Terraform module applied into the user's own
    /// cloud account.
    CloudServices,
}

impl Target {
    /// Every target, in the order they are listed (`isoloom targets`, the snapshot).
    pub const ALL: [Target; 12] = [
        Target::Docker,
        Target::Hosted,
        Target::DockerVm,
        Target::CloudDocker,
        Target::Kubernetes,
        Target::Hybrid,
        Target::Vagrant,
        Target::Proxmox,
        Target::Esxi,
        Target::CloudVm,
        Target::External,
        Target::CloudServices,
    ];

    /// The implementation every machine needs for this target.
    #[must_use]
    pub fn needs(self) -> Shape {
        match self {
            Target::Docker | Target::Hosted | Target::DockerVm | Target::CloudDocker | Target::Kubernetes => Shape::Docker,
            Target::Vagrant | Target::Esxi | Target::CloudVm => Shape::Vm,
            Target::Proxmox => Shape::VmOrLxc,
            Target::Hybrid => Shape::Either,
            Target::External => Shape::External,
            Target::CloudServices => Shape::Cloud,
        }
    }

    /// The target's name on the command line and in its folder under `.isoloom/`.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Target::Docker => "docker",
            Target::Hosted => "hosted",
            Target::DockerVm => "docker-vm",
            Target::CloudDocker => "cloud-docker",
            Target::Kubernetes => "kubernetes",
            Target::Hybrid => "hybrid",
            Target::Vagrant => "vagrant",
            Target::Proxmox => "proxmox",
            Target::Esxi => "esxi",
            Target::CloudVm => "cloud-vm",
            Target::External => "external",
            Target::CloudServices => "cloud-services",
        }
    }
}

/// The two ways a machine can be produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// A `docker:` block: a container.
    Docker,
    /// A `vm:` block: a virtual machine.
    Vm,
    /// Either implementation, machine by machine.
    Either,
    /// A `vm:` block, or a `docker:` one that may run as an LXC container (`docker.lxc`).
    VmOrLxc,
    /// An `external` block: the machine exists already.
    External,
    /// No machines: the spec's `cloud:` services.
    Cloud,
}

impl Shape {
    /// The spec key that gives a machine this shape, as a refusal names it (`docker`, `vm`).
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Shape::Docker => "docker",
            Shape::Vm => "vm",
            Shape::Either => "docker` or `vm",
            Shape::VmOrLxc => "docker.lxc` or `vm",
            Shape::External => "external",
            Shape::Cloud => "cloud",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_names_the_keys_a_refusal_quotes() {
        let quoted = [Shape::Docker, Shape::Vm, Shape::Either, Shape::VmOrLxc, Shape::External, Shape::Cloud].map(|s| format!("`{}`", s.key()));
        assert_eq!(
            quoted,
            ["`docker`", "`vm`", "`docker` or `vm`", "`docker.lxc` or `vm`", "`external`", "`cloud`"]
        );
    }
}
