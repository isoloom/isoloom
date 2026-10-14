use serde_json::json;

use super::*;

/// A schema with each shape the walk meets: a map of definitions, a list of mappings, a field
/// that is a scalar or a mapping, a closed set, limits, and a shallow field.
fn schema() -> Value {
    json!({
        "properties": {
            "version": { "type": "integer", "minimum": 1, "maximum": 1, "examples": [1] },
            "name": { "type": "string", "pattern": "^[a-z]+$", "description": "The name,\nwrapped." },
            "port": { "type": "integer", "format": "uint16", "minimum": 0, "maximum": 65535 },
            "small": { "type": "integer", "minimum": 2 },
            "big": { "type": "integer", "maximum": 9 },
            "items": { "type": "array" },
            "free": {},
            "nets": { "type": "object", "additionalProperties": false, "patternProperties": { "^[a-z]+$": { "$ref": "#/$defs/Net" } }, "propertyNames": { "pattern": "^[a-z]+$" }, "default": {} },
            "list": { "type": "array", "items": { "$ref": "#/$defs/Item" }, "default": [] },
            "gpus": { "$ref": "#/$defs/Gpus", "description": "Overrides [`Gpus`]." },
            "arch": { "$ref": "#/$defs/Arch", "default": "amd64" },
            "os": { "type": "string", "enum": ["a", "b"] },
            "vols": { "type": "object", "additionalProperties": { "type": "string", "pattern": "^/" } },
            "common": { "$ref": "#/$defs/Shared" },
            "loose": { "properties": { "x": { "type": "boolean", "default": false } } },
            "groups": { "type": "object", "additionalProperties": { "$ref": "#/$defs/Shared" } }
        },
        "$defs": {
            "Net": { "type": "object", "properties": { "cidr": { "type": "string" } } },
            "Item": { "anyOf": [{ "type": "string" }, { "type": "object", "properties": { "port": { "type": "integer" } } }] },
            "Gpus": { "anyOf": [{ "type": "integer" }, { "$ref": "#/$defs/Full" }], "description": "GPUs." },
            "Full": { "type": "object", "properties": { "count": { "type": "integer" } } },
            "Arch": { "oneOf": [{ "const": "amd64", "description": "x86-64." }, { "const": "arm64", "description": "ARM." }] },
            "Shared": { "type": "object", "properties": { "arch": {}, "dns": {} } }
        }
    })
}

fn field<'a>(all: &'a [Field], path: &str) -> &'a Field {
    all.iter().find(|f| f.path == path).unwrap_or_else(|| panic!("no {path}"))
}

#[test]
fn every_shape_is_walked_with_its_type_in_words() {
    let all = fields(&schema());
    let paths: Vec<&str> = all.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "version",
            "name",
            "port",
            "small",
            "big",
            "items",
            "free",
            "nets",
            "nets.*.cidr",
            "list",
            "list[].port",
            "gpus",
            "gpus.count",
            "arch",
            "os",
            "vols",
            "common",
            "loose",
            "loose.x",
            "groups"
        ]
    );
    let kinds: Vec<&str> = all.iter().map(|f| f.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "integer",
            "string",
            "integer",
            "integer",
            "integer",
            "list of values",
            "value",
            "map of name to mapping",
            "string",
            "list of strings or mappings",
            "integer",
            "integer or mapping",
            "integer",
            "string",
            "string",
            "map of name to string",
            "mapping",
            "mapping",
            "boolean",
            "map of name to mapping"
        ]
    );
}

#[test]
fn defaults_values_limits_and_descriptions_read_as_a_spec_writes_them() {
    let all = fields(&schema());
    let version = field(&all, "version");
    assert_eq!(
        (version.limits.clone(), version.examples.clone()),
        (vec!["from 1 to 1".to_string()], vec!["1".to_string()])
    );
    let name = field(&all, "name");
    assert_eq!(
        (name.description.as_str(), name.limits[0].as_str()),
        ("The name, wrapped.", "matches `^[a-z]+$`")
    );
    assert!(field(&all, "port").limits.is_empty(), "a range that is only the format's");
    assert_eq!(field(&all, "small").limits, ["at least 2"]);
    assert_eq!(field(&all, "big").limits, ["at most 9"]);
    assert_eq!(field(&all, "nets").limits, ["names match `^[a-z]+$`"]);
    assert_eq!(field(&all, "vols").limits, ["each value matches `^/`"]);
    assert_eq!((field(&all, "nets").default.clone(), field(&all, "list").default.clone()), (None, None));
    assert_eq!(field(&all, "loose.x").default.as_deref(), Some("false"));
}

#[test]
fn closed_sets_shallow_fields_and_rustdoc_links_read_plainly() {
    let all = fields(&schema());
    let arch = field(&all, "arch");
    assert_eq!(arch.default.as_deref(), Some("amd64"));
    assert_eq!(
        arch.allowed,
        [("amd64".to_string(), "x86-64.".to_string()), ("arm64".to_string(), "ARM.".to_string())]
    );
    assert_eq!(field(&all, "os").allowed, [("a".to_string(), String::new()), ("b".to_string(), String::new())]);
    assert_eq!(field(&all, "gpus").description, "Overrides `Gpus`.");
    assert_eq!(field(&all, "common").members, ["arch", "dns"]);
    assert_eq!(field(&all, "groups").members, ["arch", "dns"]);
}

#[test]
fn a_schema_that_refers_to_itself_stops() {
    let looping = json!({
        "properties": { "a": { "$ref": "#/$defs/A" }, "b": { "$ref": "#/$defs/Missing" } },
        "$defs": { "A": { "type": "object", "properties": { "a": { "$ref": "#/$defs/A" } } } }
    });
    let all = fields(&looping);
    assert!(all.len() > MAX_DEPTH / 2 && all.len() <= MAX_DEPTH + 1, "{}", all.len());
    assert_eq!(field(&all, "b").kind, "value");
    let alias = json!({ "properties": { "x": { "$ref": "#/$defs/X" } }, "$defs": { "X": { "$ref": "#/$defs/X" } } });
    assert_eq!(fields(&alias)[0].kind, "value");
}

#[test]
fn headings_get_github_anchors() {
    assert_eq!(slug("From the spec's side"), "from-the-specs-side");
    assert_eq!(slug("`machines.*.vm.os`"), "machinesvmos");
    assert_eq!(slug("Top level"), "top-level");
}
