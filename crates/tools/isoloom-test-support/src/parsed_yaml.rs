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

/// The values of every key `key` anywhere in `value` (every `image` of a manifest's containers,
/// init containers included), each scalar as text, in document order: a key read by name, never
/// by a line that holds `key:`.
#[must_use]
pub fn yaml_values(value: &serde_yaml_ng::Value, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    collect(value, key, &mut out);
    out
}

fn collect(value: &serde_yaml_ng::Value, key: &str, out: &mut Vec<String>) {
    match value {
        serde_yaml_ng::Value::Mapping(map) => {
            for (k, v) in map {
                if k.as_str() == Some(key) {
                    match v {
                        serde_yaml_ng::Value::String(s) => out.push(s.clone()),
                        serde_yaml_ng::Value::Number(n) => out.push(n.to_string()),
                        serde_yaml_ng::Value::Bool(b) => out.push(b.to_string()),
                        _ => {}
                    }
                }
                collect(v, key, out);
            }
        }
        serde_yaml_ng::Value::Sequence(items) => items.iter().for_each(|v| collect(v, key, out)),
        serde_yaml_ng::Value::Tagged(tagged) => collect(&tagged.value, key, out),
        _ => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    const COMPOSE: &str = "services:\n  web:\n    image: nginx\n    ports: ['8080:80', 8443]\n    labels: { isoloom.managed: 'true' }\n    networks: { lan: { ipv4_address: 10.0.0.2 } }\n";

    #[test]
    fn a_value_is_read_by_its_path_a_list_item_by_index_a_dotted_key_in_backquotes() {
        let v = yaml(COMPOSE);
        assert_eq!(yaml_str(&v, "services.web.networks.lan.ipv4_address"), "10.0.0.2");
        assert_eq!(yaml_str(&v, "services.web.labels.`isoloom.managed`"), "true");
        assert_eq!(yaml_str(&v, "services.web.ports.1"), "8443", "a number as written");
        assert_eq!(yaml_strs(&v, "services.web.ports"), ["8080:80", "8443"]);
        assert_eq!(yaml_keys_at(&v, "services.web"), ["image", "ports", "labels", "networks"]);
        assert_eq!(yaml_keys_at(&v, ""), ["services"], "the top's keys");
        for missing in ["services.db", "services.web.ports.2", "services.web.image.x", "services.web.ports.first"] {
            assert_eq!(yaml_at(&v, missing), None, "{missing}");
        }
        assert_eq!(yaml_strs(&v, "services.web.image"), Vec::<String>::new(), "not a list");
    }

    #[test]
    #[should_panic(expected = "no scalar at `services.db.image`")]
    fn a_scalar_that_isnt_there_fails_the_test_rather_than_read_empty() {
        let _text = yaml_str(&yaml(COMPOSE), "services.db.image");
    }

    #[test]
    #[should_panic(expected = "not YAML")]
    fn text_that_isnt_yaml_fails_the_test() {
        let _value = yaml("a: [unclosed");
    }

    #[test]
    fn a_key_is_read_anywhere_by_name_in_document_order() {
        let pod = yaml("spec:\n  initContainers: [{ name: a, image: busybox }]\n  containers: [{ name: b, image: nginx, env: [{ name: image, value: x }] }]\n");
        assert_eq!(yaml_values(&pod, "image"), ["busybox", "nginx"]);
        assert_eq!(yaml_values(&pod, "name"), ["a", "b", "image"]);
    }

    const MANIFESTS: &str = "kind: Service\nmetadata: { name: web }\n---\n---\nkind: Deployment\nmetadata: { name: web }\nspec: { containers: [{ name: app, image: nginx }, { name: side, image: envoy }] }\n---\nkind: Deployment\nmetadata: { name: db }\n";

    #[test]
    fn manifests_are_found_by_kind_and_name_and_an_item_of_a_list_by_its_name() {
        let docs = yaml_documents(MANIFESTS);
        assert_eq!(docs.len(), 3, "the empty document left out");
        assert_eq!(manifest_names(&docs, "Deployment"), ["web", "db"]);
        let web = manifest(&docs, "Deployment", "web");
        assert_eq!(yaml_str(named(web, "spec.containers", "side"), "image"), "envoy");
    }

    #[test]
    #[should_panic(expected = "0 Deployment `cache` among the manifests")]
    fn a_manifest_that_isnt_there_fails_the_test() {
        let _found = manifest(&yaml_documents(MANIFESTS), "Deployment", "cache");
    }

    #[test]
    #[should_panic(expected = "0 items named `app` at `spec.volumes`")]
    fn an_item_that_isnt_in_the_list_fails_the_test() {
        let docs = yaml_documents(MANIFESTS);
        let _found = named(manifest(&docs, "Deployment", "web"), "spec.volumes", "app");
    }
}
