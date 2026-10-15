//! A lab: a spec as its author wrote it, ready to be planned for any target. It lowers the spec
//! once per target (with the runner's image table, when one is given), says which targets can
//! run it, and validates it: the authored spec's own checks, then those of its lowered form, each
//! problem reported at the field the author wrote.

use std::path::Path;

use isoloom_spec::images::Table;
use isoloom_spec::model::{self, Target};
use isoloom_spec::syntax::{self, Code, FieldPath, Problem, Problems};

use crate::lower::{Lowered, Scope, lower};

/// An authored spec, ready to plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Lab {
    source: syntax::Spec,
    images: Table,
}

impl Lab {
    /// The lab of an authored spec.
    #[must_use]
    pub fn new(source: syntax::Spec) -> Lab {
        Lab {
            source,
            images: Table::default(),
        }
    }

    /// The lab of `yaml`, written in the authored format.
    ///
    /// # Errors
    ///
    /// The problems that stop the parser, coded and placed.
    pub fn parse(yaml: &str) -> Result<Lab, Problems> {
        syntax::parse(yaml).map(Lab::new)
    }

    /// The lab with the runner's image table: each VM's image from it unless the spec's
    /// overrides give one, and the access machine the spec leaves to the runner.
    #[must_use]
    pub fn with_images(mut self, images: Table) -> Lab {
        self.images = images;
        self
    }

    /// The spec as written.
    #[must_use]
    pub fn source(&self) -> &syntax::Spec {
        &self.source
    }

    /// The spec lowered for `scope`, the image table applied.
    #[must_use]
    pub fn lowered(&self, scope: Scope) -> Lowered {
        let mut l = lower(&self.source, scope);
        l.spec = self.images.apply(&l.spec);
        l
    }

    /// The spec as `target` runs it.
    #[must_use]
    pub fn for_target(&self, target: Target) -> model::Spec {
        self.lowered(Scope::Target(target)).spec
    }

    /// The spec as the lab itself: every form, no target's own facts.
    #[must_use]
    pub fn spec(&self) -> model::Spec {
        self.lowered(Scope::Lab).spec
    }

    /// The targets every machine has the form for, narrowed by the spec's `targets:`.
    #[must_use]
    pub fn possible(&self) -> Vec<Target> {
        if self.source.cloud.is_some() {
            return vec![Target::CloudServices];
        }
        if self.source.machines.is_empty() {
            return Vec::new();
        }
        Target::ALL
            .into_iter()
            .filter(|t| *t != Target::CloudServices)
            .filter(|t| crate::lower::missing_form(&self.source, *t).is_none())
            .filter(|t| *t != Target::Hybrid || isoloom_spec::targets::mixed(&self.spec()))
            .filter(|t| self.source.targets.as_ref().is_none_or(|wanted| wanted.contains(t)))
            .collect()
    }

    /// Why `target` can't run the lab, found while lowering: the first machine it has no form
    /// for, or a VM step it doesn't render yet, as a `refused` problem at the field.
    #[must_use]
    pub fn refusal(&self, target: Target) -> Option<Problem> {
        let mut refusals = self.lowered(Scope::Target(target)).refusals;
        (!refusals.is_empty()).then(|| self.place(refusals.remove(0)))
    }

    /// The spec's problems: its own, then its lowered form's at the fields the author wrote, in
    /// file order.
    #[must_use]
    pub fn problems(&self) -> Problems {
        let mut out = syntax::check(&self.source);
        let lowered = lower(&self.source, Scope::Lab);
        let model: Vec<Problem> = isoloom_spec::validate(&lowered.spec)
            .iter()
            .map(|p| translated(&lowered, p))
            .filter(|p| !follows_from(&out, p, &self.source))
            .collect();
        add(&mut out, lowered.problems.into_iter().chain(model));
        // What only a target's overrides set (an existing network or volume) is checked once
        // lowered for that target.
        for (key, target) in override_targets(&self.source.overrides) {
            let on_target = lower(&self.source, Scope::Target(target));
            let found: Vec<Problem> = isoloom_spec::validate(&on_target.spec)
                .iter()
                .map(|p| under_overrides_of(key, translated(&on_target, p)))
                .filter(|p| !follows_from(&out, p, &self.source))
                .collect();
            add(&mut out, found);
        }
        self.sorted(out)
    }

    /// [`Lab::problems`], and the project paths the spec names that don't exist in `dir`.
    #[must_use]
    pub fn problems_with_files(&self, dir: &Path) -> Problems {
        let mut out = self.problems();
        let lowered = lower(&self.source, Scope::Lab);
        add(
            &mut out,
            isoloom_spec::validate_files(&lowered.spec, dir).iter().map(|p| translated(&lowered, p)),
        );
        add(&mut out, copied_files(&self.source, dir));
        self.sorted(out)
    }

    /// Problems placed and in file order: a clone's at its counted machine.
    fn sorted(&self, problems: Problems) -> Problems {
        let mut placed: Problems = problems.into_iter().map(|p| self.place(p)).collect();
        placed.sort_by_key(|p| p.span.map(|s| (s.line, s.column)));
        placed
    }
}

impl Lab {
    /// `p` placed, a clone's field (`machines.ws-01.os`) at its counted machine's (`machines.ws.os`).
    fn place(&self, mut p: Problem) -> Problem {
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

/// Every problem an author sees in `yaml`: the parser's when it can't read it, else the lab's
/// (what `isoloom validate` shows, files aside).
#[must_use]
pub fn problems_in(yaml: &str) -> Problems {
    Lab::parse(yaml).map_or_else(|refused| refused, |lab| lab.problems())
}

/// `found` added to `out`, unless a problem at the same field is already there (the authored
/// spec's own check says it better).
fn add(out: &mut Problems, found: impl IntoIterator<Item = Problem>) {
    for p in found {
        if !out.iter().any(|o| o.path == p.path) {
            out.push(p);
        }
    }
}

/// The targets whose `overrides` block the spec writes and validation must see: each with the
/// block's key.
fn override_targets(o: &syntax::Overrides) -> Vec<(&'static str, Target)> {
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
fn under_overrides_of(key: &str, mut p: Problem) -> Problem {
    if let Some(rest) = p.path.as_str().strip_prefix("overrides.docker.")
        && key != "docker"
    {
        p.path = FieldPath::new(format!("overrides.{key}.{rest}"));
    }
    p
}

/// The fields that say what a machine is: an authored problem there leaves the lowered machine
/// a guess, so what lowering finds under it isn't shown.
const FORM_FIELDS: [&str; 3] = ["os", "service", "app"];

/// Whether lowered problem `p` only follows from a machine the authored spec's own checks
/// already refused (a catalogue service, a machine without its OS): it would say the same thing
/// less clearly. A problem elsewhere on the machine (a bad variable name) refuses nothing: its
/// other fields are still checked.
fn follows_from(found: &[Problem], p: &Problem, source: &syntax::Spec) -> bool {
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
fn translated(lowered: &Lowered, p: &isoloom_spec::Problem) -> Problem {
    let path = lowered.trail.path(&p.at);
    Problem::new(code_of(&p.message), path, p.message.clone())
}

/// The code a lowered spec's problem has, from what it says.
fn code_of(message: &str) -> Code {
    const UNKNOWN: &[&str] = &[
        "no network named",
        "no machine named",
        "names no machine",
        "isn't a machine",
        "isn't declared",
        "no such",
        "no secret named",
        "no input named",
    ];
    if UNKNOWN.iter().any(|u| message.contains(u)) {
        Code::UnknownReference
    } else {
        Code::InvalidValue
    }
}

/// The project files `copy` steps name, each present in `dir`.
fn copied_files(source: &syntax::Spec, dir: &Path) -> Vec<Problem> {
    let mut out = Vec::new();
    for (name, m) in &source.machines {
        let Some(app) = m.app() else { continue };
        for (i, step) in app.steps.iter().enumerate() {
            if let syntax::Step::Copy(c) = step {
                let at = FieldPath::new("machines").key(name).key("app").key("steps").index(i).key("copy").key("from");
                if c.from.starts_with('/') || c.from.split('/').any(|s| s == "..") {
                    out.push(Problem::new(
                        Code::InvalidValue,
                        at,
                        format!("`{}` must be a path inside the project folder", c.from),
                    ));
                } else if !dir.join(&c.from).exists() {
                    out.push(Problem::new(
                        Code::InvalidValue,
                        at,
                        format!("`{}` doesn't exist in the project folder", c.from),
                    ));
                }
            }
        }
    }
    out
}
