//! The tags as a spec or the defaults write them: each value text, or a number or `true`/`false`
//! read as its text (`cost-center: 1234` is the tag `cost-center=1234`), since a tag is text on
//! every cloud. A number is its YAML value written back, so a value whose form matters (`007`)
//! is quoted.

use std::fmt;

use indexmap::IndexMap;
use schemars::{Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserialize, Deserializer, Visitor};

/// The tags, each value as text.
///
/// # Errors
///
/// A value that is a list or a mapping.
pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<IndexMap<String, String>, D::Error> {
    Ok(text(IndexMap::<String, TagValue>::deserialize(d)?))
}

/// The tags as read, each value as text.
pub(crate) fn text(tags: IndexMap<String, TagValue>) -> IndexMap<String, String> {
    tags.into_iter().map(|(k, TagValue(v))| (k, v)).collect()
}

/// The schema of the tags: values text, numbers or booleans.
pub(crate) fn schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({ "type": "object", "additionalProperties": { "type": ["string", "number", "boolean"] } })
}

/// One tag's value, as text.
pub(crate) struct TagValue(String);

impl<'de> Deserialize<'de> for TagValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<TagValue, D::Error> {
        d.deserialize_any(TagValueVisitor)
    }
}

/// Reads a scalar as its text.
struct TagValueVisitor;

impl Visitor<'_> for TagValueVisitor {
    type Value = TagValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a tag's value: text, a number or true/false")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<TagValue, E> {
        Ok(TagValue(v.to_owned()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<TagValue, E> {
        Ok(TagValue(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<TagValue, E> {
        Ok(TagValue(v.to_string()))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<TagValue, E> {
        Ok(TagValue(v.to_string()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<TagValue, E> {
        Ok(TagValue(v.to_string()))
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<TagValue, E> {
        Ok(TagValue(v.to_string()))
    }
}
