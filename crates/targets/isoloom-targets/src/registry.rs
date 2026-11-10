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

static DOCKER: ComposeBackend = ComposeBackend::DOCKER;
static HOSTED: ComposeBackend = ComposeBackend::HOSTED;
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

/// The backend of `target`: every target has one.
#[must_use]
pub fn backend(target: Target) -> &'static dyn Backend {
    match target {
        Target::Docker => &DOCKER,
        Target::Hosted => &HOSTED,
        Target::DockerVm => &DOCKER_VM,
        Target::CloudDocker => &CLOUD_DOCKER,
        Target::Kubernetes => &KUBERNETES,
        Target::Hybrid => &HYBRID,
        Target::Vagrant => &VAGRANT,
        Target::Proxmox => &PROXMOX,
        Target::Esxi => &ESXI,
        Target::CloudVm => &CLOUD_VM,
        Target::External => &EXTERNAL,
        Target::CloudServices => &CLOUD_SERVICES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_targets_backend_renders_that_target() {
        for t in Target::ALL {
            assert_eq!(backend(t).target(), t);
        }
    }
}
