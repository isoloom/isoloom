//! The names and paths a step gives: units, users, groups, packages, repositories, paths in the
//! machine and in the project, URLs and checksums. Each is a newtype that only holds a value
//! matching its rule (the JSON Schema carries the same pattern), so a parsed step holds nothing a
//! renderer would have to check again. Every rule keeps out the characters no name of its kind
//! has (whitespace, quotes, `;`, `$`), and the renderers quote every value all the same.

use std::{borrow::Cow, fmt, str::FromStr};

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::super::names::{Text, name_type};
use crate::Error;

name_type!(
    /// A service's unit name (`nginx`, `php8.2-fpm`, `W3SVC`): a systemd or OpenRC unit, or a
    /// Windows service, or a key of the daemon table.
    UnitName,
    "unit name",
    "^[A-Za-z0-9][A-Za-z0-9:_.@-]{0,127}$",
    "a unit is letters, digits and `:_.@-`, starting with a letter or digit, at most 128 characters (`nginx`, `php8.2-fpm`)",
    |s| s.len() <= 128 && s.starts_with(|c: char| c.is_ascii_alphanumeric()) && s.chars().all(|c| c.is_ascii_alphanumeric() || ":_.@-".contains(c))
);

name_type!(
    /// A user's name (`www-data`, `svc_backup`, `Administrator`).
    UserName,
    "user name",
    "^[A-Za-z_][A-Za-z0-9_.-]{0,31}$",
    "a user is letters, digits and `_.-`, starting with a letter or `_`, at most 32 characters (`www-data`)",
    |s| s.len() <= 32 && s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') && s.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
);

name_type!(
    /// A group's name (`www-data`, or a Windows group such as `Remote Desktop Users`).
    GroupName,
    "group name",
    "^[A-Za-z_]([A-Za-z0-9_. -]{0,62}[A-Za-z0-9_.-])?$",
    "a group is letters, digits, `_.-` and inner spaces, starting with a letter or `_`, at most 64 characters (`www-data`, `Remote Desktop Users`)",
    |s| s.len() <= 64
        && s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && !s.ends_with(' ')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "_.- ".contains(c))
);

name_type!(
    /// A package's name, without its version (`nginx`, `libapache2-mod-php`), or a Windows
    /// feature's (`Web-Server`).
    PackageName,
    "package name",
    "^[A-Za-z0-9][A-Za-z0-9+._:-]{0,127}$",
    "a package is letters, digits and `+._:-`, starting with a letter or digit, at most 128 characters (`nginx`); its version goes in `version`",
    |s| s.len() <= 128 && s.starts_with(|c: char| c.is_ascii_alphanumeric()) && s.chars().all(|c| c.is_ascii_alphanumeric() || "+._:-".contains(c))
);

name_type!(
    /// A package repository's name, in kebab case (`nginx`, `pgdg`): it names its source and key
    /// files.
    RepositoryName,
    "repository name",
    "^[a-z][a-z0-9]*(-[a-z0-9]+)*$",
    "a repository's name is kebab-case, at most 63 characters (`nginx`, `pgdg`)",
    |s| s.len() <= 63
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && !s.ends_with('-')
        && !s.contains("--")
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
);

name_type!(
    /// An absolute path in the machine (`/etc/app.toml`), or a drive path on Windows
    /// (`C:\inetpub\wwwroot`).
    AbsPath,
    "path in the machine",
    "^(/|[A-Za-z]:[\\\\/])[^\\n]*$",
    "a path in the machine is absolute (`/etc/app.toml`), or a drive path on Windows (`C:\\app`)",
    |s| (s.starts_with('/') || drive(s)) && !s.contains(['\n', '\0'])
);

name_type!(
    /// A file or folder of the project, relative to its folder, without `..` (`provision/web.sh`;
    /// a folder ends with `/`, `site/`).
    ProjectPath,
    "project path",
    "^(?![/\\\\]|[A-Za-z]:[/\\\\])(?!(.*[/\\\\])?\\.\\.([/\\\\]|$))[^\\n]+$",
    "a project path is relative to the project folder and stays inside it: no leading `/`, no `..` (`provision/web.sh`)",
    |s| !s.is_empty() && !s.starts_with(['/', '\\']) && !drive(s) && !s.split(['/', '\\']).any(|part| part == "..") && !s.contains(['\n', '\0'])
);

name_type!(
    /// An `https` URL (`https://nginx.org/keys/nginx_signing.key`): every fetch goes over TLS.
    HttpsUrl,
    "URL",
    "^https://[^\\s/]+\\S*$",
    "a URL is `https://<host>/<path>`, with no spaces: every fetch goes over TLS",
    |s| s
        .strip_prefix("https://")
        .is_some_and(|rest| !rest.starts_with('/') && !rest.is_empty() && !rest.contains(char::is_whitespace))
);

name_type!(
    /// A SHA-256, as `sha256sum` prints it: 64 lowercase hex digits.
    Sha256,
    "SHA-256",
    "^[0-9a-f]{64}$",
    "a SHA-256 is 64 lowercase hex digits, as `sha256sum` prints it",
    |s| s.len() == 64 && s.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
);

name_type!(
    /// An apt suite or component (`bookworm`, `main`, `nginx`).
    AptWord,
    "apt suite or component",
    "^[A-Za-z0-9][A-Za-z0-9._/-]{0,63}$",
    "a suite or component is letters, digits and `._/-`, starting with a letter or digit (`bookworm`, `main`)",
    |s| s.len() <= 64 && s.starts_with(|c: char| c.is_ascii_alphanumeric()) && s.chars().all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c))
);

/// Whether `s` starts with a Windows drive (`C:\`, `C:/`).
fn drive(s: &str) -> bool {
    matches!(s.as_bytes(), [d, b':', b'\\' | b'/', ..] if d.is_ascii_alphabetic())
}

/// A package's version, as its family's package manager writes it (`1.30.6-1~bookworm`,
/// `1:2.4.62-1`, `1.30.6-r1`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageVersion(String);

impl PackageVersion {
    /// The version as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for PackageVersion {
    type Err = Error;

    /// The version, when it follows the rule.
    ///
    /// # Errors
    ///
    /// [`Error::Name`] with the rule.
    fn from_str(value: &str) -> Result<PackageVersion, Error> {
        let ok = value.len() <= 128
            && value.starts_with(|c: char| c.is_ascii_alphanumeric())
            && value.chars().all(|c| c.is_ascii_alphanumeric() || ".+~:_-".contains(c));
        if ok {
            Ok(PackageVersion(value.to_owned()))
        } else {
            Err(Error::Name {
                what: "package version",
                value: value.to_owned(),
                rule: "a version is letters, digits and `.+~:_-`, as the package manager writes it (`1.30.6-1~bookworm`)",
            })
        }
    }
}

impl fmt::Display for PackageVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for PackageVersion {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Serialize for PackageVersion {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PackageVersion {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<PackageVersion, D::Error> {
        d.deserialize_any(VersionText)?.parse().map_err(serde::de::Error::custom)
    }
}

/// A version as written: a text, or a whole number; a decimal number would lose its zeros.
struct VersionText;

impl serde::de::Visitor<'_> for VersionText {
    type Value = String;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a version, quoted (`\"1.30.6-1~bookworm\"`)")
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<String, E> {
        Ok(v.to_owned())
    }

    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<String, E> {
        Ok(v.to_string())
    }

    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<String, E> {
        Err(E::custom(format!("quote the version (`\"{v}\"`): read as a number, it loses its zeros")))
    }
}

impl JsonSchema for PackageVersion {
    fn schema_name() -> Cow<'static, str> {
        "PackageVersion".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": ["string", "integer"], "pattern": "^[A-Za-z0-9][A-Za-z0-9.+~:_-]{0,127}$" })
    }
}

#[cfg(test)]
mod tests;
