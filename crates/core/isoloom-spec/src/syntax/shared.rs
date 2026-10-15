//! `common:` and `groups:`: machine fields several machines share. They take any machine field
//! but what makes a machine itself (`networks`, `ports`, `access`, `count`, `aliases`); a
//! machine's own value wins over its groups' (the most specific first), and theirs over
//! `common:`. The fields fold into the machines before the machines are read ([`crate::groups`]),
//! so a value is checked where it lands.

use std::borrow::Cow;
use std::fmt;

use indexmap::IndexMap;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::machine::MachineInput;

/// The machine fields `common:` and groups can't share: each is one machine's own.
pub(crate) const OWN_FIELDS: &[&str] = &["networks", "ports", "access", "count", "aliases"];

/// The machine fields `common:` and groups share.
pub(crate) const SHARED_FIELDS: &[&str] = &[
    "os",
    "service",
    "with",
    "app",
    "command",
    "setup",
    "healthcheck",
    "depends_on",
    "environment",
    "files",
    "volumes",
    "disks",
    "tmpfs",
    "resources",
    "hardware",
    "dns",
    "privileged",
    "read_only",
    "shm_size",
    "sysctls",
    "ulimits",
    "rate_limit",
];

/// Shared machine fields, as written (checked where they land, in each machine).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Shared(pub IndexMap<String, serde_json::Value>);

/// A group: the machines in it, and the fields they share.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Group {
    /// Machine names, globs (`ws*`), or other groups.
    pub members: Vec<String>,
    /// The fields its members share.
    pub shared: Shared,
}

/// Reads shared fields from a map; with `members`, a group's.
struct Fields {
    group: bool,
}

impl<'de> Visitor<'de> for Fields {
    type Value = Group;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a map of machine fields")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Group, A::Error> {
        let mut out = Group::default();
        while let Some(key) = map.next_key::<String>()? {
            if self.group && key == "members" {
                out.members = map.next_value()?;
            } else if OWN_FIELDS.contains(&key.as_str()) {
                return Err(de::Error::custom(format!("`{key}` is each machine's own: write it on the machines")));
            } else if SHARED_FIELDS.contains(&key.as_str()) {
                out.shared.0.insert(key, map.next_value()?);
            } else {
                return Err(de::Error::unknown_field(&key, SHARED_FIELDS));
            }
        }
        if self.group && out.members.is_empty() {
            return Err(de::Error::custom("a group names its `members`"));
        }
        Ok(out)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Group, E> {
        Ok(Group::default())
    }
}

impl<'de> Deserialize<'de> for Shared {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Shared, D::Error> {
        d.deserialize_any(Fields { group: false }).map(|g| g.shared)
    }
}

impl<'de> Deserialize<'de> for Group {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Group, D::Error> {
        d.deserialize_map(Fields { group: true })
    }
}

/// A machine's schema without its own fields, and nothing required.
fn shared_schema(g: &mut SchemaGenerator) -> Schema {
    let mut s = MachineInput::json_schema(g);
    if let Some(props) = s.get_mut("properties").and_then(serde_json::Value::as_object_mut) {
        props.retain(|k, _| !OWN_FIELDS.contains(&k.as_str()));
    }
    s.remove("required");
    s
}

impl JsonSchema for Shared {
    fn schema_name() -> Cow<'static, str> {
        "Shared".into()
    }

    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        let mut s = shared_schema(g);
        s.insert("description".into(), "Machine fields every machine shares (a machine's own value wins).".into());
        s
    }
}

impl JsonSchema for Group {
    fn schema_name() -> Cow<'static, str> {
        "Group".into()
    }

    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        let mut s = shared_schema(g);
        let members = g.subschema_for::<Vec<String>>();
        if let Some(props) = s.get_mut("properties").and_then(serde_json::Value::as_object_mut) {
            let mut m = members.to_value();
            if let Some(o) = m.as_object_mut() {
                o.insert("description".into(), "Machine names, globs (`ws*`), or other groups.".into());
            }
            props.insert("members".into(), m);
        }
        s.insert("required".into(), serde_json::json!(["members"]));
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_fields_left_empty_share_nothing() {
        assert_eq!(serde_yaml_ng::from_str::<Shared>("~").unwrap(), Shared::default());
        let wrong = serde_yaml_ng::from_str::<Group>("[a]").unwrap_err().to_string();
        assert!(wrong.contains("a map of machine fields"), "{wrong}");
    }

    #[test]
    fn the_shared_fields_are_a_machines_but_its_own() {
        let schema = MachineInput::json_schema(&mut SchemaGenerator::default());
        let mut fields: Vec<&str> = schema.get("properties").unwrap().as_object().unwrap().keys().map(String::as_str).collect();
        fields.retain(|f| !OWN_FIELDS.contains(f));
        let mut shared = SHARED_FIELDS.to_vec();
        fields.sort_unstable();
        shared.sort_unstable();
        assert_eq!(fields, shared);
    }

    #[test]
    fn groups_share_fields_but_not_a_machines_own() {
        let g: Group = serde_yaml_ng::from_str("{ members: [dc01, ws*], resources: { cpus: 2 }, os: windows-server@2019 }").unwrap();
        assert_eq!((g.members.len(), g.shared.0.len()), (2, 2));
        let e = serde_yaml_ng::from_str::<Group>("{ members: [a], networks: { lab: 10 } }")
            .unwrap_err()
            .to_string();
        assert!(e.contains("each machine's own"), "{e}");
        assert!(
            serde_yaml_ng::from_str::<Shared>("{ docker: { image: x } }")
                .unwrap_err()
                .to_string()
                .contains("unknown field `docker`")
        );
        assert!(serde_yaml_ng::from_str::<Group>("{ resources: { cpus: 2 } }").is_err());
    }
}
