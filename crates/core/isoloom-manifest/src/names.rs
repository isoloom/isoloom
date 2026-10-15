//! The names, ids and versions of a manifest: each a newtype that only holds a value matching its
//! pattern, so a manifest that parsed has no malformed name anywhere. The JSON Schema carries the
//! same pattern.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ManifestError;

/// A newtype over `String` that holds only values `$check` accepts; `$pattern` is the same rule
/// as a JSON Schema pattern, `$rule` as a reader would fix the value.
macro_rules! pattern_type {
    ($(#[$doc:meta])* $ty:ident, $what:literal, $pattern:literal, $rule:literal, $check:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $ty(String);

        impl $ty {
            /// The value, when it matches the pattern.
            pub fn parse(value: &str) -> Result<$ty, ManifestError> {
                let check: fn(&str) -> bool = $check;
                if check(value) {
                    Ok($ty(value.to_string()))
                } else {
                    Err(ManifestError::Pattern { what: $what, value: value.to_string(), rule: $rule })
                }
            }

            /// The value as written.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl Serialize for $ty {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<$ty, D::Error> {
                let raw = String::deserialize(d)?;
                $ty::parse(&raw).map_err(serde::de::Error::custom)
            }
        }

        impl JsonSchema for $ty {
            fn schema_name() -> Cow<'static, str> {
                stringify!($ty).into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({ "type": "string", "pattern": $pattern, "description": $rule })
            }
        }
    };
}

pattern_type!(
    /// A lab's name: kebab-case (`flag-portal`, `shop-2`), as the spec's `name:`.
    LabName,
    "lab name",
    "^[a-z0-9]+(-[a-z0-9]+)*$",
    "lowercase letters and digits in words joined by single dashes (`flag-portal`)",
    |v| v.split('-').all(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
);

pattern_type!(
    /// The name of a machine, a network, a volume, a port or a check runner.
    Name,
    "name",
    "^[A-Za-z0-9][A-Za-z0-9_.-]*$",
    "a letter or digit, then letters, digits, `_`, `.` or `-` (`web`, `isoloom-check-db`)",
    |v| first_then(v, |c| c.is_ascii_alphanumeric(), |c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
);

pattern_type!(
    /// An environment variable's name, and a param's.
    EnvName,
    "environment variable name",
    "^[A-Za-z_][A-Za-z0-9_]*$",
    "a letter or `_`, then letters, digits or `_` (`CTF_FLAG_MAIN`)",
    |v| first_then(v, |c| c.is_ascii_alphabetic() || c == '_', |c| c.is_ascii_alphanumeric() || c == '_')
);

pattern_type!(
    /// A slot's name (an input as the spec names it, `CTF_FLAG_MAIN`; a secret, `db-password`),
    /// and a Terraform output's.
    Identifier,
    "identifier",
    "^[A-Za-z_][A-Za-z0-9_-]*$",
    "a letter or `_`, then letters, digits, `_` or `-` (`CTF_FLAG_MAIN`, `db-password`)",
    |v| first_then(v, |c| c.is_ascii_alphabetic() || c == '_', |c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
);

pattern_type!(
    /// A step's id, unique within its list (`up`, `job-web-init-1`).
    StepId,
    "step id",
    "^[a-z][a-z0-9-]*$",
    "a lowercase letter, then lowercase letters, digits or `-` (`pin-ports`)",
    |v| first_then(v, |c| c.is_ascii_lowercase(), |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
);

pattern_type!(
    /// A snake_case name: an output's, a Terraform variable's, an action argument's.
    SnakeName,
    "snake_case name",
    "^[a-z][a-z0-9_]*$",
    "a lowercase letter, then lowercase letters, digits or `_` (`web_portal`)",
    |v| first_then(v, |c| c.is_ascii_lowercase(), |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
);

pattern_type!(
    /// A native tool's or a Vagrant plugin's name (`docker-compose`, `vagrant-libvirt`).
    ToolName,
    "tool name",
    "^[a-z][a-z0-9_.-]*$",
    "a lowercase letter, then lowercase letters, digits, `_`, `.` or `-` (`docker-compose`)",
    |v| first_then(v, |c| c.is_ascii_lowercase(), |c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-'))
);

pattern_type!(
    /// An operating system's id (`debian-12`, `windows-server-2019`, `alpine-3.19`).
    OsName,
    "operating system id",
    "^[a-z0-9][a-z0-9.-]*$",
    "a lowercase letter or digit, then lowercase letters, digits, `.` or `-` (`debian-12`)",
    |v| first_then(v, |c| c.is_ascii_lowercase() || c.is_ascii_digit(), |c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-'))
);

pattern_type!(
    /// A version: `major.minor.patch`, with an optional pre-release (`0.10.1`, `2.17.0`).
    Version,
    "version",
    "^[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.-]+)?$",
    "three numbers joined by dots, optionally `-` and a pre-release (`2.17.0`, `1.0.0-rc.1`)",
    is_version
);

pattern_type!(
    /// A container image pinned by its digest (`hashicorp/terraform:1.9.8@sha256:…`).
    PinnedImage,
    "pinned image",
    "^[^@\\s]+@sha256:[0-9a-f]{64}$",
    "an image reference, then `@sha256:` and the 64 lowercase hex digits of its digest",
    is_pinned
);

/// Whether `v` is a first character `first` accepts followed by characters `rest` accepts.
fn first_then(v: &str, first: fn(char) -> bool, rest: fn(char) -> bool) -> bool {
    let mut chars = v.chars();
    chars.next().is_some_and(first) && chars.all(rest)
}

fn is_version(v: &str) -> bool {
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (v, None),
    };
    let numbers: Vec<&str> = core.split('.').collect();
    numbers.len() == 3
        && numbers.iter().all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        && pre.is_none_or(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-')))
}

fn is_pinned(v: &str) -> bool {
    v.split_once("@sha256:").is_some_and(|(image, digest)| {
        !image.is_empty()
            && !image.chars().any(|c| c == '@' || c.is_whitespace())
            && digest.len() == 64
            && digest.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_their_patterns() {
        assert!(LabName::parse("flag-portal").is_ok());
        assert!(LabName::parse("Flag").is_err());
        assert!(LabName::parse("a--b").is_err());
        assert!(Name::parse("isoloom-check-db").is_ok());
        assert!(Name::parse("-web").is_err());
        assert!(EnvName::parse("CTF_FLAG_MAIN").is_ok());
        assert!(EnvName::parse("ctf_flag").is_ok());
        assert!(EnvName::parse("1FLAG").is_err());
        assert!(Identifier::parse("db-password").is_ok());
        assert!(StepId::parse("job-web-init-1").is_ok());
        assert!(StepId::parse("Up").is_err());
        assert!(SnakeName::parse("web_portal").is_ok());
        assert!(SnakeName::parse("web-portal").is_err());
        assert!(OsName::parse("alpine-3.19").is_ok());
    }

    #[test]
    fn versions_are_three_numbers_and_a_pre_release() {
        for ok in ["0.10.1", "2.17.0", "1.0.0-rc.1"] {
            assert!(Version::parse(ok).is_ok(), "{ok}");
        }
        for bad in ["1.0", "1.0.0-", "v1.0.0", "1..0"] {
            assert!(Version::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_pinned_image_names_its_digest() {
        let digest = "a".repeat(64);
        assert!(PinnedImage::parse(&format!("hashicorp/terraform:1.9.8@sha256:{digest}")).is_ok());
        assert!(PinnedImage::parse("hashicorp/terraform:1.9.8").is_err());
        assert!(PinnedImage::parse(&format!("@sha256:{digest}")).is_err());
    }

    #[test]
    fn a_refusal_names_the_value_and_the_rule() {
        let err = StepId::parse("Up").unwrap_err();
        assert_eq!(err.code(), "pattern");
        assert!(err.to_string().starts_with("`Up` isn't a valid step id: a lowercase letter"), "{err}");
    }
}
