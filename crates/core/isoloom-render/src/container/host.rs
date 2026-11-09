//! The VM the containers run on (a cloud VM, a local VM): its size, every container's at once.

use isoloom_plan::{Environment, MachineShape};

/// The size of the VM the containers run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct HostSize {
    /// CPUs: one per container unless it says, at least 2, at most 16.
    pub cpus: u32,
    /// Memory in MB: 512 per container unless it says, plus 1 GB for Docker itself, at least
    /// [`HostSize::MIN_MEMORY_MB`].
    pub memory_mb: u32,
    /// Disk in GB: 5 per container unless it says, at least [`HostSize::MIN_DISK_GB`].
    pub disk_gb: u32,
}

impl HostSize {
    /// The smallest disk of a containers' host: room for the system, Docker, and the lab's images
    /// while they are pulled (each download beside its unpacked layers), which the 5 GB a
    /// container counts by default don't cover for a large one: a full desktop or tools image
    /// unpacks to about 12 GB, and an access machine's image may be one the runner is given
    /// (`ISOLOOM_ACCESS_IMAGE`), which the spec doesn't size. 20 GB filled up.
    pub const MIN_DISK_GB: u32 = 40;

    /// The smallest memory of a containers' host: Debian, Docker and a small lab's containers
    /// together, which 1.5 GB (one container and Docker's own gigabyte) leaves short.
    pub const MIN_MEMORY_MB: u32 = 2048;

    /// The host's size for the environment's containers.
    #[must_use]
    pub fn of(env: &Environment) -> HostSize {
        let containers = env.machines.values().filter(|m| m.shape == MachineShape::Container);
        let (cpus, memory_mb, disk_gb) = containers.fold((0_u32, 1024_u32, 0_u32), |(cpus, memory, disk), m| {
            let size = m.size();
            (
                cpus.saturating_add(size.cpus),
                memory.saturating_add(size.memory_mb),
                disk.saturating_add(size.disk_gb),
            )
        });
        HostSize {
            cpus: cpus.clamp(2, 16),
            memory_mb: memory_mb.max(Self::MIN_MEMORY_MB),
            disk_gb: disk_gb.max(Self::MIN_DISK_GB),
        }
    }

    /// The most memory a containers' host on a cloud has: each cloud's largest size of the three
    /// [`HostSize::pick`] chooses from. `cloud-docker` refuses an environment whose containers
    /// need more.
    pub const LARGEST_MEMORY_MB: u32 = 16384;

    /// One of three sizes of VM by its memory: up to 4 GB, up to 8 GB, up to
    /// [`HostSize::LARGEST_MEMORY_MB`] (the largest for more, which `cloud-docker` refuses).
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
                disk_gb: 40
            }
        );
        assert_eq!(size.pick(["small", "medium", "large"]), "medium");
    }

    #[test]
    fn a_few_small_containers_take_the_smallest_host() {
        let size =
            host_size("version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  a: { networks: { lan: 10 }, app: { image: nginx } }\n");
        // 1.5 GB (512 MB and Docker's gigabyte) is lifted to the floor, on every target alike.
        assert_eq!((size.cpus, size.memory_mb, size.disk_gb), (2, HostSize::MIN_MEMORY_MB, 40));
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
