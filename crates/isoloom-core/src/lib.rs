//! Isoloom: describe an environment once (machines, networks, the services they expose),
//! run it anywhere. Each target produces the same external behavior its own way: containers,
//! VMs on this machine, a Proxmox server, or the cloud.
//!
//! This crate is the format itself: parse, validate, derive the targets a spec can run on,
//! and add up what it needs; generate each target's files (Docker Compose, Vagrant); and
//! track what each output does with every field ([`coverage`]).

pub mod checks;
pub mod coverage;
pub mod generate;
pub mod images;
pub mod import;
pub mod instance;
pub mod model;
pub mod registry;
pub mod schema;
pub mod targets;
pub mod validate;

use std::path::{Path, PathBuf};

pub use generate::resolved;
pub use generate::{GenerateError, GeneratedFile, OUTPUT_DIR, generate, generate_all, generate_instance, refusal};
pub use model::{Arch, Check, Declared, Dns, Expect, KNOWN_OS, Machine, Network, Reach, Resources, Service, Shape, Spec, Target};
pub use targets::{derive, effective};
pub use validate::{Problem, validate, validate_files};

/// The spec file names, at the root of a project: `isoloom.yml` (canonical) or `isoloom.yaml`.
pub const SPEC_FILES: &[&str] = &["isoloom.yml", "isoloom.yaml"];

#[derive(Debug)]
pub enum LoadError {
    NotFound(PathBuf),
    /// Both `isoloom.yml` and `isoloom.yaml` exist: which one is meant is ambiguous.
    Ambiguous(PathBuf),
    Read(PathBuf, std::io::Error),
    Parse(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NotFound(d) => write!(f, "no isoloom.yml in {}", d.display()),
            LoadError::Ambiguous(d) => write!(f, "both isoloom.yml and isoloom.yaml exist in {}; keep one", d.display()),
            LoadError::Read(p, e) => write!(f, "can't read {}: {e}", p.display()),
            LoadError::Parse(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Parses a spec from YAML text. Unknown fields are errors (typos shouldn't pass silently).
pub fn parse(yaml: &str) -> Result<Spec, LoadError> {
    serde_yaml_ng::from_str(yaml).map_err(|e| LoadError::Parse(e.to_string()))
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
    let path = find(dir)?;
    let text = std::fs::read_to_string(&path).map_err(|e| LoadError::Read(path, e))?;
    parse(&text)
}

/// Defaults when a machine doesn't say: a VM needs more than a container.
pub const DEFAULT_CPUS: u32 = 1;
pub const DEFAULT_MEMORY_MB: u32 = 1024;
pub const DEFAULT_DISK_GB: u32 = 20;

/// What the whole spec needs when every machine is a VM (the access machine included).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Totals {
    pub machines: u32,
    pub cpus: u32,
    pub memory_mb: u32,
    pub disk_gb: u32,
}

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
