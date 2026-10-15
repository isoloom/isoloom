//! What machines need when the spec doesn't say: the default size of one (sized for a VM, which
//! needs more than a container), and what the whole spec adds up to.

use crate::model::{self, Resources};

/// A machine's CPUs when its `resources` don't say (defaults sized for a VM, which needs more
/// than a container).
pub const DEFAULT_CPUS: u32 = 1;
/// A machine's memory when its `resources` don't say, in MB.
pub const DEFAULT_MEMORY_MB: u32 = 1024;
/// A machine's disk when its `resources` don't say, in GB.
pub const DEFAULT_DISK_GB: u32 = 20;

/// What the whole spec needs when every machine is a VM (the access machine included).
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

/// What the whole spec needs when every machine is a VM, each machine's `resources` or the
/// defaults: what `isoloom resources` reports before a run.
#[must_use]
pub fn totals(spec: &model::Spec) -> Totals {
    spec.machines.values().fold(Totals::default(), |t, m| {
        let r = m.resources.unwrap_or(Resources {
            cpus: None,
            memory_mb: None,
            disk_gb: None,
        });
        // Saturating: a spec with absurd per-machine resources reports a capped total instead of
        // overflowing (a debug panic / release wrap).
        Totals {
            machines: t.machines + 1,
            cpus: t.cpus.saturating_add(r.cpus.unwrap_or(DEFAULT_CPUS)),
            memory_mb: t.memory_mb.saturating_add(r.memory_mb.unwrap_or(DEFAULT_MEMORY_MB)),
            disk_gb: t.disk_gb.saturating_add(r.disk_gb.unwrap_or(DEFAULT_DISK_GB)),
        }
    })
}
