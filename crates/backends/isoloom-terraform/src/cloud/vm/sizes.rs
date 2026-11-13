//! A cloud's size table and what a VM asks of it: the size a VM gets, and the refusal of one no
//! size takes.

use isoloom_plan::{Environment, MachinePlan};

use crate::cloud::Cloud;
use crate::machine::controller;

/// One size of a cloud's table, and what a VM of it gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Size {
    /// The size (`t3.micro`).
    pub(crate) name: &'static str,
    /// Its vCPUs.
    pub(crate) vcpus: u32,
    /// Its memory, in MB.
    pub(crate) memory_mb: u32,
    /// Its disk, in GB, when the size fixes it (DigitalOcean's droplets, Linode's plans); `None`
    /// when a machine's disk is its own setting.
    pub(crate) disk_gb: Option<u32>,
    /// The network interfaces it takes at most.
    pub(crate) interfaces: usize,
    /// The addresses one of its interfaces takes at most, where the cloud's sizes set it (AWS);
    /// `None` when the size doesn't.
    pub(crate) addresses: Option<usize>,
}

impl Size {
    /// A size with `vcpus`, `memory_mb` and room for `interfaces`, its disk a machine's own.
    pub(crate) const fn new(name: &'static str, vcpus: u32, memory_mb: u32, interfaces: usize) -> Size {
        Size {
            name,
            vcpus,
            memory_mb,
            disk_gb: None,
            interfaces,
            addresses: None,
        }
    }

    /// The size, its disk fixed at `disk_gb`.
    pub(crate) const fn with_disk(self, disk_gb: u32) -> Size {
        Size {
            disk_gb: Some(disk_gb),
            ..self
        }
    }

    /// The size, each interface taking `addresses` at most.
    pub(crate) const fn with_addresses(self, addresses: usize) -> Size {
        Size {
            addresses: Some(addresses),
            ..self
        }
    }

    /// Whether a VM of this size has what `need` asks for.
    fn takes(&self, need: &Need) -> bool {
        self.vcpus >= need.cpus
            && self.memory_mb >= need.memory_mb
            && self.disk_gb.is_none_or(|gb| gb >= need.disk_gb)
            && self.interfaces >= need.interfaces
            && self.addresses.is_none_or(|a| a >= need.addresses)
    }
}

/// The least memory a Windows VM gets, in MB: Windows wants 2 GB and more.
const WINDOWS_MEMORY_MB: u32 = 2048;

/// What a VM asks of its size: its CPUs, memory and disk, an interface per network, and the
/// addresses its busiest interface holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Need {
    /// CPUs.
    pub(crate) cpus: u32,
    /// Memory, in MB.
    pub(crate) memory_mb: u32,
    /// Disk, in GB.
    pub(crate) disk_gb: u32,
    /// Network interfaces: one per network.
    pub(crate) interfaces: usize,
    /// The addresses of its busiest interface (1: its own).
    pub(crate) addresses: usize,
}

impl Need {
    /// `cpus`, `memory_mb` and `disk_gb` on one interface, which holds its own address.
    pub(crate) fn new(cpus: u32, memory_mb: u32, disk_gb: u32) -> Need {
        Need {
            cpus,
            memory_mb,
            disk_gb,
            interfaces: 1,
            addresses: 1,
        }
    }

    /// What machine `mp` asks for: its size (a Windows machine's 2 GB of memory at least), and an
    /// interface on each of its networks.
    pub(crate) fn of(mp: &MachinePlan) -> Need {
        let size = mp.size();
        let memory_mb = if mp.is_windows() {
            size.memory_mb.max(WINDOWS_MEMORY_MB)
        } else {
            size.memory_mb
        };
        Need::new(size.cpus, memory_mb, size.disk_gb).on(mp.addresses.len())
    }

    /// What the controller asks for: its size, and an interface on every network.
    pub(crate) fn controller(env: &Environment) -> Need {
        let (cpus, memory_mb, disk_gb) = controller::size(env);
        Need::new(cpus, memory_mb, disk_gb).on(env.networks.len())
    }

    /// The same on `networks` networks, an interface on each (one at least).
    pub(crate) fn on(self, networks: usize) -> Need {
        Need {
            interfaces: networks.max(1),
            ..self
        }
    }

    /// The same, its busiest interface holding `addresses`.
    pub(crate) fn with_addresses(self, addresses: usize) -> Need {
        Need { addresses, ..self }
    }
}

/// A cloud's size table, smallest first, each size at least the one before it in every
/// respect ([`Sizes::ordered`]), so its last takes whatever any of them takes: a VM gets the first
/// that takes what it asks for, and one the last can't take is refused ([`Sizes::short`]).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sizes(&'static [Size]);

impl Sizes {
    /// `table` as a size table.
    ///
    /// # Panics
    ///
    /// When a size has less of something than the one before it (at compile time, for the
    /// constant tables).
    pub(crate) const fn ordered(table: &'static [Size]) -> Sizes {
        let mut rest = table;
        while let [a, b, ..] = rest {
            let disk = match (a.disk_gb, b.disk_gb) {
                (Some(a), Some(b)) => b >= a,
                (a, b) => a.is_none() && b.is_none(),
            };
            let addresses = match (a.addresses, b.addresses) {
                (Some(a), Some(b)) => b >= a,
                (a, b) => a.is_none() && b.is_none(),
            };
            assert!(
                b.vcpus >= a.vcpus && b.memory_mb >= a.memory_mb && b.interfaces >= a.interfaces && disk && addresses,
                "a size table's sizes grow in every respect"
            );
            if let [_, tail @ ..] = rest {
                rest = tail;
            }
        }
        Sizes(table)
    }

    /// The sizes' names, smallest first.
    #[cfg(test)]
    pub(crate) fn names(self) -> impl Iterator<Item = &'static str> {
        self.0.iter().map(|s| s.name)
    }

    /// The smallest size that takes `need`; the largest when none does, which the cloud's
    /// refusal ([`Sizes::short`]) rules out.
    pub(crate) fn fit(self, need: &Need) -> &'static str {
        self.0.iter().find(|s| s.takes(need)).or(self.0.last()).map_or("", |s| s.name)
    }

    /// What the largest size lacks for `who` (``machine `web` ``, `the controller`), which asks
    /// for `need` on `cloud`, if it lacks something: CPUs, memory, disk, interfaces, then
    /// addresses on an interface.
    pub(crate) fn short(self, cloud: Cloud, who: &str, need: &Need) -> Option<String> {
        let largest = self.0.last()?;
        let has = |what: String| format!("the largest {cloud} size here, `{}`, {what}", largest.name);
        if need.cpus > largest.vcpus {
            return Some(format!("{who} asks for {} CPUs: {}", need.cpus, has(format!("has {} vCPUs", largest.vcpus))));
        }
        if need.memory_mb > largest.memory_mb {
            return Some(format!(
                "{who} asks for {} of memory: {}",
                mb(need.memory_mb),
                has(format!("has {}", mb(largest.memory_mb)))
            ));
        }
        if let Some(disk) = largest.disk_gb.filter(|gb| need.disk_gb > *gb) {
            return Some(format!("{who} asks for a {} GB disk: {}", need.disk_gb, has(format!("has {disk} GB"))));
        }
        if need.interfaces > largest.interfaces {
            return Some(format!(
                "{who} is on {} networks: {}",
                need.interfaces,
                has(format!("takes {} network interfaces, one per network", largest.interfaces))
            ));
        }
        let most = largest.addresses.filter(|a| need.addresses > *a)?;
        Some(format!(
            "{who} asks for {} addresses on one network interface: {}",
            need.addresses,
            has(format!("takes {most}"))
        ))
    }
}

/// An amount of memory as a refusal says it: whole GB when it is, else MB.
fn mb(memory_mb: u32) -> String {
    if memory_mb.is_multiple_of(1024) {
        format!("{} GB", memory_mb / 1024)
    } else {
        format!("{memory_mb} MB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_table_that_shrinks_somewhere_is_refused() {
        static SHRINKS: [Size; 2] = [Size::new("a", 4, 1024, 2), Size::new("b", 2, 2048, 2)];
        static GROWS: [Size; 2] = [Size::new("a", 2, 1024, 2), Size::new("b", 2, 2048, 3)];
        assert!(std::panic::catch_unwind(|| Sizes::ordered(&SHRINKS)).is_err());
        let need = Need::new(1, 1500, 10);
        assert_eq!(Sizes::ordered(&GROWS).fit(&need), "b");
        assert_eq!(Sizes::ordered(&GROWS).fit(&need.on(4)), "b", "the largest when none takes it");
        assert!(
            Sizes::ordered(&GROWS)
                .short(Cloud::Aws, "x", &need.on(4))
                .is_some_and(|w| w.contains("takes 3 network interfaces"))
        );
    }
}
