//! A machine's size as it runs: its own, each field left out the default of its form.

use super::Size;

impl Size {
    /// The machine's size as it runs as a container or as a VM: these, each left out the
    /// default of that form ([`isoloom_spec::MachineSize`]).
    #[must_use]
    pub fn effective(self, container: bool) -> isoloom_spec::MachineSize {
        let r = isoloom_spec::model::Resources {
            cpus: self.cpus,
            memory_mb: self.memory_mb,
            disk_gb: self.disk_gb,
        };
        isoloom_spec::MachineSize::of(Some(r), container)
    }
}
