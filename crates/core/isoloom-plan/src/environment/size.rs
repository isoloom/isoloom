//! A machine's size as it runs: its own, each field left out the default of its form.

use isoloom_spec::MachineSize;

use super::{MachinePlan, MachineShape};

impl MachinePlan {
    /// The machine's size as it runs: its `resources`, each field left out the default of the
    /// form it runs as, a container's or a VM's ([`MachineSize`]). The one rule every backend
    /// sizes a lab machine by.
    #[must_use]
    pub fn size(&self) -> MachineSize {
        MachineSize::of(Some(self.resources), self.shape == MachineShape::Container)
    }
}
