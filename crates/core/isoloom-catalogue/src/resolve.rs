//! A machine's catalogue service resolved (design §4.9, steps 1 to 5): the entry it names, the
//! release its version picks, the OS checked against it, `with:` typed against the settings,
//! and the recipe resolved, with the official image when the OS has one. Lowering builds both
//! forms from what comes out ([`Resolved`]); every problem is a spec [`Problem`] at the field
//! the author wrote.

mod fill;
mod recipe;
mod surface;

pub(crate) use fill::{Text, Values};
pub use surface::{OfficialImage, ResolvedPort, ValueFile};

use std::collections::BTreeMap;

use indexmap::IndexMap;
use isoloom_spec::syntax::{self, AbsPath, Code, FieldPath, Healthcheck, Kind, OsRef, PortName, Problem, Problems, ProjectPath, ServiceRef, Step, closest};
use serde_json::Value;

use crate::catalogue::{Catalogue, Entry};
use crate::entry::{Release, Source};
use crate::family::Family;
use crate::names::{DataName, EntryName};
use crate::settings::{self, Settings};
use crate::version::{Version, pick};
use recipe::Context;

/// A machine's service, resolved: what its forms are built from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Resolved {
    /// The entry.
    pub entry: EntryName,
    /// The release picked.
    pub release: Version,
    /// Where it comes from.
    pub source: Source,
    /// The machine's OS.
    pub os: OsRef,
    /// The machine's settings, typed.
    pub settings: Settings,
    /// The official image, when the entry has one whose OS is the machine's.
    pub image: Option<OfficialImage>,
    /// The `install` phase: what the official image already is.
    pub install: Vec<Step>,
    /// The `configure` phase, run in every form.
    pub configure: Vec<Step>,
    /// The `start` phase, run before the services at each start.
    pub start: Vec<Step>,
    /// The texts of the templates the steps place, by the path a step's `from` names.
    pub files: BTreeMap<ProjectPath, String>,
    /// The machine's ports, by name.
    pub ports: IndexMap<PortName, ResolvedPort>,
    /// Its persistent data, by name.
    pub data: IndexMap<DataName, AbsPath>,
    /// When it counts as healthy beyond its ports.
    pub health: Option<Healthcheck>,
    /// Its passwords' value files.
    pub value_files: Vec<ValueFile>,
}

impl Catalogue {
    /// Machine `machine` of `spec`'s catalogue services, resolved; empty when it has none.
    ///
    /// # Errors
    ///
    /// Every problem of its `service`, `os` and `with`, each at its field: an unknown entry (with
    /// a did-you-mean), a version no release has, an OS the release doesn't run on, a setting
    /// `with:` gets wrong.
    pub fn resolve(&self, spec: &syntax::Spec, machine: &str) -> Result<Vec<Resolved>, Problems> {
        let Some(m) = spec.machines.get(machine) else { return Ok(Vec::new()) };
        let Kind::Service(services) = &m.kind else { return Ok(Vec::new()) };
        let at = FieldPath::new("machines").key(machine);
        let Some(os) = m.os else {
            // The spec's own check reports the missing `os`.
            return Err(Vec::new());
        };
        let mut problems = Vec::new();
        let mut out = Vec::new();
        let several = services.services.len() > 1;
        let mut known_keys: Vec<String> = Vec::new();
        for (i, service) in services.services.iter().enumerate() {
            let field = if several { at.key("service").index(i) } else { at.key("service") };
            let Some(entry) = self.entry(service.name()) else {
                problems.push(self.unknown(service, field));
                continue;
            };
            known_keys.extend(entry.definition().settings.keys().map(ToString::to_string));
            let with: IndexMap<String, Value> = services
                .with
                .iter()
                .filter(|(k, _)| !several || entry.definition().settings.keys().any(|s| s.as_str() == *k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let request = Request {
                spec,
                machine,
                os,
                at: &at,
                field,
                with: &with,
            };
            match entry.resolve(service, &request) {
                Ok(r) => out.push(r),
                Err(p) => problems.extend(p),
            }
        }
        if several {
            for key in services.with.keys().filter(|k| !known_keys.contains(k)) {
                problems.push(Problem::new(
                    Code::UnknownField,
                    at.key("with").key(key),
                    format!("no service of the machine has a setting `{key}`"),
                ));
            }
        }
        if problems.is_empty() { Ok(out) } else { Err(problems) }
    }

    /// An entry name the catalogue doesn't have.
    fn unknown(&self, service: &ServiceRef, field: FieldPath) -> Problem {
        let names = self.names();
        let p = Problem::new(Code::UnknownReference, field, format!("the catalogue has no entry `{}`", service.name()));
        match closest(service.name(), &names) {
            Some(n) => p.fix(format!("did you mean `{n}`?")),
            None if names.is_empty() => p.fix("a custom app: `app: { image: <ref> }`, or `os` and `app.steps`"),
            None => p.fix(format!(
                "its entries: {}, or a custom `app:`",
                names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
            )),
        }
    }
}

/// What a machine asks of one entry.
struct Request<'a> {
    spec: &'a syntax::Spec,
    machine: &'a str,
    os: OsRef,
    at: &'a FieldPath,
    field: FieldPath,
    with: &'a IndexMap<String, Value>,
}

impl Entry {
    /// The entry resolved for `request`.
    fn resolve(&self, service: &ServiceRef, request: &Request<'_>) -> Result<Resolved, Problems> {
        let release = self.release_for(service, &request.field).map_err(|p| vec![p])?;
        if !release.runs_on(request.os) {
            let p = Problem::new(
                Code::InvalidValue,
                request.at.key("os"),
                format!("{} {} doesn't run on {}", self.name(), release.version, request.os),
            )
            .fix(format!("one of its OSes: {}", release.os_list()));
            return Err(vec![p]);
        }
        let family = Family::from(request.os);
        let around = settings::Around {
            spec: request.spec,
            machine: request.machine,
            family,
        };
        let settings = settings::read(self.name().as_str(), &self.definition().settings, request.with, around, &request.at.key("with"))?;
        let context = Context {
            entry: self,
            release,
            os: request.os,
            settings: &settings,
        };
        let mut files = BTreeMap::new();
        let recipe = &self.definition().recipe;
        let phases = [("install", &recipe.install), ("configure", &recipe.configure), ("start", &recipe.start)]
            .map(|(name, steps)| context.phase(name, steps, &mut files));
        let [install, configure, start] = phases;
        let broken = |e: recipe::StepError| {
            vec![Problem::new(
                Code::InvalidValue,
                request.field.clone(),
                format!("{}'s recipe ({}[{}]): {}", self.name(), e.phase, e.index, e.reason),
            )]
        };
        let (install, configure, mut start) = (install.map_err(broken)?, configure.map_err(broken)?, start.map_err(broken)?);
        let surface = surface::Surface::of(&context)
            .map_err(|reason| vec![Problem::new(Code::InvalidValue, request.field.clone(), format!("{}: {reason}", self.name()))])?;
        start.splice(0..0, surface.owners);
        Ok(Resolved {
            entry: self.name().clone(),
            release: release.version.clone(),
            source: release.source,
            os: request.os,
            image: surface.image,
            install,
            configure,
            start,
            files,
            ports: surface.ports,
            data: self.definition().data.clone(),
            health: self.definition().health.clone(),
            value_files: surface.value_files,
            settings,
        })
    }

    /// The release `service`'s version picks: the default when it gives none.
    fn release_for(&self, service: &ServiceRef, field: &FieldPath) -> Result<&Release, Problem> {
        let def = self.definition();
        let asked = service.as_str().split_once('@').map(|(_, v)| v);
        let Some(asked) = asked else {
            return def
                .release(&def.default)
                .ok_or_else(|| Problem::new(Code::InvalidValue, field.clone(), format!("{}'s default release is missing", self.name())));
        };
        let none = || {
            Problem::new(Code::InvalidValue, field.clone(), format!("{} has no release `{asked}`", self.name()))
                .fix(format!("one of its releases: {}", def.releases_list()))
        };
        let version: Version = asked.parse().map_err(|_| none())?;
        let picked = pick(def.releases.iter().map(|r| &r.version), &version).ok_or_else(none)?;
        def.release(picked).ok_or_else(none)
    }
}
