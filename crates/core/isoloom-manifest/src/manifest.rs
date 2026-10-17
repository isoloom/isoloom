//! The run manifest itself: its fields, and how it is read (JSON, then the version, then the
//! types, then what spans fields) and written.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::guest::Guest;
use crate::identity::{Cloud, Driver, HostOs, Identity, Target, Tool};
use crate::lifecycle::{Condition, Expiry, Lifecycle, Recovery};
use crate::machine::{Machine, Network};
use crate::outputs::{Checks, Output};
use crate::provenance::{Generator, Lab, ManifestVersion, SUPPORTED_VERSION};
use crate::slots::{Param, SecretSlot};
use crate::{Error, Result};

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
    /// The environment the files were generated from (serialized as `environment`).
    #[serde(rename = "environment")]
    pub lab: Lab,
    /// The target whose files it sits next to.
    pub target: Target,
    /// The cloud of a cloud target's module; absent elsewhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<Cloud>,
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
    /// The VM the lab's containers run in, when Compose runs inside one (`docker-vm`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest: Option<Guest>,
}

/// Reads a manifest: JSON, then version 0, then the types, then [`Manifest::validate`].
///
/// # Errors
///
/// The first problem found: [`Error::Syntax`] for text that isn't JSON,
/// [`Error::Version`] for another version of the contract, [`Error::Field`] for a
/// value the types refuse (its path and reason), else what [`Manifest::validate`] finds.
pub fn parse(text: &str) -> Result<Manifest> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| Error::Syntax {
        line: e.line(),
        column: e.column(),
        reason: e.to_string(),
    })?;
    // The version first: another version's manifest fails on it, not on a field it renamed.
    if let Some(found) = value.get("manifest_version").filter(|v| **v != SUPPORTED_VERSION) {
        return Err(Error::Version { found: found.to_string() });
    }
    let manifest: Manifest = serde_path_to_error::deserialize(value).map_err(|e| Error::Field {
        path: e.path().to_string(),
        reason: e.into_inner().to_string(),
    })?;
    manifest.validate()?;
    Ok(manifest)
}

/// The manifest as Isoloom writes it: two-space JSON, keys in the types' order, a final newline.
///
/// # Errors
///
/// [`Error::Field`] at `.` if JSON can't hold it, which no value of these types does.
pub fn to_json(manifest: &Manifest) -> Result<String> {
    serde_json::to_string_pretty(manifest).map(|text| text + "\n").map_err(|e| Error::Field {
        path: ".".into(),
        reason: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_crosses_threads() {
        fn shared<T: Send + Sync>() {}
        shared::<Manifest>();
        shared::<Error>();
    }

    #[test]
    fn another_version_is_refused_before_its_fields() {
        let err = parse(r#"{ "manifest_version": 1, "renamed": true }"#).unwrap_err();
        assert_eq!(err.code(), "version");
        assert_eq!(parse("{ not json").unwrap_err().code(), "syntax");
        let field = parse(r#"{ "manifest_version": 0 }"#).unwrap_err();
        assert!(matches!(&field, Error::Field { path, .. } if path == "."), "{field}");
    }

    #[test]
    fn a_written_manifest_ends_with_a_newline_and_reads_back() {
        let manifest = parse(include_str!("../fixtures/minimal.run.json")).unwrap();
        let written = to_json(&manifest).unwrap();
        assert!(written.ends_with("}\n") && written.starts_with("{\n  \"manifest_version\": 0,"), "{written}");
        assert_eq!(parse(&written).unwrap(), manifest);
    }
}
