//! An entry's pins, `catalogue/<entry>/lock.yml`, written only by `cargo xtask catalogue
//! refresh` (design §4.8): each repository key's SHA-256 (a GPG key's fingerprint checked first),
//! and per release each official image's multi-platform digest, its platforms, its own
//! entrypoint and command, and each OS's exact package versions. People review intent in
//! `entry.yml`; a tool owns the pins.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use indexmap::IndexMap;
use isoloom_spec::model::Arch;
use isoloom_spec::syntax::{HttpsUrl, OsRef, PackageName, PackageVersion, Sha256};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::version::Version;

/// An entry's pins.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Lock {
    /// The day of the refresh that wrote it (`2026-10-16`).
    pub refreshed: String,
    /// Each repository key the recipe names, by its URL.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub keys: IndexMap<HttpsUrl, LockedKey>,
    /// Each release's pins, by its version.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub releases: IndexMap<Version, LockedRelease>,
}

/// A repository key, pinned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LockedKey {
    /// A GPG key's fingerprint, as the entry gives it and the refresh checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// The SHA-256 of the key's file.
    pub sha256: Sha256,
}

/// A release's pins.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LockedRelease {
    /// Each official image, by the OS it is used for.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub images: IndexMap<OsRef, LockedImage>,
    /// Each OS's exact package versions, by package (`nginx: 1.30.6-1~bookworm`).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub packages: IndexMap<OsRef, IndexMap<PackageName, PackageVersion>>,
}

/// An official image, pinned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LockedImage {
    /// The digest of its multi-platform index: what a run pulls, whatever the tag moves to.
    pub digest: Digest,
    /// The platforms it publishes.
    pub platforms: Vec<Arch>,
    /// Its own entrypoint, from its configuration (for `image.start: run`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entrypoint: Vec<String>,
    /// Its own command, from its configuration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
}

/// A content digest: `sha256:` and 64 lowercase hexadecimal digits.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Digest(String);

impl Digest {
    /// The digest as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Digest {
    type Err = String;

    /// A digest, when it is `sha256:` and 64 lowercase hexadecimal digits.
    ///
    /// # Errors
    ///
    /// What a digest is, naming the value.
    fn from_str(text: &str) -> Result<Digest, String> {
        let ok = text
            .strip_prefix("sha256:")
            .is_some_and(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
        if ok {
            Ok(Digest(text.to_owned()))
        } else {
            Err(format!("`{text}` isn't a digest: write `sha256:` and 64 lowercase hexadecimal digits"))
        }
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Digest {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Digest, D::Error> {
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Digest {
    fn schema_name() -> Cow<'static, str> {
        "Digest".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" })
    }
}

impl Lock {
    /// The image pinned for release `version` on `os`.
    #[must_use]
    pub fn image(&self, version: &Version, os: OsRef) -> Option<&LockedImage> {
        self.releases.get(version)?.images.get(&os)
    }

    /// The exact version of `package` pinned for release `version` on `os`.
    #[must_use]
    pub fn package(&self, version: &Version, os: OsRef, package: &PackageName) -> Option<&PackageVersion> {
        self.releases.get(version)?.packages.get(&os)?.get(package)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_digest_is_sha256_and_64_hex_digits() {
        assert!(format!("sha256:{}", "a".repeat(64)).parse::<Digest>().is_ok());
        assert!(format!("sha256:{}", "A".repeat(64)).parse::<Digest>().is_err());
        assert!("sha256:abc".parse::<Digest>().is_err());
        assert!("a".repeat(64).parse::<Digest>().is_err());
    }

    #[test]
    fn a_lock_finds_an_image_and_a_package_by_release_and_os() {
        let lock: Lock = serde_yaml_ng::from_str(&format!(
            "refreshed: 2026-10-16\nreleases:\n  '1.30.6':\n    images:\n      debian@13: {{ digest: 'sha256:{}', platforms: [amd64, arm64] }}\n    packages:\n      debian@12: {{ nginx: 1.30.6-1~bookworm }}\n",
            "0".repeat(64)
        ))
        .unwrap();
        let v: Version = "1.30.6".parse().unwrap();
        let debian13: OsRef = "debian@13".parse().unwrap();
        let debian12: OsRef = "debian@12".parse().unwrap();
        assert_eq!(lock.image(&v, debian13).map(|i| i.platforms.len()), Some(2));
        assert!(lock.image(&v, debian12).is_none());
        let nginx: PackageName = "nginx".parse().unwrap();
        assert_eq!(
            lock.package(&v, debian12, &nginx).map(ToString::to_string).as_deref(),
            Some("1.30.6-1~bookworm")
        );
    }
}
