//! A machine's `os`: `<family>@<version>`, or the family alone for its pinned default. Parsed once
//! into the OS it names, so a parsed spec only holds known ones.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::os::{Os, resolve};

/// A known OS, as a machine's `os` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OsRef(Os);

impl OsRef {
    /// The OS `text` names (`debian@12`, `debian`), else why it doesn't name one.
    pub fn parse(text: &str) -> Result<OsRef, String> {
        resolve(text).map(OsRef).map_err(|e| e.to_string())
    }

    /// The OS, resolved.
    pub fn os(self) -> Os {
        self.0
    }

    /// Its image-table name (`debian-12`), what the targets read.
    pub fn name(self) -> &'static str {
        self.0.name
    }

    /// Whether it is a Windows OS (no container form in 1.0).
    pub fn is_windows(self) -> bool {
        crate::images::is_windows(self.0.name)
    }
}

impl fmt::Display for OsRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.id())
    }
}

impl Serialize for OsRef {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.id())
    }
}

impl<'de> Deserialize<'de> for OsRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<OsRef, D::Error> {
        OsRef::parse(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for OsRef {
    fn schema_name() -> Cow<'static, str> {
        "Os".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "An OS as `<family>@<version>` (`debian@12`, `windows-server@2022`), or the family alone for its pinned default version (`debian` is `debian@12`).",
            "type": "string",
            "enum": crate::os::spellings(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_os_resolves_or_lists_the_known_ones() {
        let os = OsRef::parse("debian").unwrap();
        assert_eq!((os.to_string(), os.name()), ("debian@12".to_string(), "debian-12"));
        assert!(OsRef::parse("windows-server@2019").unwrap().is_windows());
        assert!(OsRef::parse("debian@10").unwrap_err().contains("11, 12, 13"));
    }
}
