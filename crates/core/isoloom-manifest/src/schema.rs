//! The manifest's JSON Schema, generated from the types (committed at
//! `schemas/run-manifest-v0.schema.json`; a test keeps the file current).

use serde_json::{Map, Value, json};

use crate::Manifest;

/// Where the schema is published.
pub const SCHEMA_URL: &str = "https://www.isoloom.com/schema/run-manifest/v0.json";

/// The schema of a [`Manifest`], with its URL and description.
#[must_use]
pub fn schema() -> Value {
    let generated = schemars::schema_for!(Manifest).to_value();
    let mut out = Map::new();
    if let Some(dialect) = generated.get("$schema") {
        out.insert("$schema".into(), dialect.clone());
    }
    out.insert("$id".into(), json!(SCHEMA_URL));
    out.insert(
        "description".into(),
        json!(
            "What Isoloom writes next to one target's generated files (`.isoloom[-<n>]/<target>[/<cloud>]/run.json`) and the Isothrum runner executes. Declarative and deterministic: no secret values, no absolute paths, no timestamps, no host shell. Version 0 is unstable: Isoloom and the runner move together until 1.0."
        ),
    );
    if let Value::Object(rest) = generated {
        for (key, value) in rest {
            if key != "$schema" && key != "description" {
                out.insert(key, value);
            }
        }
    }
    let mut out = Value::Object(out);
    unwrap_descriptions(&mut out);
    out
}

/// Doc comments wrap at 100 columns: a description reads as one line per paragraph.
fn unwrap_descriptions(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                match value {
                    Value::String(text) if key == "description" => *text = text.split("\n\n").map(|p| p.replace('\n', " ")).collect::<Vec<_>>().join("\n\n"),
                    _ => unwrap_descriptions(value),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(unwrap_descriptions),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_description_reads_as_one_line_per_paragraph_and_nothing_else_changes() {
        let mut v = json!({ "description": "one\ntwo\n\nthree", "pattern": "a\nb", "items": [{ "description": "x\ny" }] });
        unwrap_descriptions(&mut v);
        assert_eq!(
            v,
            json!({ "description": "one two\n\nthree", "pattern": "a\nb", "items": [{ "description": "x y" }] })
        );
    }
}
