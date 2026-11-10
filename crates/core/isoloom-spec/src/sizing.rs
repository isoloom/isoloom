//! What machines need when the spec doesn't say: the default size of one (a VM needs more than
//! a container), what the whole spec adds up to on a target, and what it adds up to when every
//! machine is a VM.

use crate::model::{self, Resources};

/// A machine's CPUs when its `resources` don't say. A VM and a container share this: a core is a
/// core; they differ only in the memory and disk they reserve.
pub const DEFAULT_CPUS: u32 = 1;
/// A VM's memory when its `resources` don't say, in MB.
pub(crate) const DEFAULT_MEMORY_MB: u32 = 1024;
/// A VM's disk when its `resources` don't say, in GB.
pub(crate) const DEFAULT_DISK_GB: u32 = 20;
/// A container's memory when its `resources` don't say, in MB: it shares the host kernel, so it
/// asks for less than a VM.
pub const CONTAINER_MEMORY_MB: u32 = 512;
/// A container's disk when its `resources` don't say, in GB.
pub(crate) const CONTAINER_DISK_GB: u32 = 5;

/// What a spec's machines add up to: how many, and their CPUs, memory and disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Totals {
    /// How many machines.
    pub machines: u32,
    /// Their CPUs, added up.
    pub cpus: u32,
    /// Their memory in MB, added up.
    pub memory_mb: u32,
    /// Their disks in GB, added up.
    pub disk_gb: u32,
}

/// A machine's size as it runs in one form: what its `resources` say, each field they leave out
/// the default of that form (a container reserves less memory and disk than a VM). The one rule
/// every generated file and report sizes a machine by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineSize {
    /// CPUs.
    pub cpus: u32,
    /// Memory, in MB.
    pub memory_mb: u32,
    /// Disk, in GB.
    pub disk_gb: u32,
}

impl MachineSize {
    /// The size of a machine whose `resources` are `r`, run as a container or as a VM.
    #[must_use]
    pub fn of(r: Option<Resources>, container: bool) -> MachineSize {
        let r = r.unwrap_or(Resources {
            cpus: None,
            memory_mb: None,
            disk_gb: None,
        });
        let (memory_default, disk_default) = if container {
            (CONTAINER_MEMORY_MB, CONTAINER_DISK_GB)
        } else {
            (DEFAULT_MEMORY_MB, DEFAULT_DISK_GB)
        };
        MachineSize {
            cpus: r.cpus.unwrap_or(DEFAULT_CPUS),
            memory_mb: r.memory_mb.unwrap_or(memory_default),
            disk_gb: r.disk_gb.unwrap_or(disk_default),
        }
    }
}

/// One machine's CPUs, memory and disk in a form ([`MachineSize::of`]).
fn machine_size(m: &model::Machine, container: bool) -> (u32, u32, u32) {
    let s = MachineSize::of(m.resources, container);
    (s.cpus, s.memory_mb, s.disk_gb)
}

/// Whether `target` runs `m` as a container (lighter) rather than a VM. Hybrid and Proxmox
/// decide machine by machine; the machine targets that aren't about a local footprint (external
/// hosts, cloud services) count as VM-sized, the heavier guess.
fn runs_as_container(target: model::Target, m: &model::Machine) -> bool {
    use model::Shape;
    match target.needs() {
        Shape::Docker => true,
        Shape::Vm | Shape::External | Shape::Cloud => false,
        Shape::VmOrLxc => m.docker.as_ref().is_some_and(|d| d.lxc),
        Shape::Either => m.docker.is_some(),
    }
}

/// Add each machine's size, picked by `size`, into the running totals. Saturating: a spec with
/// absurd per-machine resources reports a capped total instead of overflowing (a debug panic, a
/// release wrap).
fn add_up(spec: &model::Spec, size: impl Fn(&model::Machine) -> (u32, u32, u32)) -> Totals {
    spec.machines.values().fold(Totals::default(), |t, m| {
        let (cpus, memory_mb, disk_gb) = size(m);
        Totals {
            machines: t.machines + 1,
            cpus: t.cpus.saturating_add(cpus),
            memory_mb: t.memory_mb.saturating_add(memory_mb),
            disk_gb: t.disk_gb.saturating_add(disk_gb),
        }
    })
}

/// What the whole spec needs when every machine is a VM, each machine's `resources` or the VM
/// defaults: the heaviest a spec runs, and what `isoloom resources` reports before a run.
#[must_use]
pub fn totals(spec: &model::Spec) -> Totals {
    add_up(spec, |m| machine_size(m, false))
}

/// What the whole spec needs on `target`: every machine sized the way that target runs it (a
/// container reserves less than a VM), its `resources` filling the rest. The minimum to run the
/// lab there, so the Docker targets read lighter than the VM ones for the same spec.
#[must_use]
pub fn totals_for(spec: &model::Spec, target: model::Target) -> Totals {
    add_up(spec, |m| machine_size(m, runs_as_container(target, m)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Target;

    fn spec(machines: &str) -> model::Spec {
        crate::testing::model(&crate::testing::lab(machines)).unwrap()
    }

    #[test]
    fn totals_add_each_machines_size_or_the_defaults() {
        let spec = spec(
            "machines:\n  a: { networks: { lan: 10 }, docker: { image: x }, resources: { cpus: 4, memory_mb: 512, disk_gb: 5 } }\n  b: { networks: { lan: 11 }, docker: { image: x }, resources: { cpus: 2 } }\n  c: { networks: { lan: 12 }, docker: { image: x } }\n",
        );
        assert_eq!(
            totals(&spec),
            Totals {
                machines: 3,
                cpus: 4 + 2 + DEFAULT_CPUS,
                memory_mb: 512 + 2 * DEFAULT_MEMORY_MB,
                disk_gb: 5 + 2 * DEFAULT_DISK_GB,
            }
        );
    }

    #[test]
    fn totals_saturate_instead_of_overflowing() {
        let huge = u32::MAX;
        let spec = spec(&format!(
            "machines:\n  a: {{ networks: {{ lan: 10 }}, docker: {{ image: x }}, resources: {{ cpus: {huge} }} }}\n  b: {{ networks: {{ lan: 11 }}, docker: {{ image: x }}, resources: {{ cpus: 3 }} }}\n"
        ));
        assert_eq!(totals(&spec).cpus, u32::MAX);
    }

    #[test]
    fn a_target_sizes_containers_lighter_than_vms() {
        // One machine that can be either, no resources of its own.
        let spec = spec("machines:\n  a: { networks: { lan: 10 }, docker: { image: x }, vm: { os: debian@12 } }\n");
        let docker = totals_for(&spec, Target::Docker);
        let vagrant = totals_for(&spec, Target::Vagrant);
        assert_eq!((docker.memory_mb, docker.disk_gb), (CONTAINER_MEMORY_MB, CONTAINER_DISK_GB));
        assert_eq!((vagrant.memory_mb, vagrant.disk_gb), (DEFAULT_MEMORY_MB, DEFAULT_DISK_GB));
        assert_eq!((docker.cpus, vagrant.cpus), (DEFAULT_CPUS, DEFAULT_CPUS), "a core is a core");
        assert_eq!(totals(&spec), vagrant, "totals is the all-VM sizing");
    }

    #[test]
    fn declared_resources_win_over_the_kind_default() {
        let spec = spec("machines:\n  a: { networks: { lan: 10 }, docker: { image: x }, resources: { memory_mb: 2048 } }\n");
        let docker = totals_for(&spec, Target::Docker);
        assert_eq!(docker.memory_mb, 2048, "the spec's number wins");
        assert_eq!(docker.disk_gb, CONTAINER_DISK_GB, "the field it left out takes the container default");
    }

    #[test]
    fn hybrid_sizes_each_machine_by_its_own_kind() {
        // A container-capable machine (container on hybrid) beside a VM-only one (a VM on hybrid).
        let spec =
            spec("machines:\n  web: { networks: { lan: 10 }, docker: { image: x } }\n  win: { networks: { lan: 11 }, vm: { os: windows-server@2022 } }\n");
        let hybrid = totals_for(&spec, Target::Hybrid);
        assert_eq!(hybrid.memory_mb, CONTAINER_MEMORY_MB + DEFAULT_MEMORY_MB);
        assert_eq!(hybrid.disk_gb, CONTAINER_DISK_GB + DEFAULT_DISK_GB);
    }

    #[test]
    fn a_proxmox_lxc_container_is_lighter_than_a_proxmox_vm() {
        let lxc = spec("machines:\n  a: { networks: { lan: 10 }, docker: { image: x, lxc: true } }\n");
        let vm = spec("machines:\n  a: { networks: { lan: 10 }, vm: { os: debian@12 } }\n");
        assert_eq!(totals_for(&lxc, Target::Proxmox).memory_mb, CONTAINER_MEMORY_MB);
        assert_eq!(totals_for(&vm, Target::Proxmox).memory_mb, DEFAULT_MEMORY_MB);
    }
}
