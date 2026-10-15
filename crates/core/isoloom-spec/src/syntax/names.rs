//! The names a spec gives: ports, inputs, catalogue services. Each is a newtype that only holds a
//! value matching its rule, so a parsed spec has no malformed name anywhere; the JSON Schema
//! carries the same pattern.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A newtype over `String` holding only values `$check` accepts; `$pattern` is the same rule as
/// a JSON Schema pattern, `$rule` how an author fixes a value that breaks it.
macro_rules! name_type {
    ($(#[$doc:meta])* $ty:ident, $pattern:literal, $rule:literal, $check:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $ty(String);

        impl $ty {
            /// The name, when it follows the rule (else the rule, as an author reads it).
            pub fn parse(value: &str) -> Result<$ty, String> {
                let check: fn(&str) -> bool = $check;
                if check(value) {
                    Ok($ty(value.to_string()))
                } else {
                    Err(format!("`{value}` isn't a valid name: {}", $rule))
                }
            }

            /// The name as written.
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
                $ty::parse(&d.deserialize_any(Text)?).map_err(serde::de::Error::custom)
            }
        }

        impl JsonSchema for $ty {
            fn schema_name() -> Cow<'static, str> {
                stringify!($ty).into()
            }

            fn inline_schema() -> bool {
                true
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({ "type": "string", "pattern": $pattern })
            }
        }
    };
}

name_type!(
    /// A port's name (`site`, `dns-udp`): how messages, checks and outputs refer to it
    /// (`machines.web.ports.site.published`). Lowercase letters, digits and dashes, from a letter,
    /// at most 31 characters.
    PortName,
    "^[a-z][a-z0-9-]{0,30}$",
    "a port's name is lowercase letters, digits and dashes, starting with a letter, at most 31 characters (`web`, `dns-udp`)",
    |s| {
        let mut chars = s.chars();
        chars.next().is_some_and(|c| c.is_ascii_lowercase())
            && s.len() <= 31
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }
);

name_type!(
    /// An input's name: an environment variable's (letters, digits and `_`, not starting with a
    /// digit), never starting with `ISOLOOM_` (Isoloom's own variables).
    InputName,
    "^(?!ISOLOOM_)[A-Za-z_][A-Za-z0-9_]*$",
    "an input is named like an environment variable (letters, digits and `_`, not starting with a digit) and doesn't start with `ISOLOOM_`",
    |s| is_variable(s) && !s.starts_with("ISOLOOM_")
);

name_type!(
    /// A catalogue service, with its version or without (`postgres@16`, `nginx`).
    ServiceRef,
    "^[a-z][a-z0-9-]*(@[A-Za-z0-9.+-]+)?$",
    "a service is `<name>` or `<name>@<version>` (`postgres@16`)",
    |s| {
        let (name, version) = s.split_once('@').map_or((s, None), |(n, v)| (n, Some(v)));
        name.starts_with(|c: char| c.is_ascii_lowercase())
            && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && version.is_none_or(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-')))
    }
);

impl ServiceRef {
    /// The service's name, without its version.
    pub fn name(&self) -> &str {
        self.0.split_once('@').map_or(self.0.as_str(), |(n, _)| n)
    }
}

/// A name as written: a text, or a number read as its digits (so `80:` as a port's name is
/// reported against the naming rule, not as a type error).
struct Text;

impl Visitor<'_> for Text {
    type Value = String;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a name")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
        Ok(v.to_string())
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<String, E> {
        Ok(v.to_string())
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<String, E> {
        Ok(v.to_string())
    }
}

/// Whether `s` is an environment variable's name: letters, digits and `_`, not starting with a
/// digit.
pub fn is_variable(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The format version, `1`: the only one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Version;

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u32(1)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Version, D::Error> {
        match u32::deserialize(d)? {
            1 => Ok(Version),
            n => Err(serde::de::Error::custom(format!(
                "version {n} isn't one this isoloom reads; write `version: 1`"
            ))),
        }
    }
}

impl JsonSchema for Version {
    fn schema_name() -> Cow<'static, str> {
        "Version".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "description": "The version of the `isoloom.yml` format the file is written in: `1`, the only one.", "const": 1 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_their_rules() {
        assert!(PortName::parse("dns-udp").is_ok() && PortName::parse("80").is_err() && PortName::parse("Web").is_err());
        assert!(PortName::parse(&"a".repeat(31)).is_ok() && PortName::parse(&"a".repeat(32)).is_err());
        assert!(InputName::parse("CTF_FLAG_MAIN").is_ok() && InputName::parse("1FLAG").is_err());
        assert!(InputName::parse("ISOLOOM_X").unwrap_err().contains("ISOLOOM_"));
        assert_eq!(ServiceRef::parse("postgres@16").unwrap().name(), "postgres");
        assert!(ServiceRef::parse("postgres@").is_err() && ServiceRef::parse("Nginx").is_err());
    }

    #[test]
    fn only_version_one_reads() {
        assert!(serde_yaml_ng::from_str::<Version>("1").is_ok());
        assert!(serde_yaml_ng::from_str::<Version>("2").unwrap_err().to_string().contains("version: 1"));
    }
}
