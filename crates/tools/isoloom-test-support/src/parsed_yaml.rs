//! Rendered YAML (Compose files, Kubernetes manifests, Ansible), parsed, and read by path.

/// A rendered YAML file (Compose, Kubernetes, Ansible), parsed.
///
/// # Panics
/// When it isn't YAML.
#[must_use]
pub fn yaml(text: &str) -> serde_yaml_ng::Value {
    serde_yaml_ng::from_str(text).unwrap_or_else(|e| panic!("not YAML: {e}\n{text}"))
}

/// The value at `path` in `value`: keys separated by `.`, a list's item by its index
/// (`services.web.networks.lan.ipv4_address`, `services.web.ports.0`); a key holding a `.` is
/// written in backquotes (`` labels.`isoloom.managed` ``).
#[must_use]
pub fn yaml_at<'v>(value: &'v serde_yaml_ng::Value, path: &str) -> Option<&'v serde_yaml_ng::Value> {
    let mut at = value;
    for key in yaml_keys(path) {
        at = match at {
            serde_yaml_ng::Value::Sequence(items) => items.get(key.parse::<usize>().ok()?)?,
            serde_yaml_ng::Value::Mapping(map) => map.get(key.as_str())?,
            _ => return None,
        };
    }
    Some(at)
}

/// The text of the scalar at `path` in `value` (a string, a number or a boolean as written).
///
/// # Panics
/// When there is none there, or it isn't a scalar.
#[must_use]
pub fn yaml_str(value: &serde_yaml_ng::Value, path: &str) -> String {
    match yaml_at(value, path) {
        Some(serde_yaml_ng::Value::String(s)) => s.clone(),
        Some(serde_yaml_ng::Value::Number(n)) => n.to_string(),
        Some(serde_yaml_ng::Value::Bool(b)) => b.to_string(),
        other => panic!("no scalar at `{path}` (found {other:?})"),
    }
}

/// The texts of the list at `path` in `value`, each item a scalar; empty when there is no list.
#[must_use]
pub fn yaml_strs(value: &serde_yaml_ng::Value, path: &str) -> Vec<String> {
    match yaml_at(value, path) {
        Some(serde_yaml_ng::Value::Sequence(items)) => items
            .iter()
            .map(|v| match v {
                serde_yaml_ng::Value::String(s) => s.clone(),
                other => serde_yaml_ng::to_string(other).unwrap_or_default().trim_end().to_owned(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The keys of the mapping at `path` in `value`, in order; empty when there is none.
#[must_use]
pub fn yaml_keys_at(value: &serde_yaml_ng::Value, path: &str) -> Vec<String> {
    let at = if path.is_empty() { Some(value) } else { yaml_at(value, path) };
    match at {
        Some(serde_yaml_ng::Value::Mapping(map)) => map.keys().filter_map(|k| k.as_str().map(str::to_owned)).collect(),
        _ => Vec::new(),
    }
}

/// `path`'s keys: split at `.`, a backquoted key whole.
fn yaml_keys(path: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut key = String::new();
    let mut quoted = false;
    for c in path.chars() {
        match c {
            '`' => quoted = !quoted,
            '.' if !quoted => keys.push(std::mem::take(&mut key)),
            c => key.push(c),
        }
    }
    keys.push(key);
    keys
}

/// Every document of a rendered multi-document YAML file (Kubernetes manifests), parsed; the
/// empty ones left out.
///
/// # Panics
/// When a document isn't YAML.
#[must_use]
pub fn yaml_documents(text: &str) -> Vec<serde_yaml_ng::Value> {
    use serde::Deserialize as _;
    serde_yaml_ng::Deserializer::from_str(text)
        .map(|d| serde_yaml_ng::Value::deserialize(d).unwrap_or_else(|e| panic!("not YAML: {e}\n{text}")))
        .filter(|v| !v.is_null())
        .collect()
}

/// The one manifest of `kind` named `name` among `docs`.
///
/// # Panics
/// When there isn't exactly one.
#[must_use]
pub fn manifest<'d>(docs: &'d [serde_yaml_ng::Value], kind: &str, name: &str) -> &'d serde_yaml_ng::Value {
    let found: Vec<_> = docs
        .iter()
        .filter(|d| d.get("kind").and_then(serde_yaml_ng::Value::as_str) == Some(kind))
        .filter(|d| yaml_at(d, "metadata.name").and_then(serde_yaml_ng::Value::as_str) == Some(name))
        .collect();
    match found.as_slice() {
        [one] => one,
        _ => panic!("{} {kind} `{name}` among the manifests", found.len()),
    }
}

/// The names of the manifests of `kind` among `docs`, in order.
#[must_use]
pub fn manifest_names(docs: &[serde_yaml_ng::Value], kind: &str) -> Vec<String> {
    docs.iter()
        .filter(|d| d.get("kind").and_then(serde_yaml_ng::Value::as_str) == Some(kind))
        .filter_map(|d| yaml_at(d, "metadata.name").and_then(serde_yaml_ng::Value::as_str).map(str::to_owned))
        .collect()
}

/// The item of the list at `path` in `value` whose `name` is `name` (a pod's container, a
/// volume, an init container).
///
/// # Panics
/// When there isn't exactly one.
#[must_use]
pub fn named<'v>(value: &'v serde_yaml_ng::Value, path: &str, name: &str) -> &'v serde_yaml_ng::Value {
    let items = yaml_at(value, path)
        .and_then(serde_yaml_ng::Value::as_sequence)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let found: Vec<_> = items
        .iter()
        .filter(|i| i.get("name").and_then(serde_yaml_ng::Value::as_str) == Some(name))
        .collect();
    match found.as_slice() {
        [one] => one,
        _ => panic!("{} items named `{name}` at `{path}`", found.len()),
    }
}
