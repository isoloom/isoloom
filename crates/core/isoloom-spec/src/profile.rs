//! Profiles: one spec, one file per customer or site.
//!
//! A product has one spec and many places to run it: this customer on Proxmox without
//! internet, that one on AWS in their own region, another on local VMs with a bigger web
//! server. A profile says what differs for one of them, kept with the project in
//! `profiles/<name>.yml` and chosen with `--profile <name>` (or `ISOLOOM_PROFILE`):
//!
//! ```yaml
//! # profiles/bank-fr.yml
//! description: Bank, Paris site: Proxmox, no internet
//! target: proxmox                     # the target this profile runs on
//! spec:                               # merged into the spec, as -s would
//!   networks: { lan: { internet: false } }
//!   machines: { web: { resources: { cpus: 4, memory_mb: 8192 } } }
//! defaults:                           # a layer of the defaults (see `defaults`)
//!   cloud: { aws: { region: eu-west-3 } }
//! inputs:                             # launch-time values, unless the environment sets them
//!   REGISTRY: registry.bank.local
//! ```
//!
//! Where it sits: the spec's own values, then the profile's `spec`, then `-s`. For the
//! defaults: built in, the user's file, the project's file, the profile, `--images`, the
//! environment, `-s defaults.<key>`. An input set in the environment wins over the profile's.
//!
//! A profile names only what the spec has: an unknown machine, network or input is an
//! error, so a renamed machine can't leave a profile silently doing nothing.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::Deserialize;
use serde_yaml_ng::{Mapping, Value};

use crate::defaults::Layer;
use crate::model::Target;

/// The folder of profiles, next to the spec.
pub const DIR: &str = "profiles";

/// A profile, read and checked against nothing yet (see [`Profile::check`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    /// The profile's name: its file name without `.yml` (`isoloom run --profile <name>`).
    pub name: String,
    /// The profile's file.
    pub path: PathBuf,
    /// What the profile is for, as `isoloom profiles` lists it.
    pub description: Option<String>,
    /// The target the profile runs on: the default when none is given, and the only one allowed.
    pub target: Option<Target>,
    /// Merged into the spec before `-s`.
    pub spec: Value,
    /// A layer of the defaults.
    pub defaults: Value,
    /// Launch-time inputs, by name.
    pub inputs: IndexMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    target: Option<Target>,
    #[serde(default)]
    spec: Option<Value>,
    #[serde(default)]
    defaults: Option<Value>,
    #[serde(default)]
    inputs: IndexMap<String, Value>,
}

/// A profile name: kebab-case, as a spec's name.
#[must_use]
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('-') && !name.ends_with('-') && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The profile file for `name` in `project`: `profiles/<name>.yml` or `.yaml`, never both.
pub fn path(project: &Path, name: &str) -> Result<PathBuf, String> {
    if !valid_name(name) {
        return Err(format!("profile `{name}`: a profile name is kebab-case (lowercase letters, digits, dashes)"));
    }
    let found: Vec<PathBuf> = ["yml", "yaml"]
        .iter()
        .map(|e| project.join(DIR).join(format!("{name}.{e}")))
        .filter(|p| p.is_file())
        .collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => {
            let known = list(project);
            let hint = if known.is_empty() {
                format!("no profiles in {}", project.join(DIR).display())
            } else {
                format!("profiles here: {}", known.join(", "))
            };
            Err(format!("no profile `{name}` ({hint})"))
        }
        _ => Err(format!("both {DIR}/{name}.yml and {DIR}/{name}.yaml exist; keep one")),
    }
}

/// The profiles of a project, by name, sorted.
#[must_use]
pub fn list(project: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(project.join(DIR)) else { return vec![] };
    let mut names: Vec<String> = entries
        .filter_map(|e| {
            let p = e.ok()?.path();
            let ext = p.extension()?.to_str()?;
            if !p.is_file() || !["yml", "yaml"].contains(&ext) {
                return None;
            }
            let stem = p.file_stem()?.to_str()?.to_owned();
            valid_name(&stem).then_some(stem)
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Reads `profiles/<name>.yml` in `project`.
pub fn load(project: &Path, name: &str) -> Result<Profile, String> {
    let path = path(project, name)?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    parse(name, path, &text)
}

/// A profile from its text.
pub fn parse(name: &str, path: PathBuf, text: &str) -> Result<Profile, String> {
    let at = |e: String| format!("profile `{name}` ({}): {e}", path.display());
    let file: File = serde_yaml_ng::from_str(text).map_err(|e| at(e.to_string()))?;
    let mapping = |v: Option<Value>, key: &str| -> Result<Value, String> {
        match v {
            None | Some(Value::Null) => Ok(Value::Mapping(Mapping::new())),
            Some(m @ Value::Mapping(_)) => Ok(m),
            Some(_) => Err(at(format!("`{key}` is a mapping"))),
        }
    };
    let spec = mapping(file.spec, "spec")?;
    let defaults = mapping(file.defaults, "defaults")?;
    let mut inputs = IndexMap::new();
    for (k, v) in file.inputs {
        let value = match v {
            Value::String(s) => s,
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => return Err(at(format!("input `{k}` takes a single value (text, a number or a boolean)"))),
        };
        inputs.insert(k, value);
    }
    Ok(Profile {
        name: name.to_owned(),
        path,
        description: file.description,
        target: file.target,
        spec,
        defaults,
        inputs,
    })
}

impl Profile {
    /// Where it shows in `isoloom defaults` and in errors.
    #[must_use]
    pub fn source(&self) -> String {
        format!("profile {} ({})", self.name, self.path.display())
    }

    /// Its layer of the defaults.
    #[must_use]
    pub fn layer(&self) -> Layer {
        Layer {
            source: self.source(),
            value: self.defaults.clone(),
        }
    }

    /// What's wrong with the profile for this spec (the spec's YAML as written, before any
    /// override): machines, networks and inputs it names that the spec doesn't have.
    pub fn check(&self, spec: &Value) -> Vec<String> {
        let mut problems = Vec::new();
        let names = |section: &str| -> Vec<String> {
            spec.get(section)
                .and_then(Value::as_mapping)
                .map(|m| m.keys().filter_map(|k| k.as_str().map(str::to_owned)).collect())
                .unwrap_or_default()
        };
        for section in ["machines", "networks"] {
            let known = names(section);
            if let Some(m) = self.spec.get(section).and_then(Value::as_mapping) {
                for k in m.keys() {
                    let k = k.as_str().unwrap_or_default();
                    if !known.iter().any(|n| n == k) {
                        problems.push(format!(
                            "profile `{}`: `spec.{section}.{k}`: the spec has no {} `{k}`",
                            self.name,
                            section.strip_suffix('s').unwrap_or(section)
                        ));
                    }
                }
            }
        }
        let declared: Vec<String> = spec
            .get("inputs")
            .and_then(Value::as_sequence)
            .map(|s| s.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        for k in self.inputs.keys() {
            if !declared.iter().any(|d| d == k) {
                problems.push(format!("profile `{}`: input `{k}` isn't one of the spec's `inputs`", self.name));
            }
        }
        problems
    }

    /// The target to use: the profile's when none is asked for; an error when another is.
    pub fn pin(&self, asked: Option<&str>) -> Result<Option<String>, String> {
        match (self.target, asked) {
            (Some(t), None) => Ok(Some(t.id().to_owned())),
            (Some(t), Some(a)) if a != t.id() => Err(format!(
                "profile `{}` runs on `{}`, not `{a}`: drop the target or choose another profile",
                self.name,
                t.id()
            )),
            (_, a) => Ok(a.map(str::to_owned)),
        }
    }
}
