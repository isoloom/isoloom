//! The Isoloom spec: the format an author writes ([`syntax`]: parsed into types that hold no
//! invalid state, every problem coded and placed), the form the plan lowers it into ([`model`]),
//! the schema and the reference page read from the types, normalization (groups, clones, VLANs,
//! profiles, instances, defaults, the image table) and validation.
//!
//! ```
//! let spec = isoloom_spec::syntax::parse(
//!     "version: 1\nname: hello\nnetworks:\n  lab: { cidr: 10.1.0.0/24 }\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    ports: { web: { port: 80, protocol: http } }\n",
//! )?;
//! assert!(isoloom_spec::syntax::check(&spec).is_empty());
//! assert_eq!(spec.machines["web"].os.map(|os| os.to_string()).as_deref(), Some("debian@12"));
//! # Ok::<(), Vec<isoloom_spec::syntax::Problem>>(())
//! ```

pub mod addresses;
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
pub mod os;
pub mod probe;
pub mod profile;
pub mod reference;
pub mod schema;
pub mod shell;
pub mod syntax;
pub mod targets;
pub mod validate;
pub mod vlans;
pub mod windows;

use std::path::{Path, PathBuf};

pub use error::{LoadError, Result, Stage};
pub use model::{Appliance, Arch, Check, CloudServices, Declared, Dns, Expect, KNOWN_OS, Machine, Network, Port, Reach, Resources, Shape, Target, Tc, Vlan};
pub use syntax::Spec;
pub use targets::{derive, effective};
pub use validate::{Problem, validate, validate_files};

/// The spec file names, at the root of a project: `isoloom.yml` (canonical) or `isoloom.yaml`.
pub const SPEC_FILES: &[&str] = &[SPEC_FILE, "isoloom.yaml"];

/// The spec file's usual name, the one a spec read from text is reported under.
pub const SPEC_FILE: &str = "isoloom.yml";

/// Reads a spec from YAML text, written in the format authors write ([`syntax`]).
///
/// # Errors
///
/// [`LoadError::Invalid`] with every problem found, coded and placed (`file` names the spec file
/// in their first line).
pub fn parse(yaml: &str) -> Result<syntax::Spec, LoadError> {
    parse_file(yaml, SPEC_FILE)
}

/// [`parse`], the problems naming the spec file `file`.
fn parse_file(yaml: &str, file: &str) -> Result<syntax::Spec, LoadError> {
    syntax::parse(yaml).map_err(|problems| LoadError::Invalid {
        file: file.to_owned(),
        problems,
    })
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
///
/// # Errors
///
/// [`LoadError`]: no spec file (or two), one that can't be read, or its problems.
pub fn load(dir: &Path) -> Result<Spec, LoadError> {
    load_with(dir, &[])
}

/// Reads the spec in `dir` with `-s key=value` overrides applied first (see
/// [`defaults::override_spec`]); keys prefixed `defaults.` are left to the defaults.
///
/// # Errors
///
/// [`LoadError`], as [`load`], or an override that doesn't apply.
pub fn load_with(dir: &Path, sets: &[String]) -> Result<Spec, LoadError> {
    load_profiled(dir, None, sets)
}

/// Reads the spec in `dir` with a profile's `spec` merged in (checked against the spec first:
/// see [`profile::Profile::check`]), then `-s key=value` overrides.
///
/// # Errors
///
/// [`LoadError`], as [`load_with`], or a profile that names what the spec doesn't have.
pub fn load_profiled(dir: &Path, profile: Option<&profile::Profile>, sets: &[String]) -> Result<Spec, LoadError> {
    let path = find(dir)?;
    let file = path.file_name().map_or_else(|| SPEC_FILE.to_owned(), |f| f.to_string_lossy().into_owned());
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
    parse_file(&text, &file)
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
#[must_use]
pub fn totals(spec: &model::Spec) -> Totals {
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
