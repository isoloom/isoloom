//! A machine the target runs as a VM, with its OS: a VM's OS is never absent.

use super::{Environment, MachinePlan, MachineShape};

/// A machine planned as a VM, with the OS a VM always has: its shape is [`MachineShape::Vm`]
/// only when the target builds it from its `vm:` form, which names one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vm<'a> {
    /// Its name.
    pub name: &'a str,
    /// Its plan.
    pub plan: &'a MachinePlan,
    /// Its OS (`debian-12`, `windows-server-2022`...).
    pub os: &'a str,
}

impl<'a> Vm<'a> {
    /// Machine `name` planned as `plan`, when it is a VM. The plan makes a machine a VM only
    /// from its `vm:` form, whose OS it keeps, and only the plan builds a [`MachinePlan`] (its
    /// OS is private): every VM it made is one here.
    #[must_use]
    pub fn of(name: &'a str, plan: &'a MachinePlan) -> Option<Vm<'a>> {
        match (plan.shape, plan.os.as_deref()) {
            (MachineShape::Vm, Some(os)) => Some(Vm { name, plan, os }),
            _ => None,
        }
    }
}

impl Environment {
    /// The machines it runs as VMs, in start order, each with its OS.
    pub fn vms(&self) -> impl Iterator<Item = Vm<'_>> {
        self.start_order.iter().filter_map(|n| self.vm(n))
    }

    /// Machine `name`, when it runs as a VM, with its OS.
    #[must_use]
    pub fn vm(&self, name: &str) -> Option<Vm<'_>> {
        let (name, plan) = self.machines.get_key_value(name)?;
        Vm::of(name, plan)
    }
}
