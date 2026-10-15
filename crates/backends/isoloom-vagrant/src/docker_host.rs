//! The VM that runs an environment's containers: on this machine for `docker-vm`, beside the
//! VMs for `hybrid`. Sized for every container at once.

use isoloom_plan::{Environment, MachineShape};

/// The containers' host's size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Size {
    /// One CPU a container unless it says; at least 2, at most 16.
    pub(crate) cpus: u32,
    /// 512 MB a container unless it says, plus 1 GB for Docker itself.
    pub(crate) memory_mb: u32,
}

impl Size {
    /// The host's size for the environment's containers.
    pub(crate) fn of(env: &Environment) -> Size {
        let containers = env.machines.values().filter(|m| m.shape == MachineShape::Container);
        let (cpus, memory_mb) = containers.fold((0u32, 1024u32), |(c, m), x| {
            (c.saturating_add(x.size.cpus.unwrap_or(1)), m.saturating_add(x.size.memory_mb.unwrap_or(512)))
        });
        Size {
            cpus: cpus.clamp(2, 16),
            memory_mb,
        }
    }
}

#[cfg(test)]
mod tests {
    use isoloom_plan::Environment;
    use isoloom_spec::model::Target;

    use super::Size;

    #[test]
    fn every_container_counts_and_docker_takes_a_gigabyte() {
        let spec = isoloom_plan::Lab::parse(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { networks: { lan: 10 }, app: { image: nginx } }\n  b: { networks: { lan: 11 }, resources: { cpus: 3, memory_mb: 2048 }, app: { image: nginx } }\n",
        )
        .expect("parses")
        .for_target(Target::DockerVm);
        let env = Environment::build(&spec, Target::DockerVm, None);
        assert_eq!(Size::of(&env), Size { cpus: 4, memory_mb: 3584 });
    }
}
