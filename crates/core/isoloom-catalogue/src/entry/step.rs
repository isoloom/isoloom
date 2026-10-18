//! An entry's recipe: its phases (`install`, `configure`, `start`), each a list of steps of the
//! configuration model. An entry's step may also carry `when:` (it applies only where its
//! conditions hold) and `for_each:` (it repeats for each item of a list setting). A step holds
//! placeholders until it is resolved for a machine ([`crate::Resolved`]), so it is kept as
//! written here and parsed into the spec's [`Step`](isoloom_spec::syntax::Step) once filled in.

use std::borrow::Cow;

use isoloom_spec::syntax::OsRef;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer};

use crate::names::SettingName;

/// The recipe, phase by phase, in the order they run.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// Installs the service: skipped on the official image's form, which is this phase.
    #[serde(default)]
    pub install: Vec<EntryStep>,
    /// Configures it, in every form (on the official image, as a thin build over it).
    #[serde(default)]
    pub configure: Vec<EntryStep>,
    /// Prepares each start: host keys, a first-start database, passwords from secrets. Runs
    /// before the services at each start, with the secrets and inputs as files.
    #[serde(default)]
    pub start: Vec<EntryStep>,
}

impl Recipe {
    /// The phases, by name, in order.
    #[must_use]
    pub fn phases(&self) -> [(&'static str, &[EntryStep]); 3] {
        [("install", &self.install), ("configure", &self.configure), ("start", &self.start)]
    }
}

/// One step of an entry, as written: the step itself, with its placeholders, and when it applies.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryStep {
    /// The step as written (`reboot`, or a map with one kind key), without `when` and
    /// `for_each`.
    pub body: serde_json::Value,
    /// Where it applies; everywhere when `None`.
    pub when: Option<When>,
    /// The list setting it repeats for, with `{{ item.<field> }}` for each item's fields.
    pub for_each: Option<SettingName>,
}

impl<'de> Deserialize<'de> for EntryStep {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<EntryStep, D::Error> {
        let mut body = serde_json::Value::deserialize(d)?;
        let Some(map) = body.as_object_mut() else {
            return Ok(EntryStep {
                body,
                when: None,
                for_each: None,
            });
        };
        let when = map
            .shift_remove("when")
            .map(serde_json::from_value::<When>)
            .transpose()
            .map_err(serde::de::Error::custom)?;
        let for_each = map
            .shift_remove("for_each")
            .map(serde_json::from_value::<SettingName>)
            .transpose()
            .map_err(serde::de::Error::custom)?;
        Ok(EntryStep { body, when, for_each })
    }
}

impl JsonSchema for EntryStep {
    fn schema_name() -> Cow<'static, str> {
        "EntryStep".into()
    }

    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        let when = g.subschema_for::<When>();
        json_schema!({
            "description": "A step of the configuration model (the spec's `app.steps`: `reboot`, or a map with one kind key), its texts holding `{{ version }}`, `{{ os.* }}`, `{{ vars.* }}`, `{{ with.* }}` and `{{ item.* }}`. A repository's `key` may be `{ url, fingerprint }`: the refresh pins its SHA-256 in lock.yml.",
            "anyOf": [
                { "const": "reboot" },
                {
                    "type": "object",
                    "properties": {
                        "when": when,
                        "for_each": { "type": "string", "description": "A list setting: the step repeats for each of its items, with `{{ item.<field> }}`." }
                    },
                    "additionalProperties": true
                }
            ]
        })
    }
}

/// Where an entry's step applies: every condition given holds (a list holds when one of its
/// values does). Never the form: both forms run the same recipe.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct When {
    /// The machine's OS family, as `os:` writes it (`debian`, `ubuntu`, `rocky`, `alpine`).
    #[serde(default)]
    pub family: Option<Vec<OsFamily>>,
    /// The machine's OS, in full (`debian@12`).
    #[serde(default)]
    pub os: Option<Vec<OsRef>>,
    /// The release: a channel (`mainline`) or a version or its prefix (`1.30`).
    #[serde(default)]
    pub release: Option<Vec<String>>,
    /// A setting `with:` gives (or that has a default).
    #[serde(default)]
    pub set: Option<SettingName>,
    /// A setting `with:` leaves out, with no default.
    #[serde(default)]
    pub unset: Option<SettingName>,
}

/// An OS family as a machine's `os:` writes it, before the `@` (`debian`, `rocky`): one of the
/// OS table's.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OsFamily(&'static str);

impl OsFamily {
    /// The family's name.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

impl<'de> Deserialize<'de> for OsFamily {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<OsFamily, D::Error> {
        let text = String::deserialize(d)?;
        let families = isoloom_spec::os::FAMILIES;
        families.iter().find(|f| f.name == text).map(|f| OsFamily(f.name)).ok_or_else(|| {
            let known: Vec<&str> = families.iter().map(|f| f.name).collect();
            serde::de::Error::custom(format!("`{text}` isn't an OS family; the OS table has: {}", known.join(", ")))
        })
    }
}

impl JsonSchema for OsFamily {
    fn schema_name() -> Cow<'static, str> {
        "OsFamily".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let names: Vec<&str> = isoloom_spec::os::FAMILIES.iter().map(|f| f.name).collect();
        json_schema!({ "type": "string", "enum": names })
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// A step's own fields: a key, and a text or a list of texts.
    fn body() -> impl Strategy<Value = serde_json::Map<String, serde_json::Value>> {
        let value = prop_oneof![
            "[a-z{} ./]{0,12}".prop_map(serde_json::Value::from),
            proptest::collection::vec("[a-z]{1,6}", 0..3).prop_map(serde_json::Value::from),
        ];
        proptest::collection::btree_map("[a-z_]{1,8}", value, 0..4).prop_map(|m| m.into_iter().filter(|(k, _)| k != "when" && k != "for_each").collect())
    }

    proptest! {
        /// Whatever a step holds, the reader keeps it as written and takes `when` and `for_each`
        /// apart, never failing on the step's own fields.
        #[test]
        fn a_step_reads_back_as_written(fields in body(), unset in proptest::option::of("[a-z][a-z0-9_]{0,6}"), each in proptest::option::of("[a-z][a-z0-9_]{0,6}")) {
            let mut written = fields.clone();
            if let Some(u) = &unset {
                written.insert("when".into(), serde_json::json!({ "unset": u }));
            }
            if let Some(e) = &each {
                written.insert("for_each".into(), serde_json::json!(e));
            }
            let step: EntryStep = serde_json::from_value(serde_json::Value::Object(written)).unwrap();
            prop_assert_eq!(step.body, serde_json::Value::Object(fields));
            prop_assert_eq!(step.when.and_then(|w| w.unset).map(|s| s.to_string()), unset);
            prop_assert_eq!(step.for_each.map(|s| s.to_string()), each);
        }
    }

    #[test]
    fn a_step_keeps_its_body_apart_from_when_and_for_each() {
        let step: EntryStep = serde_yaml_ng::from_str("{ packages: [nginx], when: { family: [debian, ubuntu], unset: config }, for_each: users }").unwrap();
        assert_eq!(step.body, serde_json::json!({ "packages": ["nginx"] }));
        let when = step.when.unwrap();
        assert_eq!(when.family.unwrap().iter().map(OsFamily::as_str).collect::<Vec<_>>(), ["debian", "ubuntu"]);
        assert_eq!(when.unset.unwrap().as_str(), "config");
        assert_eq!(step.for_each.unwrap().as_str(), "users");
        let reboot: EntryStep = serde_yaml_ng::from_str("reboot").unwrap();
        assert_eq!((reboot.body, reboot.when), (serde_json::json!("reboot"), None));
    }

    #[test]
    fn a_when_names_only_what_it_has() {
        let form = serde_yaml_ng::from_str::<EntryStep>("{ file: x, when: { form: vm } }").unwrap_err().to_string();
        assert!(form.contains("unknown field `form`"), "{form}");
        let family = serde_yaml_ng::from_str::<EntryStep>("{ file: x, when: { family: [gentoo] } }")
            .unwrap_err()
            .to_string();
        assert!(family.contains("`gentoo` isn't an OS family"), "{family}");
    }
}
