//! A machine as this backend reads it: its name, what the spec says of it, and its plan, found
//! together once instead of looked up by name in each.

use isoloom_plan::{Environment, MachinePlan, MachineShape};
use isoloom_spec::model;

/// A machine of the environment: its name, its spec and its plan.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Machine<'a> {
    /// Its name, its service's too.
    pub(crate) name: &'a str,
    /// What the plan doesn't model yet: its `docker:` block, services, privileges, DNS.
    pub(crate) spec: &'a model::Machine,
    /// Its plan.
    pub(crate) plan: &'a MachinePlan,
}

impl<'a> Machine<'a> {
    /// Machine `name` of `env`, when it has one by that name.
    pub(crate) fn of(env: &'a Environment, name: &str) -> Option<Machine<'a>> {
        let (name, plan) = env.machines.get_key_value(name)?;
        let spec = env.spec().machines.get(name)?;
        Some(Machine { name, spec, plan })
    }

    /// Every machine of `env`, in spec order.
    pub(crate) fn all(env: &'a Environment) -> impl Iterator<Item = Machine<'a>> {
        env.machines.iter().filter_map(|(name, plan)| {
            let spec = env.spec().machines.get(name)?;
            Some(Machine { name, spec, plan })
        })
    }

    /// The machines of `env` that are containers (a service each), in spec order.
    pub(crate) fn containers(env: &'a Environment) -> impl Iterator<Item = Machine<'a>> {
        Machine::all(env).filter(|m| m.plan.shape == MachineShape::Container)
    }

    /// The access boxes of `env` that are `Supplied` (no form of their own), each rendered as an
    /// on-demand service; Container-shape access boxes are already in [`Machine::containers`].
    pub(crate) fn supplied_access(env: &'a Environment) -> impl Iterator<Item = Machine<'a>> {
        Machine::all(env).filter(|m| m.spec.access && m.plan.shape != MachineShape::Container)
    }

    /// Whether it is a network appliance (a router or switch OS in its container).
    pub(crate) fn is_appliance(&self) -> bool {
        self.spec.docker.as_ref().is_some_and(|d| d.appliance.is_some())
    }
}
