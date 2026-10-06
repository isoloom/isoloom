//! Defaults: settings that aren't the spec's business but a person's or a team's, in layers,
//! the last one winning:
//!
//! 1. built in (the image table in [`crate::images`], each cloud's region in its generator),
//! 2. the user's file, `$ISOLOOM_HOME/defaults.yml` (`~/.isoloom/defaults.yml`),
//! 3. the project's file, `isoloom.defaults.yml` next to the spec (committed: the team's),
//! 4. environment variables `ISOLOOM_<KEY>`, `__` standing for a dot (`ISOLOOM_CLOUD__AWS__REGION`),
//! 5. the command line, `-s defaults.<key>=<value>`.
//!
//! Mappings deep-merge, so a file that sets one region leaves the others alone. The same `-s`
//! without the `defaults.` prefix overrides the spec itself (`-s machines.web.vm.os=ubuntu-24.04`),
//! for one run.
//!
//! ```yaml
//! images:                 # the image table (see Images): OS images, the access machine
//!   os: { debian-12: { vagrant: my-org/debian-12 } }
//!   access: { docker: kalilinux/kali-rolling, vm: kali }
//! vagrant:
//!   provider: libvirt     # `isoloom run vagrant` passes --provider
//! cloud:
//!   aws: { region: eu-west-1 }
//!   azure: { region: westeurope }
//! ```

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::Deserialize;
use serde_yaml_ng::{Mapping, Value};

use crate::generate::GeneratedFile;
use crate::images::Table;

/// The project-level file, next to `isoloom.yml`.
pub const PROJECT_FILE: &str = "isoloom.defaults.yml";

/// The typed defaults, once the layers are merged. Unknown keys are errors.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    /// The image table: OS images and the access machine the runner supplies.
    #[serde(default)]
    pub images: Table,
    #[serde(default)]
    pub vagrant: Vagrant,
    /// Per cloud (`aws`, `azure`, `gcp`, `digitalocean`, `linode`, `oci`).
    #[serde(default)]
    pub cloud: IndexMap<String, Cloud>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vagrant {
    /// The provider `isoloom run vagrant` asks for (`virtualbox`, `libvirt`, `vmware_desktop`,
    /// `parallels`, `utm`, `qemu`, `hyperv`, `vmware_esxi`). Default: Vagrant's own choice.
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cloud {
    /// The region the generated module defaults to.
    #[serde(default)]
    pub region: Option<String>,
}

/// Each cloud's built-in region, as its generator writes it. A test keeps them equal.
pub const BUILTIN_REGIONS: &[(&str, &str)] = &[
    ("aws", "eu-west-3"),
    ("azure", "swedencentral"),
    ("gcp", "europe-west9"),
    ("digitalocean", "fra1"),
    ("linode", "fr-par"),
    ("oci", "eu-paris-1"),
];

/// One layer's contribution: where it came from, and what it set.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub source: String,
    pub value: Value,
}

/// The merged defaults with, for every leaf, the layer that set it last.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub defaults: Defaults,
    pub merged: Value,
    /// Dotted key -> source, for `isoloom defaults`.
    pub sources: Vec<(String, String)>,
}

/// The user's defaults file.
pub fn user_file() -> PathBuf {
    crate::registry::home().join("defaults.yml")
}

/// Reads every layer for a project: the user's file, the project's, the environment, then
/// `sets` (`defaults.cloud.aws.region=eu-west-1`, the prefix optional), and `images_file`
/// (the `--images` table, between the files and `sets`).
pub fn load(project: &Path, images_file: Option<&Path>, sets: &[String]) -> Result<Resolved, String> {
    let mut layers = Vec::new();
    for (label, path) in [("user file", user_file()), ("project file", project.join(PROJECT_FILE))] {
        match std::fs::read_to_string(&path) {
            Ok(text) => layers.push(Layer {
                source: format!("{label} {}", path.display()),
                value: serde_yaml_ng::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("can't read {}: {e}", path.display())),
        }
    }
    if let Some(f) = images_file {
        let text = std::fs::read_to_string(f).map_err(|e| format!("can't read {}: {e}", f.display()))?;
        let table: Value = serde_yaml_ng::from_str(&text).map_err(|e| format!("{}: {e}", f.display()))?;
        let mut m = Mapping::new();
        m.insert(Value::String("images".into()), table);
        layers.push(Layer {
            source: format!("--images {}", f.display()),
            value: Value::Mapping(m),
        });
    }
    let env = from_env(std::env::vars());
    if !env.is_empty() {
        for (k, v) in env {
            layers.push(Layer {
                source: format!("environment ISOLOOM_{}", k.replace('.', "__").to_uppercase()),
                value: at_path(&k, scalar(&v)),
            });
        }
    }
    for s in sets {
        let (k, v) = parse_set(s)?;
        let Some(k) = k.strip_prefix("defaults.") else { continue };
        layers.push(Layer {
            source: format!("-s {s}"),
            value: at_path(k, scalar(v)),
        });
    }
    resolve(layers)
}

/// Merges layers in order; later ones win leaf by leaf.
pub fn resolve(layers: Vec<Layer>) -> Result<Resolved, String> {
    let mut merged = Value::Mapping(Mapping::new());
    let mut sources: Vec<(String, String)> = Vec::new();
    for layer in &layers {
        merge(&mut merged, &layer.value);
        for leaf in leaves(&layer.value, "") {
            sources.retain(|(k, _)| *k != leaf);
            sources.push((leaf, layer.source.clone()));
        }
    }
    let defaults: Defaults = serde_yaml_ng::from_value(merged.clone()).map_err(|e| {
        let where_ = layers.iter().map(|l| l.source.as_str()).collect::<Vec<_>>().join(", ");
        format!("defaults ({where_}): {e}")
    })?;
    Ok(Resolved { defaults, merged, sources })
}

/// `ISOLOOM_CLOUD__AWS__REGION=x` -> `("cloud.aws.region", "x")`. Only the defaults' sections
/// are read (`ISOLOOM_HOME`, `ISOLOOM_DERIVED` and the publish variables mean other things).
pub fn from_env(vars: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vars
        .filter_map(|(k, v)| {
            let rest = k.strip_prefix("ISOLOOM_")?;
            let key = rest.to_lowercase().replace("__", ".");
            let section = key.split('.').next().unwrap_or("");
            ["images", "vagrant", "cloud"].contains(&section).then_some((key, v))
        })
        .collect();
    out.sort();
    out
}

/// `key=value` of a `-s`.
pub fn parse_set(s: &str) -> Result<(&str, &str), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("`-s {s}`: expected key=value, like -s machines.web.vm.os=ubuntu-24.04"))?;
    if k.is_empty() || k.split('.').any(|p| p.is_empty()) {
        return Err(format!("`-s {s}`: the key is a dotted path, like cloud.aws.region"));
    }
    Ok((k, v))
}

/// A value as YAML would read it (`3` a number, `true` a boolean, else text).
pub fn scalar(text: &str) -> Value {
    serde_yaml_ng::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

/// A nested mapping holding `value` at a dotted path.
pub fn at_path(path: &str, value: Value) -> Value {
    path.rsplit('.').fold(value, |inner, key| {
        let mut m = Mapping::new();
        m.insert(Value::String(key.to_string()), inner);
        Value::Mapping(m)
    })
}

/// Deep-merges `from` into `into`: mappings key by key, anything else replaced.
pub fn merge(into: &mut Value, from: &Value) {
    match (into, from) {
        (Value::Mapping(a), Value::Mapping(b)) => {
            for (k, v) in b {
                match a.get_mut(k) {
                    Some(existing) if matches!((&*existing, v), (Value::Mapping(_), Value::Mapping(_))) => merge(existing, v),
                    _ => {
                        a.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (into, from) => *into = from.clone(),
    }
}

/// Dotted paths of every leaf of a value.
pub fn leaves(v: &Value, prefix: &str) -> Vec<String> {
    match v {
        Value::Mapping(m) if !m.is_empty() => m
            .iter()
            .flat_map(|(k, c)| {
                let key = k.as_str().map(str::to_string).unwrap_or_else(|| format!("{k:?}"));
                leaves(c, &if prefix.is_empty() { key } else { format!("{prefix}.{key}") })
            })
            .collect(),
        _ => vec![prefix.to_string()],
    }
}

/// The spec's YAML with `-s key=value` overrides applied (keys without the `defaults.` prefix),
/// before it is parsed: `machines.web.vm.os=ubuntu-24.04`, `networks.lab.internet=false`.
pub fn override_spec(yaml: &str, sets: &[String]) -> Result<String, String> {
    let mut doc: Value = serde_yaml_ng::from_str(yaml).map_err(|e| e.to_string())?;
    let mut any = false;
    for s in sets {
        let (k, v) = parse_set(s)?;
        if k.starts_with("defaults.") {
            continue;
        }
        merge(&mut doc, &at_path(k, scalar(v)));
        any = true;
    }
    if !any {
        return Ok(yaml.to_string());
    }
    serde_yaml_ng::to_string(&doc).map_err(|e| e.to_string())
}

/// Applies the defaults that change generated files: each cloud module's region default.
/// (Images are applied to the spec before generating; `vagrant.provider` at run time.)
pub fn apply_to_files(files: Vec<GeneratedFile>, d: &Defaults) -> Vec<GeneratedFile> {
    files
        .into_iter()
        .map(|mut f| {
            // .isoloom[-n]/cloud-vm/<cloud>/main.tf, .isoloom[-n]/cloud-docker/<cloud>/main.tf
            let parts: Vec<&str> = f.path.split('/').collect();
            if parts.len() == 4
                && parts[1].starts_with("cloud-")
                && parts[3] == "main.tf"
                && let Some(region) = d.cloud.get(parts[2]).and_then(|c| c.region.as_deref())
            {
                f.contents = set_region(&f.contents, region);
            }
            f
        })
        .collect()
}

/// The module text with its `variable "region"` default replaced.
pub fn set_region(tf: &str, region: &str) -> String {
    let Some(start) = tf.find("variable \"region\"") else { return tf.to_string() };
    let Some(close) = tf[start..].find("\n}") else { return tf.to_string() };
    let block = &tf[start..start + close];
    let Some(d) = block.find("default") else { return tf.to_string() };
    let Some(q1) = block[d..].find('"') else { return tf.to_string() };
    let q1 = d + q1 + 1;
    let Some(q2) = block[q1..].find('"') else { return tf.to_string() };
    let mut out = String::with_capacity(tf.len());
    out.push_str(&tf[..start + q1]);
    out.push_str(region);
    out.push_str(&tf[start + q1 + q2..]);
    out
}

/// What the built-in layer holds, for `isoloom defaults --system`: the clouds' regions (the
/// image table is documented on its own page and read from the code).
pub fn builtin() -> Value {
    let mut cloud = Mapping::new();
    for (c, r) in BUILTIN_REGIONS {
        let mut m = Mapping::new();
        m.insert(Value::String("region".into()), Value::String((*r).into()));
        cloud.insert(Value::String((*c).into()), Value::Mapping(m));
    }
    let mut top = Mapping::new();
    top.insert(Value::String("cloud".into()), Value::Mapping(cloud));
    Value::Mapping(top)
}
