//! The spec's `inputs`: values the launcher, CI or the command line provides at each start.
//! `inputs: [FLAG]` is short for `inputs: { FLAG: {} }`. An input is sensitive unless it says
//! otherwise, and a sensitive one can only reach a machine as a file (or a catalogue setting).

use std::borrow::Cow;
use std::fmt;

use indexmap::IndexMap;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use super::names::InputName;

/// One input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Input {
    /// Whether the value must stay hidden (default `true`): a sensitive input may be a file's
    /// `input` or a `with:` value, never an `environment` value, which `docker inspect` shows.
    #[serde(default = "yes")]
    pub sensitive: bool,
    /// What the value is, shown when a runner asks for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Default for Input {
    fn default() -> Input {
        Input {
            sensitive: true,
            description: None,
        }
    }
}

fn yes() -> bool {
    true
}

/// The spec's inputs, by name, in the order written.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(transparent)]
pub struct Inputs(pub IndexMap<InputName, Input>);

impl Inputs {
    /// The input named `name`, when declared.
    pub fn get(&self, name: &InputName) -> Option<&Input> {
        self.0.get(name)
    }
}

impl<'de> Deserialize<'de> for Inputs {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Inputs, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Inputs;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a list of input names, or a map of name to `{ sensitive, description }`")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Inputs, A::Error> {
                let mut out = IndexMap::new();
                while let Some(name) = seq.next_element::<InputName>()? {
                    if out.insert(name.clone(), Input::default()).is_some() {
                        return Err(de::Error::custom(format!("`{name}` is listed twice")));
                    }
                }
                Ok(Inputs(out))
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Inputs, A::Error> {
                let raw = IndexMap::<InputName, Option<Input>>::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(Inputs(raw.into_iter().map(|(k, v)| (k, v.unwrap_or_default())).collect()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Inputs, E> {
                Ok(Inputs::default())
            }
        }
        d.deserialize_any(V)
    }
}

/// `inputs:` as written, for the schema.
#[derive(JsonSchema)]
#[serde(untagged)]
#[expect(dead_code, reason = "the schema's shape only: `Inputs` reads the values.")]
enum InputsInput {
    /// Input names, each sensitive.
    Names(Vec<InputName>),
    /// Inputs by name, each with whether it is sensitive and what it is.
    Full(IndexMap<InputName, Input>),
}

impl JsonSchema for Inputs {
    fn schema_name() -> Cow<'static, str> {
        "Inputs".into()
    }

    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        InputsInput::json_schema(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_are_a_list_or_a_map() {
        let list: Inputs = serde_yaml_ng::from_str("[FLAG, API_URL]").unwrap();
        assert!(list.0.values().all(|i| i.sensitive));
        let map: Inputs = serde_yaml_ng::from_str("{ FLAG: , API_URL: { sensitive: false } }").unwrap();
        let flags: Vec<bool> = map.0.values().map(|i| i.sensitive).collect();
        assert_eq!(flags, [true, false]);
        assert!(serde_yaml_ng::from_str::<Inputs>("[FLAG, FLAG]").unwrap_err().to_string().contains("twice"));
        assert!(serde_yaml_ng::from_str::<Inputs>("[1FLAG]").is_err());
    }
}
