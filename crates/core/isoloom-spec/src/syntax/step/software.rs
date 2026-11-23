//! The steps that install software: `packages` (each by name, with its version or without) and
//! `repository` (a vendor's package repository, its key pinned by SHA-256).

use std::fmt;

use schemars::JsonSchema;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::super::mistake::Mistake;
use super::super::problem::Code;
use super::words::{AptWord, HttpsUrl, PackageName, PackageVersion, RepositoryName, Sha256};

/// A package to install: its name, and its version as its family's package manager writes it
/// (`1.30.6-1~bookworm`) when it is pinned. On Windows, a Windows feature (`Web-Server`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(from = "PackageInput")]
pub struct Package {
    /// Its name.
    pub name: PackageName,
    /// Its exact version, when pinned.
    pub version: Option<PackageVersion>,
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.version {
            Some(v) => write!(f, "{} {v}", self.name),
            None => write!(f, "{}", self.name),
        }
    }
}

/// A package written in full.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PackageFull {
    /// The package's name.
    name: PackageName,
    /// Its exact version, as the family's package manager writes it (`1.30.6-1~bookworm`,
    /// `1:2.4.62-1`, `1.30.6-r1`), quoted.
    #[serde(default)]
    version: Option<PackageVersion>,
}

// What the schema shows and the reader returns.
/// A package as written, for the schema.
#[derive(JsonSchema)]
#[serde(untagged)]
enum PackageInput {
    /// Its name: the version the package manager picks.
    Name(PackageName),
    /// Its name and its exact version.
    Pinned(PackageFull),
}

schema_as!(Package, PackageInput);

impl From<PackageInput> for Package {
    fn from(written: PackageInput) -> Package {
        match written {
            PackageInput::Name(name) => Package { name, version: None },
            PackageInput::Pinned(full) => Package {
                name: full.name,
                version: full.version,
            },
        }
    }
}

impl<'de> Deserialize<'de> for PackageInput {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<PackageInput, D::Error> {
        d.deserialize_any(PackageVisitor)
    }
}

/// A package as written: its name, or `{ name, version }`.
struct PackageVisitor;

impl<'de> Visitor<'de> for PackageVisitor {
    type Value = PackageInput;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a package's name, or `{ name, version }`")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<PackageInput, E> {
        if let Some((name, version)) = v.split_once('=') {
            return Err(E::custom(
                Mistake::new(Code::InvalidValue, format!("`{v}`: a package's version goes in `version`"))
                    .fix(format!("`{{ name: {name}, version: \"{version}\" }}`")),
            ));
        }
        v.parse().map(PackageInput::Name).map_err(E::custom)
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<PackageInput, A::Error> {
        PackageFull::deserialize(de::value::MapAccessDeserializer::new(map)).map(PackageInput::Pinned)
    }
}

/// A `repository` step: a vendor's package repository added to the machine's package manager,
/// its key pinned by the SHA-256 of its file. Its kind (apt, rpm or apk) is the OS family's.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Repository {
    /// Its name, in kebab case: it names the source and key files (`nginx`).
    pub name: RepositoryName,
    /// Its address (`https`).
    pub url: HttpsUrl,
    /// Its signing key: `{ url, sha256 }`, the SHA-256 of the key's file.
    pub key: RepositoryKey,
    /// The suite (apt only; default: the OS's codename, `bookworm`).
    #[serde(default)]
    pub suite: Option<AptWord>,
    /// The components (apt only; default `[main]`).
    #[serde(default)]
    pub components: Option<Vec<AptWord>>,
    /// Its priority over the OS's own packages: apt's pin priority, dnf's `priority`.
    #[serde(default)]
    pub priority: Option<i32>,
}

/// A repository's signing key, pinned.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RepositoryKey {
    /// Where the key is (`https`).
    pub url: HttpsUrl,
    /// The SHA-256 of the key's file (64 hex digits, as `sha256sum` prints it).
    pub sha256: Sha256,
}
