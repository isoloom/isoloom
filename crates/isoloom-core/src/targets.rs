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
            Shape::Either => m.docker.is_none() && m.vm.is_none(),
            Shape::External => m.external.is_none(),
            // Cloud services aren't machines: see `derive`.
            Shape::Cloud => false,
        })
        .map(|(n, _)| n.clone())
        .collect()
}

/// Every target the implementations allow, narrowed by the spec's `targets:` when set.
pub fn derive(spec: &Spec) -> Vec<Target> {
    // Cloud services run on their own target only; a spec without machines runs nowhere else.
    if spec.cloud.is_some() {
        return vec![Target::CloudServices];
    }
    if spec.machines.is_empty() {
        return Vec::new();
    }
    Target::ALL
        .into_iter()
        .filter(|t| *t != Target::CloudServices)
        .filter(|t| missing(spec, t.needs()).is_empty())
        .filter(|t| *t != Target::Hybrid || mixed(spec))
        .collect()
}

/// Hybrid is worth it only when the environment mixes both: a machine that can only be a VM
/// (Windows, say) and one that can be a container. Otherwise Docker or VMs alone run it.
pub fn mixed(spec: &Spec) -> bool {
    let own = || spec.machines.values().filter(|m| !m.access);
    own().any(|m| m.docker.is_some()) && own().any(|m| m.docker.is_none() && m.vm.is_some())
}

/// The targets the generators should produce: derived, then narrowed by `targets:`.
pub fn effective(spec: &Spec) -> Vec<Target> {
    let possible = derive(spec);
    match &spec.targets {
        Some(wanted) => possible.into_iter().filter(|t| wanted.contains(t)).collect(),
        None => possible,
    }
}
