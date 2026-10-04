//! Which targets a spec can run on. Derived, not declared: a target is possible when every
//! machine (except the access machine, which the runner supplies) has the implementation
//! that target needs. All-or-nothing: one machine without `docker:` rules out every Docker
//! target, because an environment missing a machine doesn't behave like the original.

use crate::model::{Shape, Spec, Target};

/// Machines lacking the implementation a shape needs (the access machine never counts).
pub fn missing(spec: &Spec, shape: Shape) -> Vec<String> {
    spec.machines
        .iter()
        .filter(|(_, m)| !m.access)
        .filter(|(_, m)| match shape {
            Shape::Docker => m.docker.is_none(),
            Shape::Vm => m.vm.is_none(),
        })
        .map(|(n, _)| n.clone())
        .collect()
}

/// Every target the implementations allow, narrowed by the spec's `targets:` when set.
pub fn derive(spec: &Spec) -> Vec<Target> {
    Target::ALL.into_iter().filter(|t| missing(spec, t.needs()).is_empty()).collect()
}

/// The targets the generators should produce: derived, then narrowed by `targets:`.
pub fn effective(spec: &Spec) -> Vec<Target> {
    let possible = derive(spec);
    match &spec.targets {
        Some(wanted) => possible.into_iter().filter(|t| wanted.contains(t)).collect(),
        None => possible,
    }
}
