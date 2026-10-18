//! The matrix's rows as `probes.yml` writes them, read once: each row's change to the lab, the
//! lab each column decides on, and what makes a spec use the row.

use isoloom_plan::Feature;
use isoloom_spec::model::Target;
use serde::Deserialize;
use serde_yaml_ng::{Mapping, Value};

use crate::{Error, Result};

/// The rows, as written.
const DATA: &str = include_str!("probes.yml");

/// The file: the lab, `hybrid`'s two more machines, the cloud lab, the rows.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    base: Value,
    hybrid: Value,
    cloud: Value,
    rows: Vec<RowFile>,
}

/// A row as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RowFile {
    field: String,
    #[serde(default)]
    value: Option<String>,
    about: String,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    uses: Option<String>,
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    also: Vec<String>,
    #[serde(default)]
    without: Vec<String>,
    #[serde(default)]
    only: Vec<String>,
    #[serde(default)]
    cloud: bool,
    #[serde(default = "yes")]
    hosts: bool,
    patch: Value,
}

/// `true`, a default.
fn yes() -> bool {
    true
}

/// What makes a spec use a row.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Uses {
    /// The field written in the spec, as [`Written`] says.
    Written(Written),
    /// One of the row's features among the spec's requirements on the target.
    Features,
    /// Lowering refusing the spec on the target at the row's field: a row about a field the spec
    /// lacks (a machine with no existing host on `external`).
    Refusal,
}

/// A field written in a spec: at `field` (`*` any name, `[]` any item), with one of `values`
/// when there are any (a `*` in one stands for any text), with at least `count` entries, and
/// with the fields `also` and none of `without` on the same machine, network or item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Written {
    pub(crate) field: String,
    pub(crate) values: Vec<String>,
    pub(crate) count: Option<usize>,
    pub(crate) also: Vec<String>,
    pub(crate) without: Vec<String>,
}

/// A row: a field, why it is there, its features, its change to the lab and where it applies.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Probe {
    pub(crate) field: String,
    pub(crate) value: Option<String>,
    pub(crate) about: String,
    pub(crate) features: Vec<Feature>,
    pub(crate) uses: Uses,
    /// The targets it applies to; empty: every target.
    pub(crate) only: Vec<Target>,
    /// A row of the cloud-services lab.
    pub(crate) cloud: bool,
    /// Whether `external`'s lab gives each machine an existing host.
    pub(crate) hosts: bool,
    pub(crate) patch: Value,
}

/// The labs the rows change, and the rows.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Probes {
    /// The one-machine lab.
    pub(crate) base: Value,
    /// The same, with `hybrid`'s container-only and VM-only machines beside.
    pub(crate) hybrid: Value,
    /// The lab of cloud services.
    pub(crate) cloud: Value,
    pub(crate) rows: Vec<Probe>,
}

/// The rows of `probes.yml`, then one per OS of the spec's table.
pub(crate) fn probes() -> Result<Probes> {
    let file: File = serde_yaml_ng::from_str(DATA).map_err(|e| invalid("probes.yml", &e.to_string()))?;
    let mut rows = file.rows.into_iter().map(row).collect::<Result<Vec<_>>>()?;
    rows.extend(os_rows());
    let mut hybrid = file.base.clone();
    merge(&mut hybrid, &mapping([("machines", file.hybrid)]));
    Ok(Probes {
        base: file.base,
        hybrid,
        cloud: file.cloud,
        rows,
    })
}

/// The error for a row that names what doesn't exist.
fn invalid(row: &str, what: &str) -> Error {
    Error::Probe {
        row: row.to_owned(),
        what: what.to_owned(),
    }
}

/// A row as typed: its features and targets known.
fn row(r: RowFile) -> Result<Probe> {
    let features = r
        .features
        .iter()
        .map(|id| {
            Feature::ALL
                .iter()
                .copied()
                .find(|f| f.id() == id)
                .ok_or_else(|| invalid(&r.field, &format!("no feature `{id}`")))
        })
        .collect::<Result<Vec<_>>>()?;
    let only = r
        .only
        .iter()
        .map(|id| {
            Target::ALL
                .into_iter()
                .find(|t| t.id() == id)
                .ok_or_else(|| invalid(&r.field, &format!("no target `{id}`")))
        })
        .collect::<Result<Vec<_>>>()?;
    let uses = match r.uses.as_deref() {
        None => Uses::Written(Written {
            field: r.field.clone(),
            values: r.value.iter().cloned().collect(),
            count: r.count,
            also: r.also,
            without: r.without,
        }),
        Some("features") if !features.is_empty() => Uses::Features,
        Some("refusal") => Uses::Refusal,
        Some(other) => {
            return Err(invalid(
                &r.field,
                &format!("`uses: {other}`: `features` (with the row's features) or `refusal`"),
            ));
        }
    };
    Ok(Probe {
        field: r.field,
        value: r.value,
        about: r.about,
        features,
        uses,
        only,
        cloud: r.cloud,
        hosts: r.hosts,
        patch: r.patch,
    })
}

/// One row per OS of the spec's table: the machine is that OS alone.
fn os_rows() -> Vec<Probe> {
    isoloom_spec::os::FAMILIES
        .iter()
        .flat_map(|family| {
            family.versions.iter().map(move |(version, _)| {
                let id = format!("{}@{version}", family.name);
                let mut values = vec![id.clone()];
                if *version == family.default {
                    values.push(family.name.to_owned());
                }
                let mut m = Mapping::new();
                m.insert("os".into(), Value::String(id.clone()));
                m.insert("app".into(), Value::Null);
                let patch = mapping([("machines", mapping([("m", Value::Mapping(m))]))]);
                Probe {
                    field: "machines.*.os".to_owned(),
                    value: Some(id),
                    about: "the OS alone, nothing installed".to_owned(),
                    features: Vec::new(),
                    uses: Uses::Written(Written {
                        field: "machines.*.os".to_owned(),
                        values,
                        count: None,
                        also: Vec::new(),
                        without: Vec::new(),
                    }),
                    only: Vec::new(),
                    cloud: false,
                    hosts: true,
                    patch,
                }
            })
        })
        .collect()
}

/// A mapping of `entries`.
fn mapping<const N: usize>(entries: [(&str, Value); N]) -> Value {
    Value::Mapping(entries.into_iter().map(|(k, v)| (Value::String(k.to_owned()), v)).collect())
}

/// `patch` merged into `into`: a mapping into a mapping key by key, `null` removing the key,
/// anything else replacing.
pub(crate) fn merge(into: &mut Value, patch: &Value) {
    match (into, patch) {
        (Value::Mapping(target), Value::Mapping(changes)) => {
            for (key, change) in changes {
                if change.is_null() {
                    target.remove(key);
                } else if let Some(existing) = target.get_mut(key) {
                    merge(existing, change);
                } else {
                    target.insert(key.clone(), change.clone());
                }
            }
        }
        (slot, change) => *slot = change.clone(),
    }
}

/// Every machine of `lab` given an existing host (`overrides.external`), for the `external`
/// target, which reaches machines that exist already.
pub(crate) fn with_hosts(lab: &mut Value) {
    let names: Vec<Value> = lab
        .get("machines")
        .and_then(Value::as_mapping)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    let hosts: Mapping = names
        .into_iter()
        .zip(10..)
        .map(|(name, n): (Value, u8)| {
            let ssh = mapping([("address", Value::String(format!("192.0.2.{n}")))]);
            (name, mapping([("ssh", ssh)]))
        })
        .collect();
    let patch = mapping([("overrides", mapping([("external", mapping([("machines", Value::Mapping(hosts))]))]))]);
    merge(lab, &patch);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(text: &str) -> Value {
        serde_yaml_ng::from_str(text).unwrap()
    }

    #[test]
    fn a_patch_merges_mappings_replaces_the_rest_and_removes_with_null() {
        let mut lab = yaml("machines: { m: { os: debian@12, app: { image: x, steps: [a] } } }");
        merge(&mut lab, &yaml("machines: { m: { app: { steps: null, build: b } }, w: { os: alpine } }"));
        assert_eq!(lab, yaml("machines: { m: { os: debian@12, app: { image: x, build: b } }, w: { os: alpine } }"));
        merge(&mut lab, &yaml("machines: { m: { os: [1] } }"));
        assert_eq!(lab["machines"]["m"]["os"], yaml("[1]"));
    }

    #[test]
    fn every_machine_gets_an_existing_host() {
        let mut lab = yaml("machines: { m: {}, w: {} }");
        with_hosts(&mut lab);
        assert_eq!(lab["overrides"]["external"]["machines"]["w"]["ssh"]["address"], yaml("192.0.2.11"));
    }

    #[test]
    fn the_rows_read_and_each_os_of_the_table_has_one() {
        let probes = probes().unwrap();
        let os = probes.rows.iter().filter(|r| r.field == "machines.*.os").count();
        let versions: usize = isoloom_spec::os::FAMILIES.iter().map(|f| f.versions.len()).sum();
        assert_eq!(os, versions);
        assert!(probes.rows.iter().any(|r| r.value.as_deref() == Some("windows-server@2022")));
    }
}
