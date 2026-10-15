//! The run manifest, version 0: the contract between Isoloom and the Isothrum runner.
//!
//! Isoloom writes one `run.json` next to a target's generated files; the runner executes it and
//! reads nothing else. The manifest holds the native commands in order (argv, never a shell), the
//! documented driver actions, readiness, outputs, check runners, and the slots that carry inputs
//! and secrets (names and destinations, never values).
//!
//! The types parse, they don't validate: a [`Manifest`] that parsed holds no shell as a program,
//! no `${secrets.…}` placeholder, no absolute or climbing path and no other version than 0.
//! [`Manifest::validate`] then checks what spans fields (step references, machines, placeholders).
//! The JSON Schema ([`schema`]) is generated from the types and committed at
//! `schemas/run-manifest-v0.schema.json`; the fixtures under `fixtures/` are shared with the
//! runner, which tests against the same files.

mod argv;
mod error;
mod identity;
mod lifecycle;
mod list;
mod machine;
mod names;
mod outputs;
mod path;
mod schema;
mod slots;
mod step;
mod template;
mod validate;
mod walk;

pub use argv::{Argv, HostArgv, PerHostArgv, Program};
pub use error::{ManifestError, SUPPORTED_VERSION};
pub use identity::{
    ComposeIdentity, Driver, Elevation, HostOs, Identity, Provider, StateOwner, Target, TerraformIdentity, Tool, VagrantIdentity, VagrantProvider,
};
pub use lifecycle::{Condition, Expiry, ExpiryUnit, Lifecycle, Park, Provision, ReadyGrammar, Recovery, Retries};
pub use list::{NonEmpty, StepSelection};
pub use machine::{Arch, Cidr, Machine, Native, Network, Port, Protocol, Publish, PublishMode, Resources, Role, Volume};
pub use names::{EnvName, Identifier, LabName, Name, OsName, PinnedImage, SnakeName, StepId, ToolName, Version};
pub use outputs::{Check, CheckKind, CheckProtocol, CheckRunner, Checks, Output, OutputRole, OutputSource};
pub use path::{GuestPath, PathTemplate, RelPath};
pub use schema::{SCHEMA_URL, schema};
pub use slots::{Delivery, Param, SecretSlot, SecretSource, SetBy, SlotKind, StateFileFormat, Store, TfVarShape, Transport};
pub use step::{Action, Failure, Run, Step, When};
pub use template::{Part, Placeholder, RunVar, Template};

use std::borrow::Cow;
use std::collections::BTreeSet;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The contract's version: 0, read only by a runner built for exactly 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
            Err(serde::de::Error::custom(ManifestError::Version { found: found.to_string() }))
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GeneratorName {
    /// Isoloom.
    Isoloom,
}

/// Provenance: a name and a version, nothing that changes between two runs of one Isoloom on one
/// spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Generator {
    /// The program.
    pub name: GeneratorName,
    /// Its version.
    pub version: Version,
}

/// The Isoloom instance a lab's files were generated for (1 to 99).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instance(u8);

impl Instance {
    /// The instance, when it is 1 to 99.
    pub fn new(n: u8) -> Result<Instance, ManifestError> {
        if (1..=99).contains(&n) {
            Ok(Instance(n))
        } else {
            Err(ManifestError::OutOfRange {
                what: "an instance",
                value: n.into(),
                min: 1,
                max: 99,
            })
        }
    }

    /// The number.
    pub fn get(self) -> u8 {
        self.0
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

/// The lab the files were generated from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Lab {
    /// The spec's `name:`, with the instance suffix when there is one (`shop-2`): the default run
    /// name.
    pub name: LabName,
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

/// A run manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "Isoloom run manifest, version 0")]
pub struct Manifest {
    /// The schema's URL, for editors.
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// The contract's version.
    pub manifest_version: ManifestVersion,
    /// The program that wrote it.
    pub generator: Generator,
    /// The lab.
    pub lab: Lab,
    /// The target whose files it sits next to.
    pub target: Target,
    /// The cloud of a cloud target's module; absent elsewhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<identity::Cloud>,
    /// The runner driver that implements its actions.
    pub driver: Driver,
    /// The host operating systems the target runs from.
    #[schemars(extend("minItems" = 1))]
    pub hosts: BTreeSet<HostOs>,
    /// The native tools the steps run, probed before `up`.
    pub tools: Vec<Tool>,
    /// How the runner finds what the run created.
    pub identity: Identity,
    /// Non-secret values set at run time.
    pub params: Vec<Param>,
    /// Input and secret slots: names, sources and destinations, never values.
    pub secrets: Vec<SecretSlot>,
    /// The lab's networks.
    pub networks: Vec<Network>,
    /// Every machine the target creates, Isoloom's own included (`role` tells them apart).
    pub machines: Vec<Machine>,
    /// The operations.
    pub lifecycle: Lifecycle,
    /// Conditions that must all hold, in order, once `lifecycle.up` succeeded.
    pub ready: Vec<Condition>,
    /// What the run gives back.
    pub outputs: Vec<Output>,
    /// The lab's checks.
    pub checks: Checks,
    /// How a start that tripped over leftovers is retried.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<Recovery>,
    /// When the run stops itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry: Option<Expiry>,
}

pub use identity::Cloud;

/// Reads a manifest: JSON, then version 0, then the types, then [`Manifest::validate`].
pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| ManifestError::Syntax {
        line: e.line(),
        column: e.column(),
        reason: e.to_string(),
    })?;
    // The version first: another version's manifest fails on it, not on a field it renamed.
    if let Some(found) = value.get("manifest_version").filter(|v| **v != SUPPORTED_VERSION) {
        return Err(ManifestError::Version { found: found.to_string() });
    }
    let manifest: Manifest = serde_path_to_error::deserialize(value).map_err(|e| ManifestError::Field {
        path: e.path().to_string(),
        reason: e.into_inner().to_string(),
    })?;
    manifest.validate()?;
    Ok(manifest)
}

/// The manifest as Isoloom writes it: two-space JSON, keys in the types' order, a final newline.
pub fn to_json(manifest: &Manifest) -> Result<String, ManifestError> {
    serde_json::to_string_pretty(manifest)
        .map(|text| text + "\n")
        .map_err(|e| ManifestError::Field {
            path: ".".into(),
            reason: e.to_string(),
        })
}
