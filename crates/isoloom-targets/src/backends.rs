//! The registry of backends, one per generated target. Every target renders the plan: Compose,
//! Kubernetes, external, docker-vm, hybrid, Vagrant, Proxmox, ESXi, cloud-vm, cloud-docker and
//! cloud-services.

use isoloom_compose::ComposeBackend;
use isoloom_external::ExternalBackend;
use isoloom_kubernetes::KubernetesBackend;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::cloud_docker::CloudDockerBackend;
use isoloom_terraform::cloud_services::CloudServicesBackend;
use isoloom_terraform::cloud_vm::CloudVmBackend;
use isoloom_terraform::esxi::EsxiBackend;
use isoloom_terraform::proxmox::ProxmoxBackend;
use isoloom_vagrant::vagrant::VagrantBackend;
use isoloom_vagrant::{DockerVmBackend, HybridBackend};

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
pub static BACKENDS: &[&dyn Backend] = &[
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
pub fn backend(target: Target) -> Option<&'static dyn Backend> {
    BACKENDS.iter().copied().find(|b| b.target() == target)
}
