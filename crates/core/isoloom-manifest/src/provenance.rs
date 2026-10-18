//! Where a manifest comes from: the contract's version, the program that wrote it and the
//! environment its files were generated from (its name, the Isoloom instance, the message shown
//! once it is ready). Nothing here changes between two runs of one Isoloom on one spec.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::names::{EnvironmentName, Version};
use crate::template::Template;
use crate::{Error, Result};

/// The manifest version this crate reads and writes.
pub const SUPPORTED_VERSION: u64 = 0;

/// The contract's version: 0, read only by a runner built for exactly 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ManifestVersion;

impl Serialize for ManifestVersion {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(SUPPORTED_VERSION)
    }
}

impl<'de> Deserialize<'de> for ManifestVersion {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<ManifestVersion, D::Error> {
        let found = serde_json::Value::deserialize(d)?;
        if found == SUPPORTED_VERSION {
            Ok(ManifestVersion)
        } else {
            Err(serde::de::Error::custom(Error::Version { found: found.to_string() }))
        }
    }
}

impl JsonSchema for ManifestVersion {
    fn schema_name() -> Cow<'static, str> {
        "ManifestVersion".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "description": "The contract's version. 0: unstable, read only by a runner built for exactly 0.", "const": SUPPORTED_VERSION })
    }
}

/// The program that wrote the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GeneratorName {
    /// Isoloom.
    Isoloom,
}

/// Provenance: a name and a version, nothing that changes between two runs of one Isoloom on one
/// spec.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Generator {
    /// The program.
    pub name: GeneratorName,
    /// Its version.
    pub version: Version,
}

/// The Isoloom instance an environment's files were generated for (1 to 99).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Instance(u8);

impl Instance {
    /// The instance, when it is 1 to 99.
    ///
    /// # Errors
    ///
    /// [`Error::OutOfRange`] for 0 or over 99.
    pub fn new(n: u8) -> Result<Instance> {
        if (1..=99).contains(&n) {
            Ok(Instance(n))
        } else {
            Err(Error::OutOfRange {
                what: "an instance",
                value: n.into(),
                min: 1,
                max: 99,
            })
        }
    }

    /// The number.
    #[must_use]
    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Instance {
    type Error = Error;

    fn try_from(n: u8) -> Result<Instance> {
        Instance::new(n)
    }
}

impl From<Instance> for u8 {
    fn from(i: Instance) -> u8 {
        i.0
    }
}

impl Serialize for Instance {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for Instance {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Instance, D::Error> {
        Instance::new(u8::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Instance {
    fn schema_name() -> Cow<'static, str> {
        "Instance".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "integer", "minimum": 1, "maximum": 99 })
    }
}

/// An instance, or null (a field that must be present).
fn nullable_instance(generator: &mut SchemaGenerator) -> Schema {
    json_schema!({ "anyOf": [generator.subschema_for::<Instance>(), { "type": "null" }] })
}

/// A field that must be present, and may be null.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}

/// The environment the files were generated from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentRef {
    /// The spec's `name:`, with the instance suffix when there is one (`shop-2`): the default run
    /// name.
    pub name: EnvironmentName,
    /// The Isoloom instance the files were generated for (moved Docker blocks, `.isoloom-<n>/`),
    /// or null.
    #[serde(deserialize_with = "present")]
    #[schemars(required, schema_with = "nullable_instance")]
    pub instance: Option<Instance>,
    /// The spec's `message:`, its placeholders rewritten to manifest ones (`{{
    /// machines.web.ports.site.published }}` becomes `${published.web.site}`). The runner fills it once
    /// the run is ready; a sensitive output is never substituted.
    #[serde(default)]
    pub message: Option<Template>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_version_0_is_read() {
        assert_eq!(serde_json::from_str::<ManifestVersion>("0").unwrap(), ManifestVersion);
        let other = serde_json::from_str::<ManifestVersion>("1").unwrap_err().to_string();
        assert!(other.starts_with("manifest_version 1: this reader understands version 0 only"), "{other}");
        assert_eq!(serde_json::to_string(&ManifestVersion).unwrap(), "0");
    }

    #[test]
    fn an_instance_is_1_to_99() {
        assert_eq!(Instance::new(7).map(Instance::get), Ok(7));
        assert_eq!(Instance::try_from(99).map(u8::from), Ok(99));
        for out in [0, 100] {
            assert_eq!(Instance::new(out).unwrap_err().code(), "out_of_range", "{out}");
        }
        assert!(serde_json::from_str::<Instance>("0").is_err());
        assert_eq!(serde_json::to_string(&Instance::new(3).unwrap()).unwrap(), "3");
    }

    #[test]
    fn an_environment_names_its_instance_even_when_it_has_none() {
        let env: EnvironmentRef = serde_json::from_str(r#"{ "name": "shop-2", "instance": 2 }"#).unwrap();
        assert_eq!(env.instance.map(Instance::get), Some(2));
        assert!(
            serde_json::from_str::<EnvironmentRef>(r#"{ "name": "shop" }"#)
                .unwrap_err()
                .to_string()
                .contains("instance")
        );
    }
}
