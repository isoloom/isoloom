//! Lowering: the spec as an author writes it ([`isoloom_spec::syntax`]) into the form the plan
//! and the backends read ([`isoloom_spec::model`]), once per target. A machine keeps every form
//! it has (each target picks the one it runs, both on `hybrid`); the overrides of the target
//! being lowered fill in its platform facts; and a machine the target can't run is a refusal
//! naming the field that rules it out (`machines.web.app.build`), never a silent stand-in.
//!
//! Lowered for the environment as a whole ([`Scope::Lab`]), no target's own facts apply: that is the
//! spec validation, the snapshot and the derived targets read.

mod carry;
mod forms;
mod machine;
mod overrides;
mod refusals;
mod service;
mod trail;

pub use trail::Trail;

use indexmap::IndexMap;
use isoloom_catalogue::Catalogue;
use isoloom_spec::model::{self, Controller, Group, SecretSource, Target, VmImage};
use isoloom_spec::syntax::{self, Problem};

/// What a lowering is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The environment itself: every form, no target's own facts.
    Lab,
    /// One target: its overrides applied, its refusals found.
    Target(Target),
}

impl Scope {
    /// The target, when lowering for one.
    #[must_use]
    pub fn target(self) -> Option<Target> {
        match self {
            Scope::Lab => None,
            Scope::Target(t) => Some(t),
        }
    }
}

/// A spec lowered: the model the plan builds from, what the target refuses, and where each
/// lowered field came from.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Lowered {
    /// The spec in the form the backends read.
    pub spec: model::Spec,
    /// The machines (or steps) the target can't run, as `refused` problems at the field that
    /// rules each out. Empty for [`Scope::Lab`].
    pub refusals: Vec<Problem>,
    /// Lowered field paths back to the written ones.
    pub trail: Trail,
    /// What lowering couldn't lower, at the field the author wrote: VLANs that don't fit their
    /// LAN, a port without its number, a healthcheck on a port the machine doesn't name. The
    /// spec's own checks report the last two as well; on a valid spec, only the first can come.
    pub problems: Vec<Problem>,
}

/// What lowering reads besides a machine: the spec, the scope, and the environment's catalogue services,
/// resolved once.
pub(super) struct Lowering<'a> {
    source: &'a syntax::Spec,
    scope: Scope,
    services: &'a service::Services,
}

/// The spec's inputs, each with the sensitivity it is declared with.
fn inputs(source: &syntax::Spec) -> Vec<model::Input> {
    let input = |(name, i): (&syntax::InputName, &syntax::Input)| model::Input {
        name: name.to_string(),
        sensitive: i.sensitive,
    };
    source.inputs.0.iter().map(input).collect()
}

/// Lowers `source` for `scope`, its catalogue services resolved in `catalogue`.
#[must_use]
pub fn lower(source: &syntax::Spec, catalogue: &Catalogue, scope: Scope) -> Lowered {
    let mut trail = Trail::default();
    let mut secrets = source.secrets.clone();
    let mut machines = IndexMap::new();
    let services = service::Services::of(catalogue, source);
    let mut problems = services.problems();
    let l = Lowering {
        source,
        scope,
        services: &services,
    };
    for (name, m) in &source.machines {
        let lowered = machine::lower(&l, name, m, &mut trail);
        for (secret, input) in lowered.inputs {
            secrets.insert(secret, SecretSource::Input { input });
        }
        problems.extend(lowered.problems);
        machines.insert(name.clone(), lowered.machine);
    }
    let mut spec = model::Spec::new(source.name.clone());
    spec.networks = overrides::networks(source, scope);
    spec.reach.clone_from(&source.reach);
    spec.dns.clone_from(&source.dns);
    spec.inputs = inputs(source);
    spec.secrets = secrets;
    spec.machines = machines;
    spec.cloud.clone_from(&source.cloud);
    spec.volumes = overrides::volumes(source, scope);
    spec.constraints.clone_from(&source.constraints);
    spec.groups = groups(source);
    spec.provision.clone_from(&source.provision);
    spec.controller = controller(source);
    spec.checks.clone_from(&source.checks);
    spec.targets.clone_from(&source.targets);
    spec.message.clone_from(&source.message);
    spec.tags.clone_from(&source.tags);
    spec.tools.clone_from(&source.tools);
    spec.clones.clone_from(&source.clones);
    machine::resolve_dns(&mut spec, scope);
    let refusals = scope.target().map(|t| refusals::of(source, &services, t)).unwrap_or_default();
    trail.inputs(source);
    // VLANs written under their LAN become networks of their own, as the backends expect.
    let spec = match isoloom_spec::vlans::flatten(spec.clone()) {
        Ok(flat) => flat,
        Err(refused) => {
            problems.push(refused);
            spec
        }
    };
    Lowered {
        spec,
        refusals,
        trail,
        problems,
    }
}

/// The first machine `target` has no form for, as a `refused` problem at the field that rules it
/// out: the target can't run the environment at all (its catalogue services resolved in `catalogue`).
#[must_use]
pub fn missing_form(source: &syntax::Spec, catalogue: &Catalogue, target: Target) -> Option<Problem> {
    refusals::forms(source, &service::Services::of(catalogue, source), target).into_iter().next()
}

/// The groups, by their members (their shared fields are already in the machines).
fn groups(source: &syntax::Spec) -> IndexMap<String, Group> {
    source
        .groups
        .iter()
        .map(|(n, g)| {
            (
                n.clone(),
                Group {
                    members: g.members.clone(),
                    ..Group::default()
                },
            )
        })
        .collect()
}

/// The controller, with its Vagrant box from `overrides.vagrant.controller`.
fn controller(source: &syntax::Spec) -> Option<Controller> {
    let vagrant = source.overrides.vagrant.as_ref().and_then(|v| v.controller.as_ref());
    let image = vagrant.filter(|b| b.box_name.is_some() || b.box_version.is_some()).map(|b| {
        let mut image = VmImage::default();
        image.vagrant.clone_from(&b.box_name);
        image.vagrant_version.clone_from(&b.box_version);
        image
    });
    match (&source.controller, image) {
        (None, None) => None,
        (c, image) => {
            let mut c = c.clone().unwrap_or_default();
            if image.is_some() {
                c.image = image;
            }
            Some(c)
        }
    }
}

#[cfg(test)]
mod tests;
