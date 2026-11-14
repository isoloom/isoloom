//! Defaults: settings that aren't the spec's business but a person's or a team's, in layers,
//! the last one winning:
//!
//! 1. built in (the image table in [`crate::images`], each cloud's region in its generator,
//!    which the environment writes into its module and run manifest: `Lab::with_regions`),
//! 2. the user's file, `$ISOLOOM_HOME/defaults.yml` (`~/.isoloom/defaults.yml`),
//! 3. the project's file, `isoloom.defaults.yml` next to the spec (committed: the team's),
//! 4. the profile's `defaults:` (`profiles/<name>.yml`, [`crate::profile`]),
//! 5. the image table given to `--images`, under `images`,
//! 6. environment variables `ISOLOOM_<KEY>`, `__` standing for a dot (`ISOLOOM_CLOUD__AWS__REGION`),
//! 7. the command line, `-s defaults.<key>=<value>`.
//!
//! Mappings deep-merge, so a file that sets one region leaves the others alone. The same `-s`
//! without the `defaults.` prefix overrides the spec itself (`-s machines.web.os=ubuntu@24.04`),
//! for one run.
//!
//! ```yaml
//! # ~/.isoloom/defaults.yml
//! images:                 # the image table (see Images): OS images, the access machine
//!   os: { "debian@12": { vagrant: my-org/debian-12 } }
//!   access: { docker: registry.example.com/workstation:1, vm: ubuntu@24.04 }
//! vagrant:
//!   provider: libvirt     # a runner passes --provider
//! cloud:
//!   aws: { region: eu-west-1 }
//!   azure: { region: westeurope }
//! ```

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::Deserialize;
use serde_yaml_ng::{Mapping, Value};

use crate::host::Host;
use crate::images::Table;
use crate::{Error, Result};

/// The project-level file, next to `isoloom.yml`.
pub const PROJECT_FILE: &str = "isoloom.defaults.yml";

/// The typed defaults, once the layers are merged. Unknown keys are errors.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    /// The image table: OS images and the access machine the runner supplies.
    #[serde(default)]
    pub images: Table,
    /// How a runner runs Vagrant.
    #[serde(default)]
    pub vagrant: Vagrant,
    /// Per cloud (`aws`, `azure`, `gcp`, `digitalocean`, `linode`, `oci`).
    #[serde(default)]
    pub cloud: IndexMap<String, Cloud>,
    /// Tags every cloud resource and Kubernetes object carries (a team's cost centre), under the
    /// spec's own `tags:` ([`Resolved::tags_under`]), by the spec's rules; a number or
    /// `true`/`false` is read as its text, as in the spec.
    #[serde(default, deserialize_with = "crate::syntax::tags::deserialize")]
    pub tags: IndexMap<String, String>,
}

impl Defaults {
    /// The region each cloud's modules default to, by the cloud's id (`aws`), where this sets one
    /// (what `Lab::with_regions` takes).
    #[must_use]
    pub fn regions(&self) -> std::collections::BTreeMap<String, String> {
        self.cloud.iter().filter_map(|(c, v)| Some((c.clone(), v.region.clone()?))).collect()
    }
}

/// The defaults' `vagrant:` section.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vagrant {
    /// The provider a runner asks for (`virtualbox`, `libvirt`, `vmware_desktop`,
    /// `parallels`, `utm`, `qemu`, `hyperv`, `vmware_esxi`). Default: Vagrant's own choice.
    #[serde(default)]
    pub provider: Option<String>,
}

/// One cloud's entry in the defaults' `cloud:` section.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cloud {
    /// The region the generated module defaults to.
    #[serde(default)]
    pub region: Option<String>,
}

/// One layer's contribution: where it came from, and what it set.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// Where the layer comes from (a file, `--images`, an `ISOLOOM_*` variable, `-s`), as `isoloom
    /// defaults` shows it.
    pub source: String,
    /// What the layer sets, as YAML.
    pub value: Value,
}

/// The merged defaults with, for every leaf, the layer that set it last.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    /// The merged defaults, typed.
    pub defaults: Defaults,
    /// The merged defaults, as YAML (what `isoloom defaults` prints).
    pub merged: Value,
    /// Dotted key -> source, for `isoloom defaults`.
    pub sources: Vec<(String, String)>,
}

/// The user's defaults file, in Isoloom's home on `host`.
#[must_use]
pub fn user_file(host: &Host) -> PathBuf {
    host.home().join("defaults.yml")
}

/// What a project's defaults are read from: the project, the `--images` table, its profile, the
/// command's `-s` and the host it runs on.
#[derive(Debug, Clone)]
pub struct Sources<'a> {
    /// The project's folder.
    pub project: &'a Path,
    /// The `--images` table, between the files and `sets`.
    pub images_file: Option<&'a Path>,
    /// The profile, between the project's file and `--images`.
    pub profile: Option<&'a crate::profile::Profile>,
    /// The command's `-s` (`defaults.cloud.aws.region=eu-west-1`, the prefix optional).
    pub sets: &'a [String],
    /// The host: Isoloom's home, and the `ISOLOOM_*` variables.
    pub host: Host,
}

/// Reads every layer for a project: the user's file, the project's, the profile, `--images`,
/// the host's `ISOLOOM_*` variables, then the `-s` overrides.
///
/// # Errors
///
/// [`Error::Read`] for a file that can't be read (one that doesn't exist is no layer),
/// [`Error::Layer`] for one that isn't YAML, [`Error::SetShape`] and [`Error::SetKey`] for a
/// `-s` that isn't `key=value`, and [`Error::Defaults`] when the merged layers hold a key or a
/// value the defaults don't have.
pub fn load(from: &Sources) -> Result<Resolved> {
    let mut layers = Vec::new();
    for (label, path) in [("user file", user_file(&from.host)), ("project file", from.project.join(PROJECT_FILE))] {
        if let Some(value) = read_layer(&path)? {
            layers.push(Layer {
                source: format!("{label} {}", path.display()),
                value,
            });
        }
    }
    if let Some(p) = from.profile {
        layers.push(p.layer());
    }
    if let Some(f) = from.images_file {
        let text = std::fs::read_to_string(f).map_err(|source| Error::Read { path: f.to_path_buf(), source })?;
        let table: Value = serde_yaml_ng::from_str(&text).map_err(|source| Error::Layer { path: f.to_path_buf(), source })?;
        let mut m = Mapping::new();
        m.insert(Value::String("images".into()), table);
        layers.push(Layer {
            source: format!("--images {}", f.display()),
            value: Value::Mapping(m),
        });
    }
    for (k, v) in from_env(from.host.text_vars().into_iter()) {
        layers.push(Layer {
            source: format!("environment ISOLOOM_{}", k.replace('.', "__").to_uppercase()),
            value: at_path(&k, typed(&k, &v)),
        });
    }
    for s in from.sets {
        let (k, v) = parse_set(s)?;
        let Some(k) = k.strip_prefix("defaults.") else { continue };
        layers.push(Layer {
            source: format!("-s {s}"),
            value: at_path(k, typed(k, v)),
        });
    }
    resolve(&layers)
}

/// A layer's file, as YAML; none when it doesn't exist.
fn read_layer(path: &Path) -> Result<Option<Value>> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_yaml_ng::from_str(&text).map(Some).map_err(|source| Error::Layer {
            path: path.to_path_buf(),
            source,
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

impl Resolved {
    /// The layer that set the defaults' tag `key`, as `isoloom defaults` names it.
    fn tag_source(&self, key: &str) -> String {
        let leaf = format!("tags.{key}");
        self.sources
            .iter()
            .find(|(k, _)| *k == leaf)
            .map_or_else(|| "defaults".to_owned(), |(_, from)| from.clone())
    }

    /// The tag `key` of the defaults refused, and why.
    fn tag_refused(&self, key: &str, why: String) -> Error {
        Error::DefaultTag {
            key: key.to_owned(),
            from: self.tag_source(key),
            why,
        }
    }

    /// The defaults' tags with `own` (the spec's) over them: a key both set keeps the spec's.
    ///
    /// # Errors
    ///
    /// [`Error::DefaultTag`] when a tag of the defaults clashes with one of the spec where a
    /// cloud carries them (`team` and the spec's `Team`), naming the layer that set it: the
    /// spec's own clashes are its validation's.
    pub fn tags_under(&self, own: &IndexMap<String, String>) -> Result<IndexMap<String, String>> {
        let mut tags = self.defaults.tags.clone();
        tags.extend(own.iter().map(|(k, v)| (k.clone(), v.clone())));
        match crate::tags::clash(&tags) {
            Some(c) if !own.contains_key(c.earlier) => Err(self.tag_refused(c.earlier, format!("{}: name them apart", c.why))),
            _ => Ok(tags),
        }
    }

    /// Each of the defaults' tags by the spec's rules ([`crate::tags`]).
    fn checked_tags(self) -> Result<Resolved> {
        for (key, value) in &self.defaults.tags {
            if let Some(why) = crate::tags::key_refusal(key).or_else(|| crate::tags::value_refusal(key, value)) {
                return Err(self.tag_refused(key, why));
            }
        }
        if let Some(c) = crate::tags::clash(&self.defaults.tags) {
            return Err(self.tag_refused(c.key, format!("{}: name them apart", c.why)));
        }
        Ok(self)
    }
}

/// Merges layers in order; later ones win leaf by leaf.
///
/// # Errors
///
/// [`Error::Defaults`] when the merged layers hold a key or a value the defaults don't have,
/// [`Error::DefaultTag`] for a tag not every cloud takes.
pub fn resolve(layers: &[Layer]) -> Result<Resolved> {
    let mut merged = Value::Mapping(Mapping::new());
    let mut sources: Vec<(String, String)> = Vec::new();
    for layer in layers {
        merge(&mut merged, &layer.value);
        for leaf in leaves(&layer.value, "") {
            sources.retain(|(k, _)| *k != leaf);
            sources.push((leaf, layer.source.clone()));
        }
    }
    let defaults: Defaults = serde_yaml_ng::from_value(merged.clone()).map_err(|source| Error::Defaults {
        sources: layers.iter().map(|l| l.source.as_str()).collect::<Vec<_>>().join(", "),
        source,
    })?;
    Resolved { defaults, merged, sources }.checked_tags()
}

/// `ISOLOOM_CLOUD__AWS__REGION=x` -> `("cloud.aws.region", "x")`, `ISOLOOM_TAGS__TEAM=ops` ->
/// `("tags.team", "ops")`. Only the defaults' sections are read (`ISOLOOM_HOME`,
/// `ISOLOOM_DERIVED` and the publish variables mean other things).
pub fn from_env(vars: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vars
        .filter_map(|(k, v)| {
            let rest = k.strip_prefix("ISOLOOM_")?;
            let key = rest.to_lowercase().replace("__", ".");
            let section = key.split('.').next().unwrap_or("");
            ["images", "vagrant", "cloud", "tags"].contains(&section).then_some((key, v))
        })
        .collect();
    out.sort();
    out
}

/// `key=value` of a `-s`.
///
/// # Errors
///
/// [`Error::SetShape`] without an `=`, [`Error::SetKey`] when the key isn't a dotted path.
pub(crate) fn parse_set(s: &str) -> Result<(&str, &str)> {
    let (k, v) = s.split_once('=').ok_or_else(|| Error::SetShape { set: s.to_owned() })?;
    if k.is_empty() || k.split('.').any(str::is_empty) {
        return Err(Error::SetKey { set: s.to_owned() });
    }
    Ok((k, v))
}

/// The value `text` gives the key `key` from the environment or a `-s`: a tag's or an
/// environment variable's as written (both are text, so `ISOLOOM_TAGS__ACCOUNT=0x1F` is `0x1F`,
/// not 31, and `-s machines.web.environment.V=1.10` is `1.10`), any other as YAML reads it
/// (`3` a number, `true` a boolean, else text).
#[must_use]
pub(crate) fn typed(key: &str, text: &str) -> Value {
    let keys: Vec<&str> = key.split('.').collect();
    if keys.first() == Some(&"tags") || crate::as_written::environment(&keys) {
        return Value::String(text.to_owned());
    }
    serde_yaml_ng::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned()))
}

/// A nested mapping holding `value` at a dotted path.
pub(crate) fn at_path(path: &str, value: Value) -> Value {
    path.rsplit('.').fold(value, |inner, key| {
        let mut m = Mapping::new();
        m.insert(Value::String(key.to_owned()), inner);
        Value::Mapping(m)
    })
}

/// Deep-merges `from` into `into`: mappings key by key, anything else replaced.
pub fn merge(into: &mut Value, from: &Value) {
    match (into, from) {
        (Value::Mapping(a), Value::Mapping(b)) => {
            for (k, v) in b {
                // A key both have merges (two mappings deeply, anything else replaced).
                match a.get_mut(k) {
                    Some(existing) => merge(existing, v),
                    None => {
                        a.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (into, from) => *into = from.clone(),
    }
}

/// Dotted paths of every leaf of a value.
#[must_use]
pub fn leaves(v: &Value, prefix: &str) -> Vec<String> {
    match v {
        Value::Mapping(m) if !m.is_empty() => m
            .iter()
            .flat_map(|(k, c)| {
                let key = k.as_str().map_or_else(|| format!("{k:?}"), str::to_owned);
                leaves(c, &if prefix.is_empty() { key } else { format!("{prefix}.{key}") })
            })
            .collect(),
        _ => vec![prefix.to_owned()],
    }
}

/// The spec's YAML with `-s key=value` overrides applied (keys without the `defaults.` prefix),
/// before it is parsed: `machines.web.os=ubuntu@24.04`, `networks.lan.internet=false`.
///
/// # Errors
///
/// As [`override_spec_with`].
pub(crate) fn override_spec(yaml: &str, sets: &[String]) -> Result<String> {
    override_spec_with(yaml, None, sets)
}

/// The spec's YAML with `base` (a profile's `spec`) deep-merged in, then `-s key=value`
/// overrides applied (keys without the `defaults.` prefix), before it is parsed:
/// `machines.web.os=ubuntu@24.04`, `networks.lan.internet=false`. With nothing to apply, the text
/// is returned as it is, comments and all.
///
/// # Errors
///
/// [`Error::Yaml`] when the spec isn't YAML (or can't be written back), [`Error::SetShape`] for
/// a set without an `=`, [`Error::SetKey`] when its key isn't a dotted path.
pub fn override_spec_with(yaml: &str, base: Option<&Value>, sets: &[String]) -> Result<String> {
    let mut doc: Value = serde_yaml_ng::from_str(yaml)?;
    // Written back as text below: an environment value keeps its written text (`1.10`, not 1.1).
    crate::as_written::keep(yaml, &mut doc, crate::as_written::environment);
    let mut any = false;
    if let Some(b) = base.filter(|b| b.as_mapping().is_some_and(|m| !m.is_empty())) {
        merge(&mut doc, b);
        any = true;
    }
    for s in sets {
        let (k, v) = parse_set(s)?;
        if k.starts_with("defaults.") {
            continue;
        }
        merge(&mut doc, &at_path(k, typed(k, v)));
        any = true;
    }
    if !any {
        return Ok(yaml.to_owned());
    }
    Ok(serde_yaml_ng::to_string(&doc)?)
}

#[cfg(test)]
mod tests;
