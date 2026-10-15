//! The VM the containers run on (a cloud VM, a local VM): its size, every container's at once.

use isoloom_plan::{Environment, MachineShape};

/// The size of the VM the containers run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct HostSize {
    /// CPUs: one per container unless it says, at least 2, at most 16.
    pub cpus: u32,
    /// Memory in MB: 512 per container unless it says, plus 1 GB for Docker itself.
    pub memory_mb: u32,
    /// Disk in GB: 5 per container unless it says, at least 20.
    pub disk_gb: u32,
}

impl HostSize {
    /// The host's size for the environment's containers.
    #[must_use]
    pub fn of(env: &Environment) -> HostSize {
        let containers = env.machines.values().filter(|m| m.shape == MachineShape::Container);
        let (cpus, memory_mb, disk_gb) = containers.fold((0_u32, 1024_u32, 0_u32), |(cpus, memory, disk), m| {
            (
                cpus.saturating_add(m.size.cpus.unwrap_or(1)),
                memory.saturating_add(m.size.memory_mb.unwrap_or(512)),
                disk.saturating_add(m.size.disk_gb.unwrap_or(5)),
            )
        });
        HostSize {
            cpus: cpus.clamp(2, 16),
            memory_mb,
            disk_gb: disk_gb.max(20),
        }
    }

    /// One of three sizes of VM by its memory: up to 4 GB, up to 8 GB, more.
    #[must_use]
    pub fn pick<T: Copy>(&self, [small, medium, large]: [T; 3]) -> T {
        match self.memory_mb {
            0..=4096 => small,
            4097..=8192 => medium,
            _ => large,
        }
    }
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::HostSize;
    use crate::testing;

    /// The containers' host for `yaml`, on `docker-vm`.
    fn host_size(yaml: &str) -> HostSize {
        HostSize::of(&testing::env(yaml, Target::DockerVm))
    }

    #[test]
    fn every_container_counts_and_docker_takes_a_gigabyte() {
        let size = host_size(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { networks: { lan: 10 }, app: { image: nginx } }\n  b: { networks: { lan: 11 }, resources: { cpus: 3, memory_mb: 4096, disk_gb: 30 }, app: { image: nginx } }\n",
        );
        assert_eq!(
            size,
            HostSize {
                cpus: 4,
                memory_mb: 512 + 4096 + 1024,
                disk_gb: 35
            }
        );
        assert_eq!(size.pick(["small", "medium", "large"]), "medium");
    }

    #[test]
    fn a_few_small_containers_take_the_smallest_host() {
        let size =
            host_size("version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { networks: { lan: 10 }, app: { image: nginx } }\n");
        assert_eq!((size.cpus, size.memory_mb, size.disk_gb), (2, 1536, 20));
        assert_eq!(size.pick(["small", "medium", "large"]), "small");
    }

    #[test]
    fn many_containers_take_at_most_sixteen_cpus_and_the_largest_host() {
        let machines: String = (0..20)
            .map(|i| format!("  m{i}: {{ networks: {{ lan: {} }}, app: {{ image: nginx }} }}\n", 10 + i))
            .collect::<Vec<_>>()
            .concat();
        let size = host_size(&format!(
            "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n{machines}"
        ));
        assert_eq!((size.cpus, size.memory_mb, size.disk_gb), (16, 20 * 512 + 1024, 100));
        assert_eq!(size.pick(["small", "medium", "large"]), "large");
    }
}
