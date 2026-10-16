//! A setting of an entry: one key of a machine's `with:`, its type, and what it accepts (design
//! §4.4). The settings are the schema of `with:`; [`crate::Settings`] reads a machine's values
//! against them.

use std::borrow::Cow;

use indexmap::IndexMap;
use isoloom_spec::syntax::UserName;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer};

use crate::names::SettingName;

/// One setting.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Setting {
    /// What it holds.
    #[serde(rename = "type")]
    pub kind: SettingType,
    /// What it sets, for editors and the catalogue page.
    pub description: String,
    /// `with:` must give it.
    #[serde(default)]
    pub required: bool,
    /// Its value when `with:` leaves it out. A setting with neither `required` nor `default` is
    /// optional: unset, `when: { unset: <setting> }` holds.
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    /// A `string`'s rule, a regular expression the whole value matches.
    #[serde(default)]
    pub pattern: Option<Pattern>,
    /// A `string`'s allowed values.
    #[serde(default)]
    pub values: Option<Vec<String>>,
    /// An `integer`'s lowest value.
    #[serde(default)]
    pub minimum: Option<i64>,
    /// An `integer`'s highest value.
    #[serde(default)]
    pub maximum: Option<i64>,
    /// A `password`'s value file's owner (default root).
    #[serde(default)]
    pub owner: Option<UserName>,
    /// A `list`'s item: its fields, by name, each a setting of any type but `list`.
    #[serde(default)]
    pub items: Option<IndexMap<SettingName, Setting>>,
    /// A `list`'s most items.
    #[serde(default)]
    pub max: Option<u32>,
}

/// What a setting holds, and how it reaches the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum SettingType {
    /// Text: in a template, it needs a `pattern` or `values`.
    String,
    /// A whole number.
    Integer,
    /// `true` or `false`.
    Boolean,
    /// A port number, 1 to 65535: a port of `ports:` may take its number from it.
    Port,
    /// `<machine>:<port>` (the machine's address on a network it shares with this one), or
    /// `<address>:<port>`.
    Endpoint,
    /// A folder of the project (`site/`).
    Folder,
    /// A file of the project (`nginx/site.conf`).
    File,
    /// A literal (lab content), `{ secret: <name> }` or `{ input: <NAME> }`: a value file at
    /// `/run/isoloom/with/<setting>`, reached as `{{ with.<setting>.path }}`.
    Password,
    /// A list of items, each a map of `items`' fields: a step repeats for each with `for_each`.
    List,
}

impl SettingType {
    /// How an entry writes it.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            SettingType::String => "string",
            SettingType::Integer => "integer",
            SettingType::Boolean => "boolean",
            SettingType::Port => "port",
            SettingType::Endpoint => "endpoint",
            SettingType::Folder => "folder",
            SettingType::File => "file",
            SettingType::Password => "password",
            SettingType::List => "list",
        }
    }

    /// What a value must be, as a message says it (`a port number, 1 to 65535`).
    #[must_use]
    pub fn expected(self) -> &'static str {
        match self {
            SettingType::String => "text",
            SettingType::Integer => "a whole number",
            SettingType::Boolean => "`true` or `false`",
            SettingType::Port => "a port number, 1 to 65535",
            SettingType::Endpoint => "`<machine>:<port>` or `<address>:<port>`",
            SettingType::Folder => "a folder of the project (`site/`)",
            SettingType::File => "a file of the project (`conf/site.conf`)",
            SettingType::Password => "a literal, `{ secret: <name> }` or `{ input: <NAME> }`",
            SettingType::List => "a list of items",
        }
    }
}

/// A `string` setting's `pattern`: a regular expression the whole value matches.
#[derive(Debug, Clone)]
pub struct Pattern {
    written: String,
    regex: regex::Regex,
}

impl Pattern {
    /// The pattern as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.written
    }

    /// Whether the whole of `value` matches.
    #[must_use]
    pub fn matches(&self, value: &str) -> bool {
        self.regex.is_match(value)
    }
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Pattern) -> bool {
        self.written == other.written
    }
}

impl TryFrom<String> for Pattern {
    type Error = regex::Error;

    fn try_from(written: String) -> Result<Pattern, regex::Error> {
        let regex = regex::Regex::new(&format!("^(?:{written})$"))?;
        Ok(Pattern { written, regex })
    }
}

impl<'de> Deserialize<'de> for Pattern {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Pattern, D::Error> {
        Pattern::try_from(String::deserialize(d)?).map_err(|e| serde::de::Error::custom(format!("a pattern is a regular expression: {e}")))
    }
}

impl JsonSchema for Pattern {
    fn schema_name() -> Cow<'static, str> {
        "Pattern".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "format": "regex" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_is_named_as_an_entry_writes_it() {
        let all = [
            SettingType::String,
            SettingType::Integer,
            SettingType::Boolean,
            SettingType::Port,
            SettingType::Endpoint,
            SettingType::Folder,
            SettingType::File,
            SettingType::Password,
            SettingType::List,
        ];
        for t in all {
            assert_eq!(serde_yaml_ng::from_str::<SettingType>(t.id()).ok(), Some(t));
            assert_ne!(t.expected(), "");
        }
    }

    #[test]
    fn a_pattern_matches_the_whole_value() {
        let p = Pattern::try_from("[a-z]+".to_owned()).unwrap();
        assert!(p.matches("abc") && !p.matches("abc1") && !p.matches("1abc"));
        assert!(
            serde_yaml_ng::from_str::<Pattern>("'['")
                .unwrap_err()
                .to_string()
                .contains("a pattern is a regular expression")
        );
    }
}
