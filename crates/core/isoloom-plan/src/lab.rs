//! A lab: a spec as its author wrote it, ready to be planned for any target. It lowers the spec
//! once per target (with the runner's image table, when one is given), says which targets can
//! run it, and validates it: the authored spec's own checks, then those of its lowered form, each
//! problem reported at the field the author wrote.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use isoloom_catalogue::Catalogue;
use isoloom_spec::images::Table;
use isoloom_spec::model::{self, Target};
use isoloom_spec::syntax::{self, Code, FieldPath, Problem, Problems, ProjectPath};

use crate::lower::{Lowered, Scope, lower};

mod steps;
mod templates;

/// An authored spec, ready to plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Lab {
    source: syntax::Spec,
    images: Table,
    templates: templates::Texts,
    catalogue: &'static Catalogue,
    memo: Memo,
}

/// The catalogue built into Isoloom, or an empty one when it doesn't load (its own tests rule
/// that out; [`Lab::problems`] says so at each service it would have resolved).
fn built_in() -> &'static Catalogue {
    static EMPTY: OnceLock<Catalogue> = OnceLock::new();
    Catalogue::embedded().unwrap_or_else(|_| EMPTY.get_or_init(Catalogue::default))
}

/// [`Scope::Lab`], then one scope per target of [`Target::ALL`].
const SCOPES: usize = Target::ALL.len() + 1;

/// What a lab works out from its spec, once each: generating reads them many times (the
/// resolved snapshot asks every target's refusal, for each target generated). Neither depends on
/// the image table, so [`Lab::with_images`] keeps them.
#[derive(Debug, Clone, Default)]
struct Memo {
    /// The spec lowered for each scope, before the image table: the lab's, then each target's.
    lowered: [OnceLock<Lowered>; SCOPES],
    /// [`Lab::problems`].
    problems: OnceLock<Problems>,
}

impl PartialEq for Memo {
    /// A memo is no part of a lab's value: two labs of the same spec are equal, whatever each
    /// has worked out so far.
    fn eq(&self, _: &Memo) -> bool {
        true
    }
}

/// The memo's slot of `scope`.
fn slot(scope: Scope) -> Option<usize> {
    match scope {
        Scope::Lab => Some(0),
        Scope::Target(t) => Target::ALL.iter().position(|x| *x == t).map(|i| i + 1),
    }
}

impl Lab {
    /// The lab of an authored spec.
    #[must_use]
    pub fn new(source: syntax::Spec) -> Lab {
        Lab {
            source,
            images: Table::default(),
            templates: templates::Texts::new(),
            catalogue: built_in(),
            memo: Memo::default(),
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

    /// The lab with the texts of its project's templates (read by the caller that generates,
    /// from [`Lab::template_paths`]): the renderers install them filled in. A template whose text
    /// isn't given fails the machine's configuration where it is applied, saying so.
    #[must_use]
    pub fn with_templates(mut self, texts: BTreeMap<ProjectPath, String>) -> Lab {
        self.templates = texts;
        self
    }

    /// The project files the lab's steps fill in as templates, each once.
    #[must_use]
    pub fn template_paths(&self) -> Vec<ProjectPath> {
        templates::paths(&self.source)
    }

    /// `spec` with the image table and the templates' texts applied.
    fn finish(&self, spec: &model::Spec) -> model::Spec {
        templates::add(self.images.apply(spec), &self.templates)
    }

    /// The spec as written.
    #[must_use]
    pub fn source(&self) -> &syntax::Spec {
        &self.source
    }

    /// The catalogue its services resolve in: Isoloom's own.
    #[must_use]
    pub fn catalogue(&self) -> &'static Catalogue {
        self.catalogue
    }

    /// The spec lowered for `scope`, the image table applied.
    #[must_use]
    pub fn lowered(&self, scope: Scope) -> Lowered {
        let mut l = self.raw(scope).into_owned();
        l.spec = self.finish(&l.spec);
        l
    }

    /// The spec lowered for `scope` before the image table, lowered once.
    fn raw(&self, scope: Scope) -> Cow<'_, Lowered> {
        match slot(scope).and_then(|i| self.memo.lowered.get(i)) {
            Some(cell) => Cow::Borrowed(cell.get_or_init(|| lower(&self.source, self.catalogue, scope))),
            None => Cow::Owned(lower(&self.source, self.catalogue, scope)),
        }
    }

    /// The spec as `target` runs it.
    #[must_use]
    pub fn for_target(&self, target: Target) -> model::Spec {
        self.finish(&self.raw(Scope::Target(target)).spec)
    }

    /// The spec as the lab itself: every form, no target's own facts.
    #[must_use]
    pub fn spec(&self) -> model::Spec {
        self.finish(&self.raw(Scope::Lab).spec)
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
            .filter(|t| crate::lower::missing_form(&self.source, self.catalogue, *t).is_none())
            .filter(|t| *t != Target::Hybrid || isoloom_spec::targets::mixed(&self.spec()))
            .filter(|t| self.source.targets.as_ref().is_none_or(|wanted| wanted.contains(t)))
            .collect()
    }

    /// Why `target` can't run the lab, found while lowering: the first machine it has no form
    /// for, or a VM step it doesn't render yet, as a `refused` problem at the field.
    #[must_use]
    pub fn refusal(&self, target: Target) -> Option<Problem> {
        let first = self.raw(Scope::Target(target)).refusals.first().cloned();
        first.map(|p| self.place(p))
    }

    /// The spec's problems: its own, then its lowered form's at the fields the author wrote, in
    /// file order.
    #[must_use]
    pub fn problems(&self) -> Problems {
        self.memo.problems.get_or_init(|| self.find_problems()).clone()
    }

    /// [`Lab::problems`], worked out.
    fn find_problems(&self) -> Problems {
        let mut out = syntax::check(&self.source);
        if let Err(broken) = Catalogue::embedded() {
            add(
                &mut out,
                self.services()
                    .map(|at| Problem::new(Code::Refused, at, format!("this Isoloom's catalogue doesn't load: {broken}"))),
            );
        }
        let lowered = self.raw(Scope::Lab);
        // The catalogue's problems (and the entries' ports and data a machine conflicts with)
        // say what is wrong with a service machine better than its lowered form would.
        add(&mut out, lowered.problems.iter().cloned());
        let model: Vec<Problem> = isoloom_spec::validate(&lowered.spec)
            .iter()
            .map(|p| translated(&lowered, p))
            .filter(|p| !follows_from(&out, p, &self.source))
            .collect();
        add(&mut out, model);
        // What only a target's overrides set (an existing network or volume) is checked once
        // lowered for that target.
        for (key, target) in override_targets(&self.source.overrides) {
            let on_target = self.raw(Scope::Target(target));
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
        let lowered = self.raw(Scope::Lab);
        add(
            &mut out,
            isoloom_spec::validate_files(&lowered.spec, dir).iter().map(|p| translated(&lowered, p)),
        );
        add(&mut out, steps::step_files(&self.source, dir));
        add(&mut out, steps::setting_files(&self.source, self.catalogue, dir));
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
    /// The `service` field of each machine that runs catalogue services.
    fn services(&self) -> impl Iterator<Item = FieldPath> + '_ {
        self.source
            .machines
            .iter()
            .filter(|(_, m)| matches!(m.kind, syntax::Kind::Service(_)))
            .map(|(name, _)| FieldPath::new("machines").key(name).key("service"))
    }

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

/// The fields that say what a machine is: a problem there leaves the lowered machine a guess,
/// so what lowering finds under it isn't shown.
const FORM_FIELDS: [&str; 4] = ["os", "service", "with", "app"];

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
    Problem::new(p.code, path, p.message.clone())
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use isoloom_spec::model::Target;

    use super::*;
    use crate::testing;

    /// A lab each target lowers its own way: an existing network on Docker and on Proxmox.
    const LAB: &str = "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 }, corp: { cidr: 10.8.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10, corp: 10 }, os: debian@12, app: { image: nginx, steps: [{ run: a.sh }] } }\noverrides: { docker: { networks: { corp: { external: corp } } }, proxmox: { networks: { corp: { external: vmbr9 } } } }\n";

    #[test]
    fn every_scope_is_lowered_once_and_kept() {
        let lab = testing::lab(LAB);
        let scopes = std::iter::once(Scope::Lab).chain(Target::ALL.into_iter().map(Scope::Target));
        for scope in scopes {
            assert!(matches!(lab.raw(scope), Cow::Borrowed(_)), "{scope:?}");
            assert_eq!(
                lab.raw(scope).spec,
                lower(lab.source(), lab.catalogue(), scope).spec,
                "{scope:?}: its own lowering"
            );
        }
    }

    #[test]
    fn two_labs_of_one_spec_are_equal_whatever_each_has_worked_out() {
        let (a, b) = (testing::lab(LAB), testing::lab(LAB));
        assert_eq!(a.problems(), []);
        assert_eq!(a, b);
        assert_ne!(a, testing::lab(&LAB.replace("lan: 10,", "lan: 11,")));
    }

    #[test]
    fn an_existing_network_is_checked_under_the_override_that_names_it() {
        let lab = testing::lab(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12 }\noverrides:\n  proxmox: { networks: { lan: { external: 'vmbr 1' } } }\n  docker: { networks: { lan: { external: 'corp net' } } }\n",
        );
        let paths: Vec<String> = lab.problems().iter().map(|p| p.path.to_string()).collect();
        assert!(paths.contains(&"overrides.proxmox.networks.lan.external".to_owned()), "{paths:?}");
        assert!(paths.contains(&"overrides.docker.networks.lan.external".to_owned()), "{paths:?}");
    }

    #[test]
    fn a_machine_the_authored_checks_refuse_shows_nothing_lowering_finds_under_it() {
        // `db` has no OS: the authored check says so, and what its lowered form lacks isn't shown.
        let lab = testing::lab(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  db: { networks: { lan: 10 }, app: { steps: [{ run: a.sh }] }, depends_on: [ghost] }\n  web: { networks: { lan: 11 }, app: { image: nginx }, depends_on: [nowhere] }\n",
        );
        let paths: Vec<String> = lab.problems().iter().map(|p| p.path.to_string()).collect();
        assert!(paths.contains(&"machines.db.os".to_owned()), "{paths:?}");
        assert!(!paths.iter().any(|p| p.starts_with("machines.db.depends_on")), "{paths:?}");
        assert!(
            paths.iter().any(|p| p.starts_with("machines.web.depends_on")),
            "an unrefused machine's: {paths:?}"
        );
    }

    #[test]
    fn a_secret_no_machine_gets_is_a_problem_unless_a_catalogue_service_may_read_it() {
        let paths = |machines: &str| -> Vec<String> {
            let lab = testing::lab(&format!("version: 1\nname: t\nsecrets: {{ db: {{ env: DB }} }}\nmachines:\n{machines}"));
            lab.problems().iter().map(|p| p.path.to_string()).collect()
        };
        assert!(paths("  web: { app: { image: nginx } }\n").contains(&"secrets.db".to_owned()));
        let with_service = paths("  db: { os: debian@12, service: postgres }\n");
        assert!(!with_service.iter().any(|p| p.starts_with("secrets.")), "{with_service:?}");
    }

    #[test]
    fn a_copy_step_takes_a_file_of_the_project_that_exists() {
        let dir = std::env::temp_dir().join(format!("isoloom-plan-copy-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("site")).expect("a project folder");
        let lab = testing::lab(
            "version: 1\nname: t\nmachines:\n  web: { os: debian@12, app: { steps: [{ copy: { from: site, to: /srv } }, { copy: { from: missing, to: /m } }] } }\n",
        );
        let found: Vec<(String, String)> = steps::step_files(lab.source(), &dir)
            .into_iter()
            .map(|p| (p.path.to_string(), p.reason))
            .collect();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(
            found,
            [(
                "machines.web.app.steps[1].copy.from".to_owned(),
                "`missing` doesn't exist in the project folder".to_owned()
            )]
        );
        // A path outside the project is refused when the spec is read.
        for outside in ["/etc/x", "a/../../b"] {
            let refused = Lab::parse(&format!(
                "version: 1\nname: t\nmachines:\n  web: {{ os: debian@12, app: {{ steps: [{{ copy: {{ from: {outside}, to: /x }} }}] }} }}\n"
            ))
            .expect_err("outside the project");
            assert_eq!(refused[0].path.as_str(), "machines.web.app.steps[0].copy.from", "{outside}");
        }
    }
}
