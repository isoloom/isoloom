//! Isoloom: describe an environment once (machines, networks, the services they expose),
//! run it anywhere. Each target produces the same external behavior its own way: containers,
//! VMs on this machine, a Proxmox server, or the cloud.
//!
//! This crate is the format itself: parse, validate, derive the targets a spec can run on,
//! and add up what it needs. The `isoloom` binary and other tools build on it; the generators (Docker Compose, Vagrant, Proxmox...) come next.

pub mod model;
pub mod targets;
pub mod validate;

use std::path::{Path, PathBuf};

pub use model::{KNOWN_OS, Machine, Network, Reach, Resources, Service, Shape, Spec, Target};
pub use targets::{derive, effective};
pub use validate::{Problem, validate, validate_files};

/// The spec file, at the root of a project.
pub const SPEC_FILE: &str = "isoloom.yaml";

#[derive(Debug)]
pub enum LoadError {
    NotFound(PathBuf),
    Read(PathBuf, std::io::Error),
    Parse(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NotFound(d) => write!(f, "no {SPEC_FILE} in {}", d.display()),
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

/// The spec file in `dir`, if there is one.
pub fn find(dir: &Path) -> Option<PathBuf> {
    Some(dir.join(SPEC_FILE)).filter(|p| p.is_file())
}

/// Reads `isoloom.yaml` in `dir`.
pub fn load(dir: &Path) -> Result<Spec, LoadError> {
    let path = find(dir).ok_or_else(|| LoadError::NotFound(dir.to_path_buf()))?;
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
        Totals {
            machines: t.machines + 1,
            cpus: t.cpus + r.cpus.unwrap_or(DEFAULT_CPUS),
            memory_mb: t.memory_mb + r.memory_mb.unwrap_or(DEFAULT_MEMORY_MB),
            disk_gb: t.disk_gb + r.disk_gb.unwrap_or(DEFAULT_DISK_GB),
        }
    })
}
