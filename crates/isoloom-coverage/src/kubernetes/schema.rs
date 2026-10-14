//! The pinned field lists the Kubernetes, Kustomize and Helm tables come from
//! (`coverage/kubernetes/*.txt`, written by `scripts/refresh-kubernetes.py`): one line per field,
//! `Definition field type`, and for the Kubernetes OpenAPI one line per object kind,
//! `kind Kind apiVersion Definition`. Types keep each field's definition, so the tests can follow
//! a generated document down to the field each key is.

use crate::Support;

/// An object kind of the API, and the definition its documents follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    /// `Deployment`.
    pub name: &'static str,
    /// `apps/v1`.
    pub api_version: &'static str,
    /// `Deployment` (the definition's short name).
    pub definition: &'static str,
}

/// A field of a definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// The definition it belongs to (`PodSpec`).
    pub definition: &'static str,
    /// Its name (`hostname`).
    pub name: &'static str,
    /// Its type: a definition, a JSON type, `[]` before a list's items or `{}` before a map's values.
    pub kind: &'static str,
}

impl Field {
    /// The definition its value (or each item or map value) follows, when it is one.
    pub fn target(&self) -> &'static str {
        self.kind.trim_start_matches(['[', ']', '{', '}'])
    }

    /// A list of items.
    pub fn is_list(&self) -> bool {
        self.kind.starts_with("[]")
    }

    /// A map of values.
    pub fn is_map(&self) -> bool {
        self.kind.starts_with("{}")
    }
}

/// One pinned field list.
#[derive(Debug, Clone, Default)]
pub struct Schema {
    /// The object kinds, when the list has any.
    pub kinds: Vec<Kind>,
    /// Every field, in the list's order.
    pub fields: Vec<Field>,
}

impl Schema {
    /// Reads a pinned list; comments, blank and malformed lines are skipped.
    pub fn parse(text: &'static str) -> Schema {
        let mut schema = Schema::default();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let parts: Vec<&'static str> = line.split_whitespace().collect();
            match parts.as_slice() {
                ["kind", name, api_version, definition] => schema.kinds.push(Kind { name, api_version, definition }),
                [definition, name, kind] => schema.fields.push(Field { definition, name, kind }),
                _ => {}
            }
        }
        schema
    }

    /// The fields of one definition (none for a scalar such as `Quantity`).
    pub fn fields_of<'a>(&'a self, definition: &'a str) -> impl Iterator<Item = &'a Field> + 'a {
        self.fields.iter().filter(move |f| f.definition == definition)
    }

    /// One field of a definition.
    pub fn field(&self, definition: &str, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.definition == definition && f.name == name)
    }

    /// The kind named `name`.
    pub fn kind(&self, name: &str) -> Option<&Kind> {
        self.kinds.iter().find(|k| k.name == name)
    }
}

/// A table's row key for a field: its definition is its section (`PodSpec: hostname`).
pub fn key(definition: &str, field: &str) -> String {
    format!("{definition}: {field}")
}

/// The rows of the tabled definitions, in the order given and each definition's fields in the
/// list's order; a field `classify` doesn't know stays unclassified, which the tests reject.
pub fn rows(schema: &Schema, tabled: &[&str], classify: fn(&str, &str) -> Option<Support>) -> Vec<(String, Support)> {
    tabled
        .iter()
        .flat_map(|d| schema.fields_of(d))
        .map(|f| (key(f.definition, f.name), classify(f.definition, f.name).unwrap_or(Support::Unclassified)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "# a comment\nkind Pod v1 Pod\nPod spec PodSpec\nPodSpec volumes []Volume\nPodSpec nodeSelector {}string\n";

    #[test]
    fn reads_kinds_and_fields() {
        let s = Schema::parse(TEXT);
        assert_eq!(s.kind("Pod").map(|k| k.api_version), Some("v1"));
        assert_eq!(s.fields_of("PodSpec").count(), 2);
        let volumes = s.field("PodSpec", "volumes").copied().unwrap_or(Field {
            definition: "",
            name: "",
            kind: "",
        });
        assert!(volumes.is_list() && !volumes.is_map());
        assert_eq!(volumes.target(), "Volume");
        assert!(s.field("PodSpec", "nodeSelector").is_some_and(Field::is_map));
    }

    #[test]
    fn unknown_fields_are_unclassified() {
        let s = Schema::parse(TEXT);
        let rows = rows(&s, &["PodSpec"], |_, f| (f == "volumes").then_some(Support::Tooling { note: "x" }));
        let unknown = rows.iter().find(|(k, _)| k == "PodSpec: nodeSelector").map(|(_, s)| *s);
        assert_eq!(unknown, Some(Support::Unclassified));
    }
}
