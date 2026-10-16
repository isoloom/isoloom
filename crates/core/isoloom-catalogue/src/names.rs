//! The names the catalogue gives: entries, settings, release variables and contract cases. Each
//! is a [`Name`] of its [`Kind`], which only holds a value matching the kind's rule, so a loaded
//! entry has no malformed name anywhere; the generated JSON Schemas carry the same pattern.

use std::borrow::Cow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::Error;

/// What a kind of name is: its rule, as code, as a JSON Schema pattern and as an author reads it.
pub trait Kind: sealed::Sealed {
    /// What the name names, as messages say it (`entry name`).
    const WHAT: &'static str;
    /// The rule as a JSON Schema pattern.
    const PATTERN: &'static str;
    /// The rule, as an author fixes a name that breaks it.
    const RULE: &'static str;
    /// Whether `value` follows the rule.
    fn allows(value: &str) -> bool;
}

mod sealed {
    /// Only this crate's kinds are kinds of names.
    pub trait Sealed {}
}

/// A name of kind `K`: only ever a value `K` allows.
pub struct Name<K: Kind>(String, PhantomData<K>);

/// Kebab case: lowercase letters and digits in words joined by single dashes, from a letter.
fn kebab(s: &str) -> bool {
    s.len() <= 63
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.split('-')
            .all(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

/// Snake case: lowercase letters, digits and `_`, from a letter.
fn snake(s: &str) -> bool {
    s.len() <= 63 && s.starts_with(|c: char| c.is_ascii_lowercase()) && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

macro_rules! kind {
    ($(#[$doc:meta])* $kind:ident, $alias:ident, $what:literal, $pattern:literal, $rule:literal, $check:path) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $kind;

        impl sealed::Sealed for $kind {}

        impl Kind for $kind {
            const WHAT: &'static str = $what;
            const PATTERN: &'static str = $pattern;
            const RULE: &'static str = $rule;
            fn allows(value: &str) -> bool {
                $check(value)
            }
        }

        $(#[$doc])*
        pub type $alias = Name<$kind>;
    };
}

kind!(
    /// An entry's name, its folder's (`nginx`, `php-fpm`): kebab case, as a machine's `service:`
    /// writes it.
    EntryKind,
    EntryName,
    "entry name",
    "^[a-z][a-z0-9]*(-[a-z0-9]+)*$",
    "an entry is named in kebab case, its folder's name (`nginx`, `php-fpm`)",
    kebab
);

kind!(
    /// A setting's name (`port`, `php_fpm`), or a field of a list setting's items: snake case, as
    /// `with:` writes it.
    SettingKind,
    SettingName,
    "setting name",
    "^[a-z][a-z0-9_]*$",
    "a setting is named in snake case (`port`, `php_fpm`)",
    snake
);

kind!(
    /// A release's variable (`branch`), read as `{{ vars.<name> }}`: snake case.
    VarKind,
    VarName,
    "variable name",
    "^[a-z][a-z0-9_]*$",
    "a release's variable is named in snake case (`branch`)",
    snake
);

kind!(
    /// A name of an entry's persistent data (`data`, `logs`): kebab case, as a volume is named.
    DataKind,
    DataName,
    "data name",
    "^[a-z][a-z0-9]*(-[a-z0-9]+)*$",
    "an entry's data is named in kebab case (`data`, `site-logs`)",
    kebab
);

kind!(
    /// A contract case's name (`default`, `php`): kebab case.
    CaseKind,
    CaseName,
    "case name",
    "^[a-z][a-z0-9]*(-[a-z0-9]+)*$",
    "a contract case is named in kebab case (`default`, `own-config`)",
    kebab
);

impl<K: Kind> Name<K> {
    /// The name as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<K: Kind> FromStr for Name<K> {
    type Err = Error;

    /// The name, when it follows its kind's rule.
    ///
    /// # Errors
    ///
    /// [`Error::Name`] with the rule, as an author reads it.
    fn from_str(value: &str) -> Result<Name<K>, Error> {
        if K::allows(value) {
            Ok(Name(value.to_owned(), PhantomData))
        } else {
            Err(Error::Name {
                what: K::WHAT,
                value: value.to_owned(),
                rule: K::RULE,
            })
        }
    }
}

impl<K: Kind> Clone for Name<K> {
    fn clone(&self) -> Name<K> {
        Name(self.0.clone(), PhantomData)
    }
}

impl<K: Kind> fmt::Debug for Name<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl<K: Kind> fmt::Display for Name<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<K: Kind> PartialEq for Name<K> {
    fn eq(&self, other: &Name<K>) -> bool {
        self.0 == other.0
    }
}

impl<K: Kind> Eq for Name<K> {}

impl<K: Kind> PartialOrd for Name<K> {
    fn partial_cmp(&self, other: &Name<K>) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: Kind> Ord for Name<K> {
    fn cmp(&self, other: &Name<K>) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl<K: Kind> Hash for Name<K> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<K: Kind> std::borrow::Borrow<str> for Name<K> {
    /// A name hashes, compares and orders as its text, so a map of names looks one up by text.
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl<K: Kind> AsRef<str> for Name<K> {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<K: Kind> Serialize for Name<K> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de, K: Kind> Deserialize<'de> for Name<K> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Name<K>, D::Error> {
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
    }
}

impl<K: Kind> JsonSchema for Name<K> {
    fn schema_name() -> Cow<'static, str> {
        K::WHAT.replace(' ', "-").into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "pattern": K::PATTERN })
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn each_kind_holds_only_names_following_its_rule() {
        assert!("php-fpm".parse::<EntryName>().is_ok() && "PHP".parse::<EntryName>().is_err() && "a--b".parse::<EntryName>().is_err());
        assert!("php_fpm".parse::<SettingName>().is_ok() && "php-fpm".parse::<SettingName>().is_err());
        assert!("branch".parse::<VarName>().is_ok() && "_x".parse::<VarName>().is_err());
        let wrong = "Own Config".parse::<CaseName>().unwrap_err().to_string();
        assert_eq!(
            wrong,
            "`Own Config` isn't a valid case name: a contract case is named in kebab case (`default`, `own-config`)"
        );
    }

    #[test]
    fn a_name_is_its_text() {
        let name: EntryName = "php-fpm".parse().unwrap();
        assert_eq!((name.as_ref(), name.as_str(), name.to_string().as_str()), ("php-fpm", "php-fpm", "php-fpm"));
    }

    #[test]
    fn a_name_reads_from_yaml_with_its_rule() {
        let wrong = serde_yaml_ng::from_str::<EntryName>("Nginx").unwrap_err().to_string();
        assert!(wrong.contains("an entry is named in kebab case"), "{wrong}");
    }

    proptest! {
        /// The code rule and the schema's pattern agree on every name.
        #[test]
        fn the_rule_and_the_pattern_agree(s in "[a-zA-Z0-9_-]{0,12}") {
            let entry = regex::Regex::new(EntryKind::PATTERN).unwrap();
            prop_assert_eq!(EntryKind::allows(&s), entry.is_match(&s));
            let setting = regex::Regex::new(SettingKind::PATTERN).unwrap();
            prop_assert_eq!(SettingKind::allows(&s), setting.is_match(&s));
        }
    }
}
