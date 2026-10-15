//! A schema node in words: its type, its allowed values, its limits, its default.

use serde_json::Value;

use super::{MAX_DEPTH, Walker};

/// The alternatives of a node that is one of several shapes (`anyOf`, `oneOf`).
pub(super) fn variants(node: &Value) -> &[Value] {
    ["anyOf", "oneOf"]
        .iter()
        .find_map(|k| node.get(*k).and_then(Value::as_array))
        .map_or(&[], Vec::as_slice)
}

/// What a map holds under each name: its `additionalProperties`, or the schema of its names'
/// pattern (`vlans`, by id).
pub(super) fn map_value(node: &Value) -> Option<&Value> {
    node.get("additionalProperties")
        .filter(|ap| ap.is_object())
        .or_else(|| node.get("patternProperties").and_then(Value::as_object).and_then(|p| p.values().next()))
}

/// The node's type in words.
pub(super) fn kind(w: &Walker, node: &Value, depth: usize) -> String {
    let node = w.resolve(node);
    if depth >= MAX_DEPTH {
        return "value".to_string();
    }
    if closed(w, node).is_some() {
        return closed_kind(w, node, depth).to_string();
    }
    let alternatives = variants(node);
    if !alternatives.is_empty() {
        let mut kinds: Vec<String> = Vec::new();
        for v in alternatives {
            let k = kind(w, v, depth + 1);
            if !kinds.contains(&k) {
                kinds.push(k);
            }
        }
        return kinds.join(" or ");
    }
    match node.get("type").and_then(Value::as_str) {
        Some("array") => {
            let items = node.get("items").map_or_else(|| "value".to_string(), |i| kind(w, i, depth + 1));
            format!("list of {}", plural(&items))
        }
        Some("object") => match map_value(node) {
            Some(value) => format!("map of name to {}", kind(w, value, depth + 1)),
            None => "mapping".to_string(),
        },
        Some(t) => t.to_string(),
        None if node.get("properties").is_some() => "mapping".to_string(),
        None => "value".to_string(),
    }
}

/// The type of a closed set's values, from its first one.
fn closed_kind(w: &Walker, node: &Value, depth: usize) -> &'static str {
    let node = w.resolve(node);
    let first = node.get("const").or_else(|| node.get("enum").and_then(|e| e.get(0)));
    match first {
        Some(Value::String(_)) => "string",
        Some(Value::Number(n)) if n.is_f64() => "number",
        Some(Value::Number(_)) => "integer",
        Some(Value::Bool(_)) => "boolean",
        Some(_) => "value",
        None => match variants(node).first() {
            Some(v) if depth < MAX_DEPTH => closed_kind(w, v, depth + 1),
            _ => "value",
        },
    }
}

/// `string or mapping` as a plural: `strings or mappings`.
fn plural(kind: &str) -> String {
    kind.split(" or ")
        .map(|alt| match alt.split_once(' ') {
            Some((first, rest)) => format!("{first}s {rest}"),
            None => format!("{alt}s"),
        })
        .collect::<Vec<_>>()
        .join(" or ")
}

/// The values of a closed set, each with its meaning: an `enum`, a `const`, or alternatives
/// that are all closed. `None` when the node accepts other values.
fn closed(w: &Walker, node: &Value) -> Option<Vec<(String, String)>> {
    let node = w.resolve(node);
    let meaning = node.get("description").and_then(Value::as_str).map(prose).unwrap_or_default();
    if let Some(values) = node.get("enum").and_then(Value::as_array) {
        return Some(values.iter().map(|v| (plain(v), String::new())).collect());
    }
    if let Some(c) = node.get("const") {
        return Some(vec![(plain(c), meaning)]);
    }
    let alternatives = variants(node);
    if alternatives.is_empty() {
        return None;
    }
    let mut all = Vec::new();
    for v in alternatives {
        all.extend(closed(w, v)?);
    }
    Some(all)
}

/// The values the node accepts, when they are a closed set.
pub(super) fn allowed(w: &Walker, node: &Value) -> Vec<(String, String)> {
    closed(w, node).unwrap_or_default()
}

/// The ranges and patterns the node enforces, beyond its type.
pub(super) fn limits(w: &Walker, node: &Value) -> Vec<String> {
    let target = w.resolve(node);
    let pick = |key: &str| node.get(key).or_else(|| target.get(key));
    let mut out = Vec::new();
    if let Some(range) = range(pick("minimum"), pick("maximum"), pick("format")) {
        out.push(range);
    }
    let nested = [
        ("pattern", None, "matches"),
        ("propertyNames", Some("pattern"), "names match"),
        ("items", Some("pattern"), "each item matches"),
        ("additionalProperties", Some("pattern"), "each value matches"),
    ];
    for (key, inner, words) in nested {
        let found = pick(key).and_then(|v| match inner {
            Some(i) => v.get(i),
            None => Some(v),
        });
        if let Some(p) = found.and_then(Value::as_str) {
            out.push(format!("{words} `{p}`"));
        }
    }
    out
}

/// A range in words, unless it only restates the integer's own format (`uint16`: 0 to 65535).
fn range(min: Option<&Value>, max: Option<&Value>, format: Option<&Value>) -> Option<String> {
    let (min, max) = (min.and_then(Value::as_f64), max.and_then(Value::as_f64));
    let bound = match format.and_then(Value::as_str) {
        Some("uint8") => Some(255.0),
        Some("uint16") => Some(65535.0),
        _ => None,
    };
    match (min, max) {
        (Some(lo), Some(hi)) if lo == 0.0 && Some(hi) == bound => None,
        (Some(lo), Some(hi)) => Some(format!("from {} to {}", number(lo), number(hi))),
        (Some(lo), None) if lo > 0.0 => Some(format!("at least {}", number(lo))),
        (None, Some(hi)) => Some(format!("at most {}", number(hi))),
        _ => None,
    }
}

/// A bound as written: `8`, not `8.0`.
fn number(n: f64) -> String {
    let s = n.to_string();
    s.strip_suffix(".0").map_or_else(|| s.clone(), str::to_string)
}

/// A default worth showing: not null, and not an empty string, list or map.
pub(super) fn default(v: &Value) -> Option<String> {
    let empty = match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    };
    (!empty).then(|| plain(v))
}

/// A value as JSON.
pub(super) fn json(v: &Value) -> String {
    v.to_string()
}

/// A value as a spec writes it: a string without its quotes, anything else as JSON.
fn plain(v: &Value) -> String {
    v.as_str().map_or_else(|| json(v), str::to_string)
}

/// A doc comment as prose: wrapped lines joined, paragraphs kept, and rustdoc's links
/// (``[`Spec::volumes`]``) shown as code.
pub(super) fn prose(s: &str) -> String {
    let joined = s
        .split("\n\n")
        .map(|p| p.split('\n').map(str::trim).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n\n");
    joined.replace("[`", "`").replace("`]", "`")
}
