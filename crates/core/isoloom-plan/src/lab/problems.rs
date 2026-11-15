//! Where a lab's problems are reported: each at the field the author wrote (a counted machine's,
//! an override block's), once, and none that only follows from a machine already refused.

use isoloom_spec::model::Target;
use isoloom_spec::syntax::{self, FieldPath, Problem, Problems};

use super::Lab;
use crate::lower::Lowered;

impl Lab {
    /// The `service` field of each machine that runs catalogue services.
    pub(super) fn services(&self) -> impl Iterator<Item = FieldPath> + '_ {
        self.source
            .machines
            .iter()
            .filter(|(_, m)| matches!(m.kind, syntax::Kind::Service(_)))
            .map(|(name, _)| FieldPath::new("machines").key(name).key("service"))
    }

    /// `p` placed, a clone's field (`machines.ws-01.os`) at its counted machine's (`machines.ws.os`).
    pub(super) fn place(&self, mut p: Problem) -> Problem {
        if p.span.is_none()
            && let Some((base, clone)) = self
                .source
                .clones
                .iter()
                .find_map(|(b, c)| c.iter().find(|n| p.path.within(&FieldPath::new("machines").key(n))).map(|n| (b, n)))
        {
            let rest = p.path.as_str().strip_prefix(&format!("machines.{clone}")).unwrap_or_default();
            p.span = self.source.spans.find(&FieldPath::new(format!("machines.{base}{rest}")));
        }
        self.source.spans.place(p)
    }
}

/// `found` added to `out`, unless a problem at the same field is already there (the authored
/// spec's own check says it better).
pub(super) fn add(out: &mut Problems, found: impl IntoIterator<Item = Problem>) {
    for p in found {
        if !out.iter().any(|o| o.path == p.path) {
            out.push(p);
        }
    }
}

/// The targets whose `overrides` block the spec writes and validation must see: each with the
/// block's key.
pub(super) fn override_targets(o: &syntax::Overrides) -> Vec<(&'static str, Target)> {
    [
        ("docker", o.docker.is_some(), Target::Docker),
        ("kubernetes", o.kubernetes.is_some(), Target::Kubernetes),
        ("proxmox", o.proxmox.is_some(), Target::Proxmox),
        ("esxi", o.esxi.is_some(), Target::Esxi),
    ]
    .into_iter()
    .filter(|(_, written, _)| *written)
    .map(|(key, _, target)| (key, target))
    .collect()
}

/// `p` at the override block it came from: the trail names Docker's for an existing network or
/// volume, which every other target writes under its own key.
pub(super) fn under_overrides_of(key: &str, mut p: Problem) -> Problem {
    if let Some(rest) = p.path.as_str().strip_prefix("overrides.docker.")
        && key != "docker"
    {
        p.path = FieldPath::new(format!("overrides.{key}.{rest}"));
    }
    p
}

/// The fields that say what a machine is: a problem there leaves the lowered machine a guess,
/// so what lowering finds under it isn't shown.
const FORM_FIELDS: [&str; 4] = ["os", "service", "with", "app"];

/// Whether lowered problem `p` only follows from a machine the authored spec's own checks
/// already refused (a catalogue service, a machine without its OS): it would say the same thing
/// less clearly. A problem elsewhere on the machine (a bad variable name) refuses nothing: its
/// other fields are still checked.
pub(super) fn follows_from(found: &[Problem], p: &Problem, source: &syntax::Spec) -> bool {
    let refused: Vec<&str> = source
        .machines
        .keys()
        .filter(|m| {
            let at = FieldPath::new("machines").key(m);
            found.iter().any(|f| f.path == at || FORM_FIELDS.iter().any(|k| f.path.within(&at.key(k))))
        })
        .map(String::as_str)
        .collect();
    let services = source.machines.values().any(|m| matches!(m.kind, syntax::Kind::Service(_)));
    p.path.as_str() == "machines" && !refused.is_empty()
        || refused
            .iter()
            .any(|m| p.path.within(&FieldPath::new("machines").key(m)) || p.reason.contains(&format!("`{m}`")))
        || services && p.path.as_str().starts_with("secrets.")
}

/// A lowered spec's problem, at the field the author wrote.
pub(super) fn translated(lowered: &Lowered, p: &isoloom_spec::Problem) -> Problem {
    let path = lowered.trail.path(&p.at);
    Problem::new(p.code, path, p.message.clone())
}
