//! The steps that make accounts: `user` and `group`. A user's `password` in a step is a literal,
//! lab content that lands in generated files: steps have no secrets.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::super::mistake::Mistake;
use super::super::problem::Code;
use super::words::{AbsPath, GroupName, UserName};

/// A `user` step: a local user, created or brought in line, never deleted.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UserStep {
    /// Its name.
    pub name: UserName,
    /// Its user id (default: the next free one).
    #[serde(default)]
    pub uid: Option<u32>,
    /// Its primary group (default: one of its own name, or the system's default).
    #[serde(default)]
    pub group: Option<GroupName>,
    /// The other groups it is a member of (`[sudo]`, `[Remote Desktop Users]`).
    #[serde(default)]
    pub groups: Vec<GroupName>,
    /// Its home folder (absolute; default the system's).
    #[serde(default)]
    pub home: Option<AbsPath>,
    /// Its login shell (absolute; `/usr/sbin/nologin` for a service account).
    #[serde(default)]
    pub shell: Option<AbsPath>,
    /// A system account (a low id, no ageing): a service's.
    #[serde(default)]
    pub system: bool,
    /// Its password, a literal: lab content, written in the generated files (hashed on Linux).
    /// A secret or an input is refused here: a catalogue entry's `users` setting takes those.
    #[serde(default)]
    pub password: Option<Password>,
}

/// A `group` step: a local group, created or brought in line, never deleted.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupStep {
    /// Its name.
    pub name: GroupName,
    /// Its group id (default: the next free one).
    #[serde(default)]
    pub gid: Option<u32>,
    /// A system group (a low id).
    #[serde(default)]
    pub system: bool,
}

/// A user's password in a step: a literal, which is lab content (it lands in generated files).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Password(String);

impl Password {
    /// The password as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Password {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Password, D::Error> {
        d.deserialize_any(PasswordVisitor)
    }
}

/// A password as written: a text or a number, or a value source, which steps refuse.
struct PasswordVisitor;

impl<'de> Visitor<'de> for PasswordVisitor {
    type Value = Password;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a password, as text")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Password, E> {
        if v.is_empty() {
            return Err(E::custom("an empty password: leave `password` out for an account without one"));
        }
        Ok(Password(v.to_owned()))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Password, E> {
        Ok(Password(v.to_string()))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Password, E> {
        Ok(Password(v.to_string()))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Password, A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Err(de::Error::custom(
            Mistake::new(
                Code::SensitiveValue,
                "steps have no secrets: a step's password is lab content, written in the generated files",
            )
            .fix("a literal (lab content), or a catalogue entry's `users` setting for a secret or an input"),
        ))
    }
}

impl JsonSchema for Password {
    fn schema_name() -> Cow<'static, str> {
        "Password".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": ["string", "integer"], "minLength": 1 })
    }
}
