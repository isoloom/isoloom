//! A machine's `os`: `<family>@<version>`, or the family alone for its pinned default. Parsed once
//! into the OS it names, so a parsed spec only holds known ones.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use std::str::FromStr;

use crate::os::{Os, OsError, resolve};

/// A known OS, as a machine's `os` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OsRef(Os);

impl OsRef {
    /// The OS, resolved.
    #[must_use]
    pub fn os(self) -> Os {
        self.0
    }

    /// Its image-table name (`debian-12`), what the targets read.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.0.name
    }

    /// Whether it is a Windows OS (no container form in 1.0).
    #[must_use]
    pub fn is_windows(self) -> bool {
        crate::images::is_windows(self.0.name)
    }
}

impl FromStr for OsRef {
    type Err = OsError;

    /// The OS `text` names: `<family>@<version>`, or the family alone for its pinned default.
    ///
    /// # Errors
    ///
    /// [`OsError`], listing the known families or the family's versions.
    fn from_str(text: &str) -> Result<OsRef, OsError> {
        resolve(text).map(OsRef)
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
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
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
    fn a_family_alone_resolves_to_its_pinned_default() {
        let os: OsRef = "debian".parse().unwrap();
        assert_eq!((os.to_string(), os.name()), ("debian@12".to_owned(), "debian-12"));
        assert!("windows-server@2019".parse::<OsRef>().unwrap().is_windows());
    }

    #[test]
    fn an_unknown_version_lists_the_versions_of_its_family() {
        assert!("debian@10".parse::<OsRef>().unwrap_err().to_string().contains("11, 12, 13"));
    }

    proptest::proptest! {
        #[test]
        fn every_spelling_reads_back_as_itself(i in 0..crate::os::spellings().len()) {
            let spelling = &crate::os::spellings()[i];
            let os: OsRef = spelling.parse().unwrap();
            // A family alone reads as its pinned default, written in full.
            let in_full = format!("{spelling}@");
            proptest::prop_assert!(os.to_string() == *spelling || os.to_string().starts_with(&in_full));
            proptest::prop_assert_eq!(os.to_string().parse::<OsRef>().unwrap(), os);
        }

        #[test]
        fn any_text_resolves_or_says_why(text in "\\PC{0,24}") {
            if let Err(e) = text.parse::<OsRef>() {
                proptest::prop_assert!(!e.to_string().is_empty());
            }
        }
    }
}
