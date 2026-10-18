//! The generated JSON Schemas: of `entry.yml` and `contract.yml` (`catalogue/entry.schema.json`
//! and `contract.schema.json`, for an entry's author), of each entry's `with:`, and the spec's
//! own schema with, for each entry, the rule that a machine naming it follows its settings
//! (`schemas/spec-v1.schema.json`), so an editor completes and checks `with:` as it is typed.

use indexmap::IndexMap;
use serde_json::{Map, Value, json};

use crate::catalogue::{Catalogue, Entry};
use crate::contract::Contract;
use crate::entry::{Definition, Setting, SettingType};
use crate::names::SettingName;

/// The schema of `entry.yml`.
#[must_use]
pub fn entry() -> Value {
    titled(
        Value::from(schemars::schema_for!(Definition)),
        "entry.yml",
        "An Isoloom catalogue entry: a service, its releases, the settings of `with:`, its ports, daemons and recipe (written by hand).",
    )
}

/// The schema of `contract.yml`.
#[must_use]
pub fn contract() -> Value {
    titled(
        Value::from(schemars::schema_for!(Contract)),
        "contract.yml",
        "An Isoloom catalogue entry's parity contract: the cases, the checks both forms pass, and the facts both forms share.",
    )
}

/// `s` with its title and description.
fn titled(mut s: Value, title: &str, description: &str) -> Value {
    if let Some(map) = s.as_object_mut() {
        map.insert("title".to_owned(), json!(title));
        map.insert("description".to_owned(), json!(description));
    }
    s
}

/// The schema of a machine's `with:` for `entry`: its settings, their types, defaults and
/// descriptions, and nothing else.
#[must_use]
pub fn with(entry: &Entry) -> Value {
    let def = entry.definition();
    settings(&def.settings, &format!("{}'s settings.", def.name))
}

/// An object of `settings`.
fn settings(settings: &IndexMap<SettingName, Setting>, description: &str) -> Value {
    let properties: Map<String, Value> = settings.iter().map(|(k, s)| (k.to_string(), setting(s))).collect();
    let required: Vec<&str> = settings.iter().filter(|(_, s)| s.required).map(|(k, _)| k.as_str()).collect();
    json!({ "type": "object", "description": description, "properties": properties, "required": required, "additionalProperties": false })
}

/// One setting's schema.
fn setting(s: &Setting) -> Value {
    let mut out = Map::new();
    let mut put = |key: &str, v: Value| {
        out.insert(key.to_owned(), v);
    };
    match s.kind {
        SettingType::String => {
            put("type", json!("string"));
            if let Some(values) = &s.values {
                put("enum", json!(values));
            }
            if let Some(p) = &s.pattern {
                put("pattern", json!(format!("^(?:{})$", p.as_str())));
            }
        }
        SettingType::Integer => {
            put("type", json!("integer"));
            s.minimum.into_iter().for_each(|lo| put("minimum", json!(lo)));
            s.maximum.into_iter().for_each(|hi| put("maximum", json!(hi)));
        }
        SettingType::Boolean => put("type", json!("boolean")),
        SettingType::Port => {
            put("type", json!("integer"));
            put("minimum", json!(1));
            put("maximum", json!(65535));
        }
        SettingType::Endpoint => {
            put("type", json!("string"));
            put("pattern", json!("^\\S+:[0-9]{1,5}$"));
        }
        SettingType::Folder | SettingType::File => {
            put("type", json!("string"));
            put("pattern", json!("^[^/\\\\]"));
        }
        SettingType::Password => put("anyOf", password()),
        SettingType::List => {
            put("type", json!("array"));
            put("items", s.items.as_ref().map_or_else(|| json!({}), |i| settings(i, "An item.")));
            s.max.into_iter().for_each(|max| put("maxItems", json!(max)));
        }
    }
    put("description", json!(s.description));
    if let Some(d) = &s.default {
        put("default", d.clone());
    }
    Value::Object(out)
}

/// A password: a literal, `{ secret }` or `{ input }`.
fn password() -> Value {
    json!([
        { "type": "string" },
        { "type": "object", "properties": { "secret": { "type": "string" } }, "required": ["secret"], "additionalProperties": false },
        { "type": "object", "properties": { "input": { "type": "string" } }, "required": ["input"], "additionalProperties": false }
    ])
}

/// The spec's schema, with each entry's rule: a machine whose `service` names the entry (with
/// a version or without) has a `with:` of its settings.
#[must_use]
pub fn spec(catalogue: &Catalogue) -> Value {
    let mut s = isoloom_spec::schema::schema();
    let rules: Vec<Value> = catalogue
        .entries()
        .map(|e| {
            json!({
                "if": { "properties": { "service": { "type": "string", "pattern": format!("^{}(@.+)?$", e.name()) } }, "required": ["service"] },
                "then": { "properties": { "with": with(e) } }
            })
        })
        .collect();
    if let Some(machine) = s.pointer_mut("/$defs/Machine").and_then(Value::as_object_mut)
        && !rules.is_empty()
    {
        machine.insert("allOf".to_owned(), Value::Array(rules));
    }
    s
}
