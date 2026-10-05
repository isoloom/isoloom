//! The JSON Schema of `isoloom.yml`, for editors: completion, hover documentation (from the
//! model's doc comments) and errors as you type. Served at [`SCHEMA_URL`]; a spec points to it
//! with a first-line comment (see [`MODELINE`]).

use serde_json::{Value, json};

use crate::model::{KNOWN_OS, Spec};

/// Where the schema is published.
pub const SCHEMA_URL: &str = "https://www.isoloom.com/schema/v1.json";

/// The first line that tells YAML editors (the YAML language server) which schema to use.
pub const MODELINE: &str = "# yaml-language-server: $schema=https://www.isoloom.com/schema/v1.json";

/// The schema, generated from the model and completed with what `isoloom validate` checks
/// that types alone don't say (name patterns, known OS names, address formats).
pub fn schema() -> Value {
    let mut s = serde_json::to_value(schemars::schema_for!(Spec)).expect("a schema serializes");
    // Optional fields come out as "this, or null". A spec never writes null, and editors report
    // a typo inside such a field as one vague "anyOf" error: keep only the real type.
    drop_null(&mut s);
    s["$id"] = json!(SCHEMA_URL);
    s["title"] = json!("isoloom.yml");
    s["description"] = json!("An Isoloom environment spec, version 1: https://www.isoloom.com/en/docs/spec-reference");
    let kebab = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";
    let set = |s: &mut Value, path: &[&str], key: &str, v: Value| {
        let mut node = s;
        for p in path {
            node = &mut node[*p];
        }
        if node.is_object() {
            node[key] = v;
        }
    };
    set(&mut s, &["properties", "version"], "const", json!(1));
    set(&mut s, &["properties", "name"], "pattern", json!("^[a-z0-9]+(-[a-z0-9]+)*$"));
    set(&mut s, &["properties", "networks"], "propertyNames", json!({ "pattern": kebab }));
    set(&mut s, &["properties", "machines"], "propertyNames", json!({ "pattern": kebab }));
    set(&mut s, &["properties", "inputs", "items"], "pattern", json!("^[A-Z_][A-Z0-9_]*$"));
    set(
        &mut s,
        &["$defs", "Network", "properties", "cidr"],
        "pattern",
        json!(r"^10\.\d{1,3}\.\d{1,3}\.\d{1,3}/(2[4-9])$"),
    );
    set(&mut s, &["$defs", "VmImpl", "properties", "os"], "enum", json!(KNOWN_OS));
    set(
        &mut s,
        &["$defs", "Machine", "properties", "networks"],
        "additionalProperties",
        json!({ "type": "integer", "minimum": 1, "maximum": 254 }),
    );
    set(
        &mut s,
        &["$defs", "Machine", "properties", "volumes"],
        "additionalProperties",
        json!({ "type": "string", "pattern": "^/" }),
    );
    s
}

/// Replaces `anyOf: [X, {type: null}]` with X and `type: [T, "null"]` with T, everywhere.
fn drop_null(v: &mut Value) {
    match v {
        Value::Object(map) => {
            if let Some(Value::Array(any)) = map.get("anyOf")
                && any.len() == 2
                && any.iter().any(|b| b.get("type") == Some(&json!("null")))
            {
                let keep = any.iter().find(|b| b.get("type") != Some(&json!("null"))).cloned().expect("two branches");
                map.remove("anyOf");
                if let Value::Object(k) = keep {
                    for (key, val) in k {
                        map.entry(key).or_insert(val);
                    }
                }
            }
            if let Some(Value::Array(types)) = map.get("type")
                && types.len() == 2
                && types.contains(&json!("null"))
            {
                let t = types.iter().find(|t| **t != json!("null")).cloned().expect("two types");
                map.insert("type".into(), t);
            }
            if map.get("default") == Some(&Value::Null) {
                map.remove("default");
            }
            for val in map.values_mut() {
                drop_null(val);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(drop_null),
        _ => {}
    }
}
