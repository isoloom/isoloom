//! A lab: a spec as its author wrote it, ready to be planned for any target. It lowers the spec
//! once per target (with the runner's image table, when one is given), says which targets can
//! run it, and validates it: the authored spec's own checks, then those of its lowered form, each
//! problem reported at the field the author wrote.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::sync::OnceLock;

use isoloom_catalogue::Catalogue;
use isoloom_spec::host_access::{self, Grants, HostReach};
use isoloom_spec::images::Table;
use isoloom_spec::model::{self, Target};
use isoloom_spec::syntax::{self, Code, FieldPath, Problem, Problems, ProjectPath};

use crate::configure::DaemonTable;
use crate::lower::{Lowered, Scope, lower};

mod access;
mod dockerfiles;
mod problems;
mod steps;
mod templates;
mod warnings;

use problems::{add, follows_from, override_targets, translated, under_overrides_of};

/// A lab: an `isoloom.yml` project ready to generate, its authored spec with what it reads
/// besides (the catalogue, the image table, the clouds' regions, the template files). Each
/// target's [`Environment`](crate::Environment), the environment the spec describes as that
/// target runs it, is planned from it (`ARCHITECTURE.md`'s terms).
#[derive(Debug, Clone, PartialEq)]
pub struct Lab {
    source: syntax::Spec,
    images: Table,
    regions: BTreeMap<String, String>,
    templates: templates::Texts,
    dockerfiles: BTreeMap<ProjectPath, String>,
    bases: crate::dockerfile::Sources,
    host_access: Grants,
    loaded: Loaded,
    memo: Memo,
}

/// The catalogue a lab resolves its services in, read once when the lab is made, with what
/// planning reads of it: lowering, planning and the checks take it from here, never from the
/// global catalogue again.
#[derive(Debug, Clone, PartialEq)]
struct Loaded {
    catalogue: &'static Catalogue,
    /// Why the catalogue didn't load (`catalogue` is then empty).
    broken: Option<String>,
    /// The daemons `catalogue` knows the foreground form of.
    daemons: DaemonTable,
}

impl Loaded {
    /// The catalogue built into Isoloom, or an empty one when it doesn't load (its own tests rule
    /// that out; [`Lab::problems`] says so at each service it would have resolved).
    fn built_in() -> Loaded {
        static EMPTY: OnceLock<Catalogue> = OnceLock::new();
        let (catalogue, broken) = match Catalogue::embedded() {
            Ok(catalogue) => (catalogue, None),
            Err(broken) => (EMPTY.get_or_init(Catalogue::default), Some(broken.to_string())),
        };
        Loaded {
            catalogue,
            broken,
            daemons: DaemonTable::from(catalogue),
        }
    }
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
    /// The spec with the VM forms its Dockerfiles give, for the targets that run VMs.
    translated: OnceLock<dockerfiles::Translated>,
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
            regions: BTreeMap::new(),
            templates: templates::Texts::new(),
            dockerfiles: BTreeMap::new(),
            bases: crate::dockerfile::Sources::default(),
            host_access: Grants::none(),
            loaded: Loaded::built_in(),
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

    /// The lab with what whoever runs it allows it to reach of the computer that runs it
    /// ([`isoloom_spec::host_access`]): nothing by default, so a spec that reaches it in any way
    /// (a host folder outside the project or written to, the engine's socket, a privileged
    /// container, devices, boot files of the host, a bridged network) has a `refused` problem at
    /// each such field until its access is allowed here. [`Lab::host_access`] lists what to ask.
    #[must_use]
    pub fn with_host_access(mut self, grants: Grants) -> Lab {
        self.host_access = grants;
        self.memo.problems = OnceLock::new();
        self
    }

    /// Where the lab reaches the computer that runs it, as written and under each target's
    /// overrides, each field once: what a caller asks whoever runs it to allow before
    /// [`Lab::with_host_access`].
    #[must_use]
    pub fn host_access(&self) -> Vec<HostReach> {
        let mut out: Vec<HostReach> = Vec::new();
        let scopes = std::iter::once(Scope::Lab).chain(override_targets(&self.source.overrides).into_iter().map(|(_, t)| Scope::Target(t)));
        for scope in scopes {
            let lowered = self.raw(scope);
            for mut r in host_access::of(&lowered.spec) {
                lowered.trail.path(&r.at).as_str().clone_into(&mut r.at);
                if !out.iter().any(|o| o.at == r.at && o.access == r.access) {
                    out.push(r);
                }
            }
        }
        out
    }

    /// The lab with the region each cloud's module defaults to, by the cloud's id (`aws`), as the
    /// person's defaults set it (`isoloom_spec::defaults::Defaults::regions`): written into the
    /// module's `region` variable. A cloud left out keeps its own.
    #[must_use]
    pub fn with_regions(mut self, regions: BTreeMap<String, String>) -> Lab {
        self.regions = regions;
        self
    }

    /// `spec` (the lab as `target` runs it, [`Lab::for_target`], or a spec derived from it)
    /// planned for `target` as instance `instance`, with the lab's catalogue and regions: the one
    /// way to plan an environment. Hidden: it returns a building-block crate's type, outside the
    /// supported API `isoloom-targets` re-exports [`Lab`] in.
    ///
    /// # Errors
    ///
    /// [`PlanError::UnresolvedRecipe`](crate::environment::PlanError::UnresolvedRecipe) for a
    /// machine whose configuration has steps its OS's family can't carry, which lowering refuses
    /// for the target first; [`PlanError::Catalogue`](crate::environment::PlanError::Catalogue)
    /// when the catalogue built into Isoloom doesn't load, never a plan with an empty one.
    #[doc(hidden)]
    pub fn plan(&self, spec: &model::Spec, target: Target, instance: Option<u8>) -> Result<crate::Environment, crate::environment::PlanError> {
        // An empty catalogue would plan the lab's services as if it had none.
        if let Some(reason) = &self.loaded.broken {
            return Err(crate::environment::PlanError::Catalogue { reason: reason.clone() });
        }
        let mut env = crate::Environment::planned(spec, target, instance, self.loaded.catalogue, &self.loaded.daemons)?;
        env.regions.clone_from(&self.regions);
        Ok(env)
    }

    /// The lab with the texts of its project's templates (read by the caller that generates,
    /// from [`Lab::template_paths`]): the renderers install them filled in. A template whose text
    /// isn't given fails the machine's configuration where it is applied, saying so.
    #[must_use]
    pub fn with_templates(mut self, texts: BTreeMap<ProjectPath, String>) -> Lab {
        self.templates = texts;
        self
    }

    /// The lab with the texts of its builds' Dockerfiles (read by the caller that generates,
    /// from [`Lab::all_dockerfile_paths`]): on a target that runs VMs, each machine with no
    /// `app.steps` takes the VM form its Dockerfile translates to, or is refused at the line that
    /// has none.
    #[must_use]
    pub fn with_dockerfiles(mut self, texts: BTreeMap<ProjectPath, String>) -> Lab {
        self.dockerfiles = texts;
        self.memo = Memo::default();
        self
    }

    /// The lab with the base images vendored in its project (`isoloom bases fetch`), which a
    /// Dockerfile's translation starts `FROM` when the service matrix doesn't have them.
    #[must_use]
    pub fn with_bases(mut self, bases: crate::dockerfile::Sources) -> Lab {
        self.bases = bases;
        self.memo = Memo::default();
        self
    }

    /// The Dockerfiles the lab's VM forms are translated from: each build's whose machine has no
    /// `app.steps` of its own, each once.
    #[must_use]
    pub fn dockerfile_paths(&self) -> Vec<ProjectPath> {
        dockerfiles::paths(&self.source)
    }

    /// Every build's Dockerfile, each once: those of [`Lab::dockerfile_paths`], and those a
    /// machine's `app.steps` stand beside, which [`Lab::warnings`] reads to say whether the steps
    /// are needed. The caller that reads the Dockerfiles for [`Lab::with_dockerfiles`] reads these.
    #[must_use]
    pub fn all_dockerfile_paths(&self) -> Vec<ProjectPath> {
        dockerfiles::all_paths(&self.source)
    }

    /// The spec `scope` lowers: the translated one for a target that runs VMs.
    fn source_for(&self, scope: Scope) -> &syntax::Spec {
        match scope {
            Scope::Target(t) if dockerfiles::wants_vm(t) => &self.translated().source,
            _ => &self.source,
        }
    }

    /// The spec with the VM forms its Dockerfiles give, worked out once.
    fn translated(&self) -> &dockerfiles::Translated {
        self.memo
            .translated
            .get_or_init(|| dockerfiles::translate(&self.source, &self.dockerfiles, &self.bases))
    }

    /// `l` with each refusal of a Dockerfile that has no VM form saying why.
    fn explained(&self, scope: Scope, mut l: Lowered) -> Lowered {
        if matches!(scope, Scope::Target(t) if dockerfiles::wants_vm(t)) {
            let t = self.translated();
            l.refusals = l.refusals.into_iter().map(|p| t.explain(p)).collect();
        }
        l
    }

    /// The project files the lab's steps fill in as templates, each once.
    #[must_use]
    pub fn template_paths(&self) -> Vec<ProjectPath> {
        templates::paths(&self.source)
    }

    /// `spec` with the image table and the templates' texts applied: the project's, and those
    /// the Dockerfiles' translations bring (catalogue entries' files), once translated.
    fn finish(&self, spec: &model::Spec) -> model::Spec {
        match self.memo.translated.get().filter(|t| !t.files.is_empty()) {
            Some(t) => {
                let mut texts = self.templates.clone();
                texts.extend(t.files.iter().map(|(k, v)| (k.clone(), v.clone())));
                templates::add(self.images.apply(spec), &texts)
            }
            None => templates::add(self.images.apply(spec), &self.templates),
        }
    }

    /// The spec as written.
    #[must_use]
    pub fn source(&self) -> &syntax::Spec {
        &self.source
    }

    /// The catalogue its services resolve in: Isoloom's own. Hidden: it returns a building-block
    /// crate's type, outside the supported API `isoloom-targets` re-exports [`Lab`] in.
    #[doc(hidden)]
    #[must_use]
    pub fn catalogue(&self) -> &'static Catalogue {
        self.loaded.catalogue
    }

    /// The daemons its catalogue knows the foreground form of.
    pub(crate) fn daemons(&self) -> &DaemonTable {
        &self.loaded.daemons
    }

    /// The spec lowered for `scope`, the image table applied (what the lowering's tests read).
    #[cfg(test)]
    pub(crate) fn lowered(&self, scope: Scope) -> Lowered {
        let mut l = self.raw(scope).into_owned();
        l.spec = self.finish(&l.spec);
        l
    }

    /// The spec lowered for `scope` before the image table, lowered once.
    fn raw(&self, scope: Scope) -> Cow<'_, Lowered> {
        match slot(scope).and_then(|i| self.memo.lowered.get(i)) {
            Some(cell) => Cow::Borrowed(cell.get_or_init(|| self.explained(scope, lower(self.source_for(scope), self.loaded.catalogue, scope)))),
            None => Cow::Owned(self.explained(scope, lower(self.source_for(scope), self.loaded.catalogue, scope))),
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

    /// The targets that can run the lab at all: those [`Lab::not_possible`] rules nothing out
    /// for, in [`Target::ALL`]'s order. A generator may still refuse one of them (`isoloom
    /// targets` says why); a target left out here isn't generated whatever the generator says.
    #[must_use]
    pub fn possible(&self) -> Vec<Target> {
        Target::ALL.into_iter().filter(|t| self.not_possible(*t).is_none()).collect()
    }

    /// Why `target` can't run the lab at all, whatever its generator would say, if it can't: the
    /// one rule [`Lab::possible`] and every caller that explains a target left out read. In this
    /// order: a spec with no machines runs nowhere; a machine the target has no form for (the access machine the spec
    /// leaves to the runner has the forms the image table gives it); and `targets:` leaves the
    /// target out.
    #[must_use]
    pub fn not_possible(&self, target: Target) -> Option<NotPossible> {
        let spec = &self.source;
        #[cfg(feature = "paas")]
        let empty = spec.machines.is_empty() && !crate::paas::has_services(spec);
        #[cfg(not(feature = "paas"))]
        let empty = spec.machines.is_empty();
        if empty {
            return Some(NotPossible::NoMachines);
        }
        if let Some(missing) = crate::lower::missing_form(self.source_for(Scope::Target(target)), self.loaded.catalogue, target) {
            let missing = if dockerfiles::wants_vm(target) {
                self.translated().explain(missing)
            } else {
                missing
            };
            return Some(NotPossible::MissingForm(self.place(missing)));
        }
        if let Some(missing) = self.unsupplied(target) {
            return Some(NotPossible::Unsupplied(self.place(missing)));
        }
        spec.targets
            .as_ref()
            .is_some_and(|wanted| !wanted.contains(&target))
            .then_some(NotPossible::LeftOut)
    }

    /// Why `target` can't run the lab, as a `refused` problem at the field: found while
    /// lowering (the first machine it has no form for, or a VM step it doesn't render), else
    /// the access machine the spec leaves to the runner, when the image table gives it none of
    /// the form the target runs.
    #[must_use]
    pub fn refusal(&self, target: Target) -> Option<Problem> {
        let first = self.raw(Scope::Target(target)).refusals.first().cloned().or_else(|| self.unsupplied(target));
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
        if let Some(broken) = &self.loaded.broken {
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
        add(&mut out, self.refused_host_access());
        let targets: Vec<Problem> = self.target_problems().into_iter().filter(|p| !follows_from(&out, p, &self.source)).collect();
        add(&mut out, targets);
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

    /// A `refused` problem at each field where the lab reaches the computer that runs it in a way
    /// [`Lab::with_host_access`] didn't allow.
    fn refused_host_access(&self) -> Vec<Problem> {
        self.host_access()
            .into_iter()
            .filter(|r| !self.host_access.allows(r.access))
            .map(|r| Problem::new(Code::Refused, FieldPath::new(r.at.clone()), r.refusal()))
            .collect()
    }

    /// The targets `targets:` asks for that the spec itself rules out ([`Lab::not_possible`]),
    /// each with why; and, when the spec has machines yet rules every target out whatever
    /// `targets:` says, that, with why the two plain forms can't (a container's, a VM's). The
    /// access machine the image table doesn't supply isn't the spec's fault: a warning says so
    /// ([`Lab::warnings`]).
    fn target_problems(&self) -> Vec<Problem> {
        let by_spec = |t: Target| self.not_possible(t).filter(|r| !matches!(r, NotPossible::LeftOut | NotPossible::Unsupplied(_)));
        let mut out = Vec::new();
        for (i, t) in self.source.targets.iter().flatten().enumerate() {
            if let Some(why) = by_spec(*t) {
                let at = FieldPath::new("targets").index(i);
                out.push(Problem::new(Code::InvalidValue, at, format!("`{}` can't run the environment: {why}", t.id())));
            }
        }
        let none = !self.source.machines.is_empty() && Target::ALL.into_iter().all(|t| by_spec(t).is_some());
        if none {
            let why: Vec<String> = [Target::Docker, Target::Vagrant]
                .into_iter()
                .filter_map(by_spec)
                .map(|r| r.to_string())
                .collect();
            out.push(Problem::new(
                Code::InvalidValue,
                FieldPath::new("machines"),
                format!("no target is possible: {}", why.join("; ")),
            ));
        }
        out
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
        add(&mut out, steps::setting_files(&self.source, self.loaded.catalogue, dir));
        self.sorted(out)
    }

    /// Problems placed and in file order: a clone's at its counted machine.
    fn sorted(&self, problems: Problems) -> Problems {
        let mut placed: Problems = problems.into_iter().map(|p| self.place(p)).collect();
        placed.sort_by_key(|p| p.span.map(|s| (s.line, s.column)));
        placed
    }
}

/// Why a target can't run a lab at all ([`Lab::not_possible`]); `Display` says it as `isoloom
/// targets` and the resolved snapshot do.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum NotPossible {
    /// The spec has no machines.
    NoMachines,
    /// The first machine the target has no form for, at the field that rules it out.
    MissingForm(Problem),
    /// The access machine the spec leaves to the runner, at its `os`: the image table gives it
    /// none of the form the target runs (a fact of this run's table, not of the spec).
    Unsupplied(Problem),
    /// The spec's `targets:` leaves the target out.
    LeftOut,
}

impl fmt::Display for NotPossible {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotPossible::NoMachines => f.write_str("the spec has no machines"),
            NotPossible::MissingForm(p) | NotPossible::Unsupplied(p) => f.write_str(&p.line()),
            NotPossible::LeftOut => f.write_str("left out by `targets:`"),
        }
    }
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

    /// A lab whose built-in catalogue didn't load never plans: an empty catalogue would read as
    /// a lab with no services.
    #[test]
    fn a_lab_whose_catalogue_doesnt_load_fails_to_plan() {
        let mut lab = testing::lab(LAB);
        lab.loaded.broken = Some("catalogue/nginx.yml: not YAML".to_owned());
        let spec = lab.for_target(Target::Docker);
        assert_eq!(
            lab.plan(&spec, Target::Docker, None).err(),
            Some(crate::environment::PlanError::Catalogue {
                reason: "catalogue/nginx.yml: not YAML".to_owned()
            })
        );
    }

    #[test]
    fn a_service_machine_of_a_lab_whose_catalogue_doesnt_load_is_refused_at_its_service() {
        let mut lab = testing::lab(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  db: { os: debian@12, service: postgres, networks: { lan: 10 } }\n  web: { os: debian@12, networks: { lan: 11 } }\n",
        );
        lab.loaded.broken = Some("catalogue/postgres.yml: not YAML".to_owned());
        let refused: Vec<String> = lab
            .problems()
            .iter()
            .filter(|p| p.code == Code::Refused)
            .map(|p| format!("{}: {}", p.path, p.reason))
            .collect();
        assert_eq!(
            refused,
            ["machines.db.service: this Isoloom's catalogue doesn't load: catalogue/postgres.yml: not YAML"]
        );
    }

    #[test]
    fn two_labs_of_one_spec_are_equal_whatever_each_has_worked_out() {
        let (a, b) = (testing::lab(LAB), testing::lab(LAB));
        assert_eq!(a.problems(), []);
        assert_eq!(a, b);
        assert_ne!(a, testing::lab(&LAB.replace("lan: 10,", "lan: 11,")));
    }

    #[test]
    fn every_vlan_problem_is_reported_not_only_the_first() {
        let lab = testing::lab(
            "version: 1\nname: t\nnetworks:\n  office: { cidr: 10.10.0.0/16, gateway: pc, vlans: { 5000: { cidr: 10.10.50.0/24 }, 20: { cidr: 10.99.0.0/24 } } }\nmachines:\n  pc: { networks: { office.vlan20: 10 }, app: { image: alpine } }\n",
        );
        let paths: Vec<String> = lab.problems().iter().map(|p| p.path.to_string()).collect();
        assert_eq!(
            paths,
            [
                "networks.office",
                "networks.office.cidr",
                "networks.office.gateway",
                "networks.office.vlans.5000",
                "networks.office.vlans.20.cidr",
                "machines.pc.networks.office.vlan20"
            ]
        );
    }

    #[test]
    fn an_existing_network_is_checked_under_the_override_that_names_it() {
        let lab = testing::lab(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12 }\noverrides:\n  proxmox: { networks: { lan: { external: 'vmbr 1' } } }\n  docker: { networks: { lan: { external: 'corp net' } } }\n",
        );
        let paths: Vec<String> = lab.problems().iter().map(|p| p.path.to_string()).collect();
        let overrides: Vec<&String> = paths.iter().filter(|p| p.starts_with("overrides.")).collect();
        assert_eq!(overrides, ["overrides.proxmox.networks.lan.external", "overrides.docker.networks.lan.external"]);
    }

    #[test]
    fn an_image_a_group_or_a_counted_machine_gives_is_checked_under_the_key_it_is_written_at() {
        let lab = testing::lab(
            "version: 1\nname: t\ngroups: { windows: { members: [dc1] } }\nmachines:\n  dc1: { os: debian@12 }\n  w: { os: debian@12, count: 2 }\n  a: { os: debian@12 }\noverrides:\n  vagrant: { machines: { windows: { box: 'bad box' }, w: { box: 'bad box' }, a: { box: 'bad box' } } }\n",
        );
        let paths: Vec<String> = lab.problems().iter().map(|p| p.path.to_string()).collect();
        // Each at the key it is written at, none at a key nobody wrote (`dc1`, `w-01`).
        let overrides: Vec<&String> = paths.iter().filter(|p| p.starts_with("overrides.")).collect();
        assert_eq!(
            overrides,
            [
                "overrides.vagrant.machines.windows.box",
                "overrides.vagrant.machines.w.box",
                "overrides.vagrant.machines.a.box"
            ]
        );
    }

    #[test]
    fn a_machine_the_authored_checks_refuse_shows_nothing_lowering_finds_under_it() {
        // `db` has no OS: the authored check says so, and what its lowered form lacks isn't shown.
        let lab = testing::lab(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  db: { networks: { lan: 10 }, app: { steps: [{ run: a.sh }] }, depends_on: [ghost] }\n  web: { networks: { lan: 11 }, app: { image: nginx }, depends_on: [nowhere] }\n",
        );
        let paths: Vec<String> = lab.problems().iter().map(|p| p.path.to_string()).collect();
        // `db`'s own and an unrefused machine's, not `db.depends_on`.
        assert_eq!(paths, ["machines.db.os", "machines.web.depends_on[0]"]);
    }

    #[test]
    fn a_secret_no_machine_gets_is_a_problem_unless_a_catalogue_service_may_read_it() {
        let secrets = |machines: &str| -> Vec<String> {
            let lab = testing::lab(&format!("version: 1\nname: t\nsecrets: {{ db: {{ env: DB }} }}\nmachines:\n{machines}"));
            lab.problems()
                .iter()
                .map(|p| p.path.to_string())
                .filter(|p| p.starts_with("secrets."))
                .collect()
        };
        assert_eq!(secrets("  web: { app: { image: nginx } }\n"), ["secrets.db"], "no machine gets it");
        assert_eq!(
            secrets("  db: { os: debian@12, service: postgres }\n"),
            Vec::<String>::new(),
            "a catalogue service may"
        );
    }

    #[test]
    fn a_copy_step_takes_a_file_of_the_project_that_exists() {
        let dir = tempfile::Builder::new().prefix("isoloom-plan-copy-").tempdir().expect("a project folder");
        let dir = dir.path();
        std::fs::create_dir_all(dir.join("site")).expect("a project folder");
        let lab = testing::lab(
            "version: 1\nname: t\nmachines:\n  web: { os: debian@12, app: { steps: [{ copy: { from: site, to: /srv } }, { copy: { from: missing, to: /m } }] } }\n",
        );
        let found: Vec<(String, String)> = steps::step_files(lab.source(), dir)
            .into_iter()
            .map(|p| (p.path.to_string(), p.reason))
            .collect();
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
