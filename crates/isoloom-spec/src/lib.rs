//! The Isoloom spec: the model, parsing and loading, the schema, normalization (groups,
//! clones, VLANs, profiles, instances, defaults, the image table) and validation.

pub mod appliance;
pub mod clones;
pub mod defaults;
pub mod error;
pub mod files;
pub mod groups;
pub mod home;
pub mod images;
pub mod instance;
pub mod model;
pub mod probe;
pub mod profile;
pub mod schema;
pub mod shell;
pub mod targets;
pub mod validate;
pub mod vlans;

use std::path::{Path, PathBuf};

pub use error::{LoadError, Stage};
pub use model::{
    Appliance, Arch, Check, CloudServices, Declared, Dns, Expect, KNOWN_OS, Machine, Network, Reach, Resources, Service, Shape, Spec, Target, Tc, Vlan,
};
pub use targets::{derive, effective};
pub use validate::{Problem, validate, validate_files};

/// The spec file names, at the root of a project: `isoloom.yml` (canonical) or `isoloom.yaml`.
pub const SPEC_FILES: &[&str] = &["isoloom.yml", "isoloom.yaml"];

/// Parses a spec from YAML text. Unknown fields are errors (typos shouldn't pass silently).
/// VLANs written under their LAN come back as networks of their own ([`vlans::flatten`]): the
/// rest of Isoloom only deals in networks.
pub fn parse(yaml: &str) -> Result<Spec, LoadError> {
    let mut doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(yaml)?;
    // `count:` clones, then `common:` and `groups:` fold into the machines, so every reader sees
    // finished ones.
    let clones = clones::expand(&mut doc).map_err(LoadError::at(Stage::Clones))?;
    groups::expand(&mut doc).map_err(LoadError::at(Stage::Groups))?;
    let mut spec: Spec = serde_yaml_ng::from_value(doc)?;
    spec.clones = clones;
    vlans::flatten(spec).map_err(LoadError::at(Stage::Vlans))
}

/// The spec file in `dir`: `isoloom.yml` or `isoloom.yaml`, never both.
pub fn find(dir: &Path) -> Result<PathBuf, LoadError> {
    let found: Vec<PathBuf> = SPEC_FILES.iter().map(|f| dir.join(f)).filter(|p| p.is_file()).collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(LoadError::NotFound(dir.to_path_buf())),
        _ => Err(LoadError::Ambiguous(dir.to_path_buf())),
    }
}

/// Reads the spec in `dir` (see [`find`]).
pub fn load(dir: &Path) -> Result<Spec, LoadError> {
    load_with(dir, &[])
}

/// Reads the spec in `dir` with `-s key=value` overrides applied first (see
/// [`defaults::override_spec`]); keys prefixed `defaults.` are left to the defaults.
pub fn load_with(dir: &Path, sets: &[String]) -> Result<Spec, LoadError> {
    load_profiled(dir, None, sets)
}

/// Reads the spec in `dir` with a profile's `spec` merged in (checked against the spec first:
/// see [`profile::Profile::check`]), then `-s key=value` overrides.
pub fn load_profiled(dir: &Path, profile: Option<&profile::Profile>, sets: &[String]) -> Result<Spec, LoadError> {
    let path = find(dir)?;
    let text = std::fs::read_to_string(&path).map_err(|source| LoadError::Read { path, source })?;
    let text = match profile {
        Some(p) => {
            let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)?;
            let problems = p.check(&doc);
            if !problems.is_empty() {
                return Err(LoadError::Profile { problems });
            }
            defaults::override_spec_with(&text, Some(&p.spec), sets)
        }
        None => defaults::override_spec(&text, sets),
    }
    .map_err(LoadError::at(Stage::Overrides))?;
    parse(&text)
}

/// A machine's CPUs when its `resources` don't say (defaults sized for a VM, which needs more
/// than a container).
pub const DEFAULT_CPUS: u32 = 1;
/// A machine's memory when its `resources` don't say, in MB.
pub const DEFAULT_MEMORY_MB: u32 = 1024;
/// A machine's disk when its `resources` don't say, in GB.
pub const DEFAULT_DISK_GB: u32 = 20;

/// What the whole spec needs when every machine is a VM (the access machine included).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Totals {
    /// How many machines.
    pub machines: u32,
    /// Their CPUs, added up.
    pub cpus: u32,
    /// Their memory in MB, added up.
    pub memory_mb: u32,
    /// Their disks in GB, added up.
    pub disk_gb: u32,
}

/// What the whole spec needs when every machine is a VM, each machine's `resources` or the
/// defaults: what `isoloom resources` reports before a run.
pub fn totals(spec: &Spec) -> Totals {
    spec.machines.values().fold(Totals::default(), |t, m| {
        let r = m.resources.unwrap_or(Resources {
            cpus: None,
            memory_mb: None,
            disk_gb: None,
        });
        // Saturating: a spec with absurd per-machine resources reports a capped total instead of
        // overflowing (a debug panic / release wrap).
        Totals {
            machines: t.machines + 1,
            cpus: t.cpus.saturating_add(r.cpus.unwrap_or(DEFAULT_CPUS)),
            memory_mb: t.memory_mb.saturating_add(r.memory_mb.unwrap_or(DEFAULT_MEMORY_MB)),
            disk_gb: t.disk_gb.saturating_add(r.disk_gb.unwrap_or(DEFAULT_DISK_GB)),
        }
    })
}
