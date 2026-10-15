//! The `kubernetes` target as a backend: what a cluster can run, why it refuses an environment,
//! and its files.

use isoloom_plan::{Capabilities, Environment, Feature, Refusal, Requirement};
use isoloom_render::backend::{Backend, Files, RenderError};
use isoloom_spec::model::Target;

use crate::{gpus, render};

/// The `kubernetes` target's backend: plain manifests with their kustomizations, the checks'
/// Jobs, and a Helm chart of the same objects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct KubernetesBackend;

impl Backend for KubernetesBackend {
    fn target(&self) -> Target {
        Target::Kubernetes
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        Ok(render::render(env)?)
    }

    /// The plan's refusal, in Kubernetes' words (a gateway machine's is its own), then what its
    /// GPUs can't be.
    fn refusal(&self, env: &Environment) -> Option<String> {
        isoloom_plan::refusal(&env.requirements, &capabilities())
            .map(|r| reason(&r))
            .or_else(|| gpus::refusal(env))
    }
}

/// What Kubernetes can render: a machine's own process, IPv6 networks of every kind (the
/// cluster picks pod addresses, so a network's IPv6 side sets the Services' IP families),
/// secrets (a Secret `isoloom run` creates; one with an owner copied into place by an init
/// container) and a project file with its own owner and mode (copied into place by an init
/// container). No controller (no `provision:`, no Ansible checks), `exec` checks run inside a
/// container, no network appliances, Service names have no dots, no gateway machine routes a
/// network, containers have no ulimits, with no router in the path there is nothing to impair
/// links with, and no host devices (a pod reaches a node's devices only through a device
/// plugin). GPUs are the device plugin's resource (`nvidia.com/gpu`). Volumes can be kept past
/// `down` or exist already (claims), and data disks are claims (raw ones block devices); a
/// tmpfs has no mode (a memory `emptyDir` has none).
pub(crate) fn capabilities() -> Capabilities {
    Capabilities::of([
        Feature::Ipv6,
        Feature::Ipv6Only,
        Feature::Ipv6Auto,
        Feature::Secrets,
        Feature::SecretOwners,
        Feature::OwnedProjectFiles,
        Feature::MachineProcess,
        Feature::ContainerGpus,
        Feature::PersistentVolumes,
        Feature::ExternalVolumes,
        Feature::DataDisks,
        Feature::RawDisks,
    ])
}

/// Why Kubernetes refuses: the plan's reason, but for gateway machines, which the plan words for
/// the hybrid target.
pub(crate) fn reason(refusal: &Refusal) -> String {
    match refusal.requirement {
        Requirement::Gateway { .. } => "networks with a `gateway` machine on Kubernetes come later".into(),
        _ => refusal.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cluster_runs_containers_but_neither_routes_nor_impairs_their_links() {
        let caps = capabilities();
        assert!(caps.has(Feature::Secrets) && caps.has(Feature::ContainerGpus) && caps.has(Feature::RawDisks));
        for missing in [Feature::Gateways, Feature::TrafficControl, Feature::NetworkAppliances, Feature::Ulimits] {
            assert!(!caps.has(missing), "{missing:?}");
        }
    }
}
