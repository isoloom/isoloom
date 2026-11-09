//! The manifest's JSON Schema, generated from the types (committed at
//! `schemas/run-manifest-v1.schema.json`; a test keeps the file current), and the compatibility
//! policy it states.

use serde_json::{Map, Value, json};

use crate::Manifest;

/// Where the schema is published: the committed file on the `main` branch.
pub const SCHEMA_URL: &str = "https://raw.githubusercontent.com/isoloom/isoloom/main/schemas/run-manifest-v1.schema.json";

/// What may change within one `manifest_version`, and what bumps it.
pub const COMPATIBILITY: &str = "Within a version, changes are additive: a new optional field comes in a minor release and the version stays. Removing or renaming a field, changing what a field holds or means, making a field required, or adding a value a runner must act on (a step action, a delivery, a condition) bumps the version. A reader is strict: it refuses a field or value it doesn't know, so a manifest that uses a field added in a minor release needs a runner built with that release.";

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
            format!(
                "What Isoloom writes next to one target's generated files (`.isoloom[-<n>]/<target>[/<cloud>]/run.json`) and a runner executes. Declarative and deterministic: no secret values, no absolute paths, no timestamps, no host shell. It carries its own version (`manifest_version`): a reader takes the versions it knows.\n\n{COMPATIBILITY}"
            )
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
    fn the_schema_names_its_published_file_its_version_and_the_policy() {
        let s = schema();
        assert_eq!(s["$id"], SCHEMA_URL);
        assert!(SCHEMA_URL.ends_with("/main/schemas/run-manifest-v1.schema.json"), "{SCHEMA_URL}");
        let description = s["description"].as_str().unwrap_or_default();
        assert!(
            description.contains("(`manifest_version`)") && description.ends_with(COMPATIBILITY),
            "{description}"
        );
        assert_eq!(s.pointer("/$defs/ManifestVersion/const"), Some(&json!(1)));
    }

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
