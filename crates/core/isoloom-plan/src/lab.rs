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
        (!refusals.is_empty()).then(|| self.source.spans.place(refusals.remove(0)))
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

    /// Problems placed and in file order.
    fn sorted(&self, problems: Problems) -> Problems {
        let mut placed: Problems = problems.into_iter().map(|p| self.source.spans.place(p)).collect();
        placed.sort_by_key(|p| p.span.map(|s| (s.line, s.column)));
        placed
    }
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

/// Whether lowered problem `p` only follows from a machine the authored spec's own checks
/// already refused (a catalogue service, a machine without its OS): it would say the same thing
/// less clearly.
fn follows_from(found: &[Problem], p: &Problem, source: &syntax::Spec) -> bool {
    let refused: Vec<&str> = source
        .machines
        .keys()
        .filter(|m| found.iter().any(|f| f.path.within(&FieldPath::new("machines").key(m))))
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
