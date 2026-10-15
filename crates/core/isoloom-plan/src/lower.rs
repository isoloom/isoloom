//! Lowering: the spec as an author writes it ([`isoloom_spec::syntax`]) into the form the plan
//! and the backends read ([`isoloom_spec::model`]), once per target. A machine keeps every form
//! it has (each target picks the one it runs, both on `hybrid`); the overrides of the target
//! being lowered fill in its platform facts; and a machine the target can't run is a refusal
//! naming the field that rules it out (`machines.web.app.build`), never a silent stand-in.
//!
//! Lowered for the lab as a whole ([`Scope::Lab`]), no target's own facts apply: that is the
//! spec validation, the snapshot and the derived targets read.

mod machine;
mod overrides;
mod refusals;
mod trail;

pub use trail::Trail;

use indexmap::IndexMap;
use isoloom_spec::model::{self, Controller, Group, SecretSource, Target, VmImage};
use isoloom_spec::syntax::{self, Problem};

/// What a lowering is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The lab itself: every form, no target's own facts.
    Lab,
    /// One target: its overrides applied, its refusals found.
    Target(Target),
}

impl Scope {
    /// The target, when lowering for one.
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
pub struct Lowered {
    /// The spec in the form the backends read.
    pub spec: model::Spec,
    /// The machines (or steps) the target can't run, as `refused` problems at the field that
    /// rules each out. Empty for [`Scope::Lab`].
    pub refusals: Vec<Problem>,
    /// Lowered field paths back to the written ones.
    pub trail: Trail,
    /// What only lowering finds wrong (VLANs that don't fit their LAN), as spec problems.
    pub problems: Vec<Problem>,
}

/// Lowers `source` for `scope`.
#[must_use]
pub fn lower(source: &syntax::Spec, scope: Scope) -> Lowered {
    let mut trail = Trail::default();
    let mut secrets = source.secrets.clone();
    let mut machines = IndexMap::new();
    for (name, m) in &source.machines {
        let lowered = machine::lower(source, name, m, scope, &mut trail);
        for (secret, input) in lowered.inputs {
            secrets.insert(secret, SecretSource::Input { input });
        }
        machines.insert(name.clone(), lowered.machine);
    }
    let mut spec = model::Spec {
        version: 1,
        name: source.name.clone(),
        networks: overrides::networks(source, scope),
        reach: source.reach.clone(),
        dns: source.dns.clone(),
        inputs: source.inputs.0.keys().map(ToString::to_string).collect(),
        plain_inputs: source.inputs.0.iter().filter(|(_, i)| !i.sensitive).map(|(name, _)| name.to_string()).collect(),
        secrets,
        machines,
        cloud: source.cloud.clone(),
        volumes: overrides::volumes(source, scope),
        constraints: source.constraints.clone(),
        common: None,
        groups: groups(source),
        provision: source.provision.clone(),
        controller: controller(source),
        checks: source.checks.clone(),
        targets: source.targets.clone(),
        message: source.message.clone(),
        tools: source.tools.clone(),
        clones: source.clones.clone(),
    };
    machine::resolve_dns(&mut spec, scope);
    let refusals = scope.target().map(|t| refusals::of(source, t)).unwrap_or_default();
    trail.inputs(source);
    // VLANs written under their LAN become networks of their own, as the backends expect.
    let (spec, problems) = match isoloom_spec::vlans::flatten(spec.clone()) {
        Ok(flat) => (flat, Vec::new()),
        Err(why) => (spec, vec![stage_problem(&why)]),
    };
    Lowered {
        spec,
        refusals,
        trail,
        problems,
    }
}

/// The first machine `target` has no form for, as a `refused` problem at the field that rules it
/// out: the target can't run the lab at all.
#[must_use]
pub fn missing_form(source: &syntax::Spec, target: Target) -> Option<Problem> {
    refusals::forms(source, target).into_iter().next()
}

/// A lowering step's refusal (`networks.office.vlans.10: ...`), at the field it names.
fn stage_problem(why: &str) -> Problem {
    let (path, reason) = why.split_once(": ").filter(|(p, _)| !p.contains(' ')).unwrap_or(("", why));
    Problem::new(syntax::Code::InvalidValue, syntax::FieldPath::new(path), reason)
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
    let image = vagrant.filter(|b| b.box_name.is_some() || b.box_version.is_some()).map(|b| VmImage {
        vagrant: b.box_name.clone(),
        vagrant_version: b.box_version.clone(),
        ..VmImage::default()
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
