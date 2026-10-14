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
    let mut s = Value::from(schemars::schema_for!(Spec));
    // Optional fields come out as "this, or null". A spec never writes null, and editors report
    // a typo inside such a field as one vague "anyOf" error: keep only the real type.
    drop_null(&mut s);
    s["$id"] = json!(SCHEMA_URL);
    s["title"] = json!("isoloom.yml");
    s["description"] = json!("An Isoloom environment spec, version 1: https://www.isoloom.com/en/docs/spec-reference");
    names(&mut s);
    networks(&mut s);
    machines(&mut s);
    s
}

/// Sets `key` to `v` on the node at `path`, when that node is an object.
fn set(s: &mut Value, path: &[&str], key: &str, v: Value) {
    let mut node = s;
    for p in path {
        node = &mut node[*p];
    }
    if node.is_object() {
        node[key] = v;
    }
}

/// The version, and the patterns of the environment's, networks', machines' and inputs' names.
fn names(s: &mut Value) {
    let kebab = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";
    set(s, &["properties", "version"], "const", json!(1));
    set(s, &["properties", "name"], "pattern", json!("^[a-z0-9]+(-[a-z0-9]+)*$"));
    set(s, &["properties", "networks"], "propertyNames", json!({ "pattern": kebab }));
    set(s, &["properties", "machines"], "propertyNames", json!({ "pattern": kebab }));
    set(s, &["properties", "inputs", "items"], "pattern", json!("^[A-Za-z_][A-Za-z0-9_]*$"));
}

/// The networks' blocks: a network is a /24 to /29; a LAN split into VLANs may be larger (from
/// /8), its VLANs are the /24 to /29 networks; Docker's blocks are inside 10.0.0.0/8.
fn networks(s: &mut Value) {
    let private = r"^(10\.\d{1,3}|172\.(1[6-9]|2\d|3[01])|192\.168)\.\d{1,3}\.\d{1,3}";
    let network = format!("{private}/(2[4-9])$");
    set(
        s,
        &["$defs", "Network", "properties", "cidr"],
        "pattern",
        json!(format!(r"{private}/([89]|1\d|2[0-9])$")),
    );
    set(s, &["$defs", "Network"], "if", json!({ "not": { "required": ["vlans"] } }));
    set(s, &["$defs", "Network"], "then", json!({ "properties": { "cidr": { "pattern": network } } }));
    set(
        s,
        &["$defs", "Network", "properties", "vlans"],
        "propertyNames",
        json!({ "pattern": "^[1-9][0-9]{0,3}$" }),
    );
    set(s, &["$defs", "Vlan", "properties", "cidr"], "pattern", json!(network));
    set(s, &["$defs", "Bridged", "properties", "vlan"], "minimum", json!(1));
    set(s, &["$defs", "Bridged", "properties", "vlan"], "maximum", json!(4094));
    set(
        s,
        &["$defs", "NetworkDocker", "properties", "cidr"],
        "pattern",
        json!(r"^10\.\d{1,3}\.\d{1,3}\.\d{1,3}/(2[4-9])$"),
    );
}

/// The machines: known OS names, the last octet on each network, absolute volume paths.
fn machines(s: &mut Value) {
    set(s, &["$defs", "VmImpl", "properties", "os"], "enum", json!(KNOWN_OS));
    // `vm: {}` is complete when `common:` or a group supplies the OS.
    set(s, &["$defs", "VmImpl"], "required", json!([]));
    set(
        s,
        &["$defs", "Machine", "properties", "networks"],
        "additionalProperties",
        // 0: an address from the LAN's DHCP server, on a `bridged: { dhcp: true }` network. A
        // list: several addresses on the network, the first the primary one.
        json!({ "anyOf": [
            { "type": "integer", "minimum": 0, "maximum": 254, "description": "The last octet of the machine's address on the network." },
            {
                "type": "array",
                "items": { "type": "integer", "minimum": 1, "maximum": 254 },
                "minItems": 1,
                "uniqueItems": true,
                "description": "Several addresses on the network, by last octet: the first is the machine's primary address (its name resolves to it), the others are on the same interface."
            }
        ] }),
    );
    set(
        s,
        &["$defs", "Machine", "properties", "volumes"],
        "additionalProperties",
        // A path, or a path and the folder of a shared volume (`Mount`), both absolute.
        json!({ "anyOf": [
            { "type": "string", "pattern": "^/", "description": "The whole volume at this absolute path." },
            { "$ref": "#/$defs/MountPart" },
        ] }),
    );
    set(s, &["$defs", "MountPart", "properties", "path"], "pattern", json!("^/"));
}

/// Replaces `anyOf: [X, {type: null}]` with X and `type: [T, "null"]` with T, everywhere.
fn drop_null(v: &mut Value) {
    match v {
        Value::Object(map) => {
            if let Some(Value::Array(any)) = map.get("anyOf")
                && any.len() == 2
                && any.iter().any(|b| b.get("type") == Some(&json!("null")))
                && let Some(keep) = any.iter().find(|b| b.get("type") != Some(&json!("null"))).cloned()
            {
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
                && let Some(t) = types.iter().find(|t| **t != json!("null")).cloned()
            {
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
