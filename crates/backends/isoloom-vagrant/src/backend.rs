//! The `vagrant` target as a backend: what Vagrant can run, why it refuses an environment, and
//! its files.

use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::backend::{Backend, Files, RenderError};
use isoloom_spec::model::Target;

use crate::vms;

/// The `vagrant` target's backend: one VM per machine in a Vagrantfile, on every provider.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct VagrantBackend;

impl Backend for VagrantBackend {
    fn target(&self) -> Target {
        Target::Vagrant
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        Ok(vms::render(env)?)
    }

    /// The plan's requirements against the capabilities, then the machines Vagrant can't build,
    /// without rendering.
    fn refusal(&self, env: &Environment) -> Option<String> {
        isoloom_plan::refusal(&env.requirements, &capabilities())
            .map(|r| r.to_string())
            .or_else(|| vms::limits::refusal(env))
    }
}

/// What Vagrant can render: a controller VM runs the environment's playbooks and the Ansible
/// checks, every check runs where its machine stands (a VM), and a machine can route a network
/// (its `gateway`). No network appliances (they are wired the way Docker attaches networks).
pub(crate) fn capabilities() -> Capabilities {
    Capabilities::of([
        Feature::EnvironmentProvisioning,
        Feature::PlaybookChecks,
        Feature::ExecWithoutContainer,
        Feature::Gateways,
        Feature::MachineAliases,
        Feature::Ulimits,
        Feature::TrafficControl,
        Feature::Ipv6,
        Feature::BridgedNetworks,
        // Given on VirtualBox, libvirt and Hyper-V; `provider_refusal` refuses the others.
        Feature::Tpm,
        // Delivered by `isoloom run` over the machine's SSH, owners included.
        Feature::Secrets,
        Feature::SecretOwners,
        Feature::MachineProcess,
        // The libvirt provider boots it; `provider_refusal` says which providers can't.
        Feature::OwnBoot,
        Feature::NestedVirtualization,
        // No DNS of its own: each VM's hosts file carries the names under the zone's domain.
        Feature::PrivateDns,
        // Each VM's set-up adds them on the interface of its primary address, on every provider.
        Feature::VmMoreAddresses,
        Feature::DataDisks,
        Feature::RawDisks,
        Feature::TmpfsModes,
    ])
}
