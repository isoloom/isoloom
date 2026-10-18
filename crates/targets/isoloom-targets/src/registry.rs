//! The registry of backends, one per target: Compose, Kubernetes, external (Ansible), docker-vm,
//! hybrid, Vagrant, Proxmox, ESXi, cloud-vm, cloud-docker and cloud-services. Every target
//! renders the plan through the backend registered for it.

use isoloom_ansible::ExternalBackend;
use isoloom_compose::ComposeBackend;
use isoloom_kubernetes::KubernetesBackend;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::{CloudServicesBackend, CloudVmBackend, EsxiBackend, ProxmoxBackend};
use isoloom_vagrant::VagrantBackend;

use crate::composite::{CloudDockerBackend, DockerVmBackend, HybridBackend};

static DOCKER: ComposeBackend = ComposeBackend(Target::Docker);
static HOSTED: ComposeBackend = ComposeBackend(Target::Hosted);
static DOCKER_VM: DockerVmBackend = DockerVmBackend;
static CLOUD_DOCKER: CloudDockerBackend = CloudDockerBackend;
static KUBERNETES: KubernetesBackend = KubernetesBackend;
static HYBRID: HybridBackend = HybridBackend;
static VAGRANT: VagrantBackend = VagrantBackend;
static PROXMOX: ProxmoxBackend = ProxmoxBackend;
static ESXI: EsxiBackend = EsxiBackend;
static CLOUD_VM: CloudVmBackend = CloudVmBackend;
static EXTERNAL: ExternalBackend = ExternalBackend;
static CLOUD_SERVICES: CloudServicesBackend = CloudServicesBackend;

/// Every backend, in the order targets are listed.
static BACKENDS: &[&dyn Backend] = &[
    &DOCKER,
    &HOSTED,
    &DOCKER_VM,
    &CLOUD_DOCKER,
    &KUBERNETES,
    &HYBRID,
    &VAGRANT,
    &PROXMOX,
    &ESXI,
    &CLOUD_VM,
    &EXTERNAL,
    &CLOUD_SERVICES,
];

/// The backend of `target`, if it has one.
#[must_use]
pub fn backend(target: Target) -> Option<&'static dyn Backend> {
    BACKENDS.iter().copied().find(|b| b.target() == target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_has_one_backend_in_the_order_targets_are_listed() {
        let registered: Vec<Target> = BACKENDS.iter().map(|b| b.target()).collect();
        assert_eq!(registered, Target::ALL);
    }
}
