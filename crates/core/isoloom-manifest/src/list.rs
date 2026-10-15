//! Lists that can't be empty, and the steps a value is delivered to.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ManifestError;
use crate::names::StepId;

/// A list with at least one item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NonEmpty<T>(Vec<T>);

impl<T> NonEmpty<T> {
    /// The list, when it has an item (`what` names it in the error).
    pub fn new(items: Vec<T>, what: &'static str) -> Result<NonEmpty<T>, ManifestError> {
        if items.is_empty() {
            Err(ManifestError::Empty { what })
        } else {
            Ok(NonEmpty(items))
        }
    }

    /// A list of one.
    pub fn one(item: T) -> NonEmpty<T> {
        NonEmpty(vec![item])
    }

    /// The items, in order.
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// The items, in order.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }
}

impl<'a, T> IntoIterator for &'a NonEmpty<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<T: Serialize> Serialize for NonEmpty<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for NonEmpty<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<NonEmpty<T>, D::Error> {
        NonEmpty::new(Vec::deserialize(d)?, "the list").map_err(de::Error::custom)
    }
}

impl<T: JsonSchema> JsonSchema for NonEmpty<T> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        format!("NonEmpty_{}", T::schema_name()).into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "array", "minItems": 1, "items": generator.subschema_for::<T>() })
    }
}

/// The steps whose process gets a value: every step of every operation (`["*"]`), or these.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StepSelection {
    /// Every step of every operation.
    Every,
    /// These steps, by id.
    Only(NonEmpty<StepId>),
}

impl Serialize for StepSelection {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            StepSelection::Every => ["*"].serialize(s),
            StepSelection::Only(ids) => ids.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for StepSelection {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<StepSelection, D::Error> {
        d.deserialize_seq(SelectionVisitor)
    }
}

struct SelectionVisitor;

impl<'de> Visitor<'de> for SelectionVisitor {
    type Value = StepSelection;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list of step ids, or `[\"*\"]` for every step")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<StepSelection, A::Error> {
        let mut every = false;
        let mut ids = Vec::new();
        while let Some(item) = seq.next_element::<String>()? {
            if item == "*" {
                every = true;
            } else {
                ids.push(StepId::parse(&item).map_err(de::Error::custom)?);
            }
        }
        match (every, ids.is_empty()) {
            (true, true) => Ok(StepSelection::Every),
            (true, false) => Err(de::Error::custom(ManifestError::MixedWildcard)),
            (false, _) => NonEmpty::new(ids, "the steps").map(StepSelection::Only).map_err(de::Error::custom),
        }
    }
}

impl JsonSchema for StepSelection {
    fn schema_name() -> Cow<'static, str> {
        "StepSelection".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "The ids of the steps whose process gets the value; `[\"*\"]` for every step of every operation.",
            "oneOf": [
                { "type": "array", "const": ["*"] },
                { "type": "array", "minItems": 1, "items": generator.subschema_for::<StepId>() },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_is_every_step_or_some() {
        assert_eq!(serde_json::from_str::<StepSelection>(r#"["*"]"#).unwrap(), StepSelection::Every);
        let some: StepSelection = serde_json::from_str(r#"["up", "job-web-init-1"]"#).unwrap();
        assert_eq!(serde_json::to_string(&some).unwrap(), r#"["up","job-web-init-1"]"#);
        assert!(
            serde_json::from_str::<StepSelection>(r#"["*", "up"]"#)
                .unwrap_err()
                .to_string()
                .contains("alone")
        );
        assert!(serde_json::from_str::<StepSelection>("[]").unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn an_empty_list_is_refused() {
        assert!(serde_json::from_str::<NonEmpty<u8>>("[]").is_err());
        assert_eq!(serde_json::from_str::<NonEmpty<u8>>("[1]").unwrap().as_slice(), [1]);
    }
}
