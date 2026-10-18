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
//! without the `defaults.` prefix overrides the spec itself (`-s machines.web.vm.os=ubuntu@24.04`),
//! for one run.
//!
//! ```yaml
//! images:                 # the image table (see Images): OS images, the access machine
//!   os: { debian-12: { vagrant: my-org/debian-12 } }
//!   access: { docker: kalilinux/kali-rolling, vm: kali }
//! vagrant:
//!   provider: libvirt     # `isothrum up` passes --provider
//! cloud:
//!   aws: { region: eu-west-1 }
//!   azure: { region: westeurope }
//! ```

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::Deserialize;
use serde_yaml_ng::{Mapping, Value};

use crate::files::GeneratedFile;
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
    /// How `isothrum up` runs Vagrant.
    #[serde(default)]
    pub vagrant: Vagrant,
    /// Per cloud (`aws`, `azure`, `gcp`, `digitalocean`, `linode`, `oci`).
    #[serde(default)]
    pub cloud: IndexMap<String, Cloud>,
}

/// The defaults' `vagrant:` section.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vagrant {
    /// The provider `isothrum up` asks for (`virtualbox`, `libvirt`, `vmware_desktop`,
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
            value: at_path(&k, scalar(&v)),
        });
    }
    for s in from.sets {
        let (k, v) = parse_set(s)?;
        let Some(k) = k.strip_prefix("defaults.") else { continue };
        layers.push(Layer {
            source: format!("-s {s}"),
            value: at_path(k, scalar(v)),
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

/// Merges layers in order; later ones win leaf by leaf.
///
/// # Errors
///
/// [`Error::Defaults`] when the merged layers hold a key or a value the defaults don't have.
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
///
/// # Errors
///
/// [`Error::SetShape`] without an `=`, [`Error::SetKey`] when the key isn't a dotted path.
pub fn parse_set(s: &str) -> Result<(&str, &str)> {
    let (k, v) = s.split_once('=').ok_or_else(|| Error::SetShape { set: s.to_owned() })?;
    if k.is_empty() || k.split('.').any(str::is_empty) {
        return Err(Error::SetKey { set: s.to_owned() });
    }
    Ok((k, v))
}

/// A value as YAML would read it (`3` a number, `true` a boolean, else text).
#[must_use]
pub fn scalar(text: &str) -> Value {
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
/// before it is parsed: `machines.web.vm.os=ubuntu@24.04`, `networks.lab.internet=false`.
///
/// # Errors
///
/// As [`override_spec_with`].
pub fn override_spec(yaml: &str, sets: &[String]) -> Result<String> {
    override_spec_with(yaml, None, sets)
}

/// [`override_spec`] with `base` (a profile's `spec`) deep-merged first.
///
/// # Errors
///
/// [`Error::Yaml`] when the spec isn't YAML (or can't be written back), and the errors of
/// [`parse_set`].
pub fn override_spec_with(yaml: &str, base: Option<&Value>, sets: &[String]) -> Result<String> {
    let mut doc: Value = serde_yaml_ng::from_str(yaml)?;
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
        merge(&mut doc, &at_path(k, scalar(v)));
        any = true;
    }
    if !any {
        return Ok(yaml.to_owned());
    }
    Ok(serde_yaml_ng::to_string(&doc)?)
}

/// Applies the defaults that change generated files: each cloud module's region default.
/// (Images are applied to the spec before generating; `vagrant.provider` at run time.)
#[must_use]
pub fn apply_to_files(files: Vec<GeneratedFile>, d: &Defaults) -> Vec<GeneratedFile> {
    files
        .into_iter()
        .map(|mut f| {
            // .isoloom[-n]/cloud-vm/<cloud>/main.tf, .isoloom[-n]/cloud-docker/<cloud>/main.tf
            let parts: Vec<&str> = f.path.split('/').collect();
            if let [_, target, cloud, "main.tf"] = parts.as_slice()
                && target.starts_with("cloud-")
                && let Some(region) = d.cloud.get(*cloud).and_then(|c| c.region.as_deref())
            {
                f.contents = set_region(&f.contents, region);
            }
            f
        })
        .collect()
}

/// The module text with its `variable "region"` default replaced.
#[must_use]
pub fn set_region(tf: &str, region: &str) -> String {
    const VARIABLE: &str = "variable \"region\"";
    let replaced = tf.split_once(VARIABLE).and_then(|(before, rest)| {
        let (block, after) = rest.split_once("\n}")?;
        let (head, from_default) = block.split_once("default")?;
        let (to_quote, quoted) = from_default.split_once('"')?;
        let (_, past_quote) = quoted.split_once('"')?;
        Some(format!("{before}{VARIABLE}{head}default{to_quote}\"{region}\"{past_quote}\n}}{after}"))
    });
    replaced.unwrap_or_else(|| tf.to_owned())
}

/// What the built-in layer holds, for `isoloom defaults --system`: the clouds' regions (the
/// image table is documented on its own page and read from the code).
#[must_use]
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::testing::folder;

    /// A test's outcome: a failed step is its report.
    type Outcome = std::result::Result<(), Box<dyn std::error::Error>>;

    /// A host with these environment variables.
    fn host(vars: &[(&str, &str)]) -> Host {
        Host::new(vars.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))))
    }

    /// The region `d` gives `cloud`.
    fn region<'d>(d: &'d Defaults, cloud: &str) -> Option<&'d str> {
        d.cloud.get(cloud)?.region.as_deref()
    }

    #[test]
    fn the_users_file_is_in_isoloom_home() {
        assert_eq!(user_file(&host(&[("ISOLOOM_HOME", "/srv/iso")])), PathBuf::from("/srv/iso/defaults.yml"));
    }

    #[test]
    fn layers_merge_in_order_and_each_leaf_names_the_last_that_set_it() -> Outcome {
        let home = folder("defaults-layers-home")?;
        let project = folder("defaults-layers-project")?;
        std::fs::write(home.join("defaults.yml"), "cloud: { aws: { region: from-user }, gcp: { region: from-user } }\n")?;
        std::fs::write(project.join(PROJECT_FILE), "cloud: { aws: { region: from-project } }\n")?;
        let home_text = home.to_string_lossy().into_owned();
        let sets = ["defaults.vagrant.provider=libvirt".to_owned(), "machines.web.os=alpine".to_owned()];
        let resolved = load(&Sources {
            project: &project,
            images_file: None,
            profile: None,
            sets: &sets,
            host: host(&[("ISOLOOM_HOME", &home_text), ("ISOLOOM_CLOUD__AZURE__REGION", "from-env")]),
        })?;
        let d = &resolved.defaults;
        assert_eq!(region(d, "aws"), Some("from-project"));
        assert_eq!(region(d, "gcp"), Some("from-user"), "a file that sets one region leaves the others alone");
        assert_eq!(region(d, "azure"), Some("from-env"));
        assert_eq!(d.vagrant.provider.as_deref(), Some("libvirt"));
        let user = format!("user file {}", home.join("defaults.yml").display());
        let sources: Vec<(&str, &str)> = resolved.sources.iter().map(|(k, s)| (k.as_str(), s.as_str())).collect();
        assert_eq!(
            sources,
            [
                ("cloud.gcp.region", user.as_str()),
                ("cloud.aws.region", &format!("project file {}", project.join(PROJECT_FILE).display())),
                ("cloud.azure.region", "environment ISOLOOM_CLOUD__AZURE__REGION"),
                ("vagrant.provider", "-s defaults.vagrant.provider=libvirt"),
            ],
            "a `-s` without the `defaults.` prefix is the spec's, not a layer"
        );
        Ok(())
    }

    #[test]
    fn a_layer_file_that_cant_be_read_is_an_error_unlike_a_missing_one() -> Outcome {
        let project = folder("defaults-unreadable")?;
        let sources = |folder| Sources {
            project: folder,
            images_file: None,
            profile: None,
            sets: &[],
            host: host(&[("ISOLOOM_HOME", "/nonexistent/isoloom")]),
        };
        assert_eq!(load(&sources(&project))?.defaults, Defaults::default(), "no file, no layer");
        std::fs::create_dir(project.join(PROJECT_FILE))?;
        assert!(matches!(load(&sources(&project)), Err(Error::Read { path, .. }) if path == project.join(PROJECT_FILE)));
        Ok(())
    }

    #[test]
    fn only_the_defaults_sections_are_read_from_the_environment() {
        let vars = [
            ("ISOLOOM_VAGRANT__PROVIDER", "qemu"),
            ("ISOLOOM_HOME", "/x"),
            ("PATH", "/bin"),
            ("ISOLOOM_IMAGES__ACCESS__VM", "kali"),
        ];
        assert_eq!(
            from_env(vars.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))),
            [
                ("images.access.vm".to_owned(), "kali".to_owned()),
                ("vagrant.provider".to_owned(), "qemu".to_owned())
            ]
        );
    }

    #[test]
    fn a_set_is_a_dotted_key_then_its_value() {
        assert!(matches!(parse_set("a.b=c=d"), Ok(("a.b", "c=d"))));
        assert!(matches!(parse_set("a.b"), Err(Error::SetShape { .. })));
        for s in ["=1", "a..b=1", ".a=1"] {
            assert!(matches!(parse_set(s), Err(Error::SetKey { .. })), "{s}");
        }
    }

    #[test]
    fn an_empty_mapping_is_a_leaf_of_its_own() -> Outcome {
        let v: Value = serde_yaml_ng::from_str("cloud: { aws: {} }\nvagrant: { provider: qemu }\n")?;
        assert_eq!(leaves(&v, ""), ["cloud.aws", "vagrant.provider"]);
        Ok(())
    }

    /// A cloud module whose region defaults to `region`, between two other variables.
    fn module(region: &str) -> String {
        format!(
            "variable \"zone\" {{\n  default = \"a\"\n}}\nvariable \"region\" {{\n  type    = string\n  default = \"{region}\"\n}}\nvariable \"size\" {{\n  default = \"s\"\n}}\n"
        )
    }

    #[test]
    fn a_region_replaces_only_the_region_variables_default() {
        assert_eq!(set_region(&module("eu-west-3"), "eu-north-1"), module("eu-north-1"));
        assert_eq!(
            set_region("variable \"zone\" {}\n", "eu-north-1"),
            "variable \"zone\" {}\n",
            "no region, no change"
        );
    }

    #[test]
    fn regions_reach_each_clouds_module_and_nothing_else() {
        let file = |path: &str| GeneratedFile {
            path: path.to_owned(),
            contents: module("old"),
        };
        let d = Defaults {
            cloud: [("aws".to_owned(), Cloud { region: Some("new".into()) })].into_iter().collect(),
            ..Defaults::default()
        };
        let paths = [
            ".isoloom/cloud-vm/aws/main.tf",
            ".isoloom-2/cloud-docker/aws/main.tf",
            ".isoloom/cloud-vm/gcp/main.tf",
            ".isoloom/cloud-vm/aws/variables.tf",
            ".isoloom/vagrant/aws/main.tf",
            ".isoloom/cloud-vm/aws/extra/main.tf",
        ];
        let regions: Vec<bool> = apply_to_files(paths.iter().map(|p| file(p)).collect(), &d)
            .iter()
            .map(|f| f.contents == module("new"))
            .collect();
        assert_eq!(regions, [true, true, false, false, false, false]);
    }

    #[test]
    fn the_built_in_layer_holds_every_clouds_region() -> Outcome {
        let d: Defaults = serde_yaml_ng::from_value(builtin())?;
        let regions: Vec<(&str, &str)> = d.cloud.iter().filter_map(|(c, r)| Some((c.as_str(), r.region.as_deref()?))).collect();
        assert_eq!(regions, BUILTIN_REGIONS);
        Ok(())
    }
}
