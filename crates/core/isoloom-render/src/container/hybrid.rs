//! The containers' side of a hybrid environment, as the Compose backend writes it for the
//! Docker host VM the Vagrant backend adds.

use isoloom_plan::Environment;
use isoloom_spec::model::Protocol;

use crate::GeneratedFile;

/// The containers' side of a hybrid environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridContainers {
    /// `.isoloom/hybrid/compose.yml`.
    pub file: GeneratedFile,
    /// The ports published on the host, in the order of the services: the Docker host forwards
    /// them.
    pub published: Vec<(u16, Protocol)>,
    /// The networks containers are on, in spec order: the Docker host creates them.
    pub networks: Vec<String>,
}

/// The Docker network a hybrid environment's container network is on the host: named after the
/// environment.
#[must_use]
pub fn docker_net(env: &Environment, net: &str) -> String {
    format!("isoloom-{}-{net}", env.name)
}
