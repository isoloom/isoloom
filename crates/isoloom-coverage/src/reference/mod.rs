//! The spec reference, `docs/spec.md`: every field of `isoloom.yml` read from its JSON schema
//! (type, default, allowed values, limits, description), with what each output does with it
//! read from the spec table. Both sources are the code itself, so the page can't drift: a test
//! fails when the committed page differs from what `isoloom docs --spec` prints.

mod example;
mod markdown;
mod text;

use serde_json::{Map, Value};

pub use markdown::{markdown, slug};

/// Fields whose own fields repeat a machine's: listed by name only, since the machine's
/// section documents them.
const SHALLOW: &[&str] = &["common", "groups"];

/// How deep the walk goes, a guard against a schema that refers to itself.
const MAX_DEPTH: usize = 10;

/// One field of the spec as the reference shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Field {
    /// Its path, the way the spec table names rows: `*` for a name the author chooses, `[]` for
    /// the items of a list (`machines.*.services[].port`).
    pub path: String,
    /// Its type in words (`string`, `list of mappings`, `map of name to integer`).
    pub kind: String,
    /// What it is for, from the model's doc comment.
    pub description: String,
    /// Its default value as JSON, when the schema states one worth showing.
    pub default: Option<String>,
    /// The values it accepts, each with what it means, when it is a closed set.
    pub allowed: Vec<(String, String)>,
    /// Ranges and patterns the schema enforces beyond the type.
    pub limits: Vec<String>,
    /// Examples the schema carries.
    pub examples: Vec<String>,
    /// For a shallow field (`common`, `groups`), the names of the fields it holds.
    pub members: Vec<String>,
}

/// Every field of `schema` (the output of `isoloom_spec::schema::schema`), in the schema's
/// order: a field, then the fields inside it.
pub fn fields(schema: &Value) -> Vec<Field> {
    let empty = Map::new();
    let walker = Walker {
        defs: schema.get("$defs").and_then(Value::as_object).unwrap_or(&empty),
    };
    let mut out = Vec::new();
    walker.children(schema, "", 0, &mut out);
    out
}

/// Walks the schema, following `$ref`s into its definitions.
struct Walker<'a> {
    defs: &'a Map<String, Value>,
}

impl<'a> Walker<'a> {
    /// The node a `$ref` points to (followed until there is none), or the node itself.
    fn resolve(&self, node: &'a Value) -> &'a Value {
        let mut node = node;
        for _ in 0..MAX_DEPTH {
            let Some(name) = node.get("$ref").and_then(Value::as_str).and_then(|r| r.strip_prefix("#/$defs/")) else {
                break;
            };
            match self.defs.get(name) {
                Some(next) => node = next,
                None => break,
            }
        }
        node
    }

    /// The property maps `node` can hold: its own, or each of its alternatives' (a field that
    /// is a scalar or a mapping, like `gpus`).
    fn objects(&self, node: &'a Value, depth: usize) -> Vec<&'a Map<String, Value>> {
        let node = self.resolve(node);
        if let Some(p) = node.get("properties").and_then(Value::as_object) {
            return vec![p];
        }
        if depth >= MAX_DEPTH {
            return Vec::new();
        }
        text::variants(node).iter().flat_map(|v| self.objects(v, depth + 1)).collect()
    }

    /// The fields inside `node`, at `path`: its properties, the properties of what its map
    /// holds (at `path.*`), or of its list's items (at `path[]`).
    fn children(&self, node: &'a Value, path: &str, depth: usize, out: &mut Vec<Field>) {
        if depth >= MAX_DEPTH {
            return;
        }
        let node = self.resolve(node);
        let (inner, at) = match (text::map_value(node), node.get("items")) {
            (Some(value), _) => (value, format!("{path}.*")),
            (_, Some(items)) => (items, format!("{path}[]")),
            _ => (node, path.to_string()),
        };
        for props in self.objects(inner, depth) {
            for (name, child) in props {
                let child_path = if at.is_empty() { name.clone() } else { format!("{at}.{name}") };
                if out.iter().any(|f| f.path == child_path) {
                    continue;
                }
                self.field(child, &child_path, depth + 1, out);
            }
        }
    }

    /// The field at `path`, then the fields inside it.
    fn field(&self, node: &'a Value, path: &str, depth: usize, out: &mut Vec<Field>) {
        let target = self.resolve(node);
        let shallow = SHALLOW.contains(&path);
        let pick = |key: &str| node.get(key).or_else(|| target.get(key));
        let description = pick("description").and_then(Value::as_str).map(text::prose).unwrap_or_default();
        let mut allowed = text::allowed(self, node);
        // A single value whose meaning is the field's own description says it once.
        if let [(_, meaning)] = allowed.as_mut_slice()
            && *meaning == description
        {
            meaning.clear();
        }
        out.push(Field {
            kind: text::kind(self, node, depth),
            description,
            default: pick("default").and_then(text::default),
            allowed,
            limits: text::limits(self, node),
            examples: pick("examples")
                .and_then(Value::as_array)
                .map(|e| e.iter().map(text::json).collect())
                .unwrap_or_default(),
            members: if shallow { self.member_names(node, depth) } else { Vec::new() },
            path: path.to_string(),
        });
        if !shallow {
            self.children(node, path, depth, out);
        }
    }

    /// The names of the fields a shallow field holds (in its map's values, for `groups`).
    fn member_names(&self, node: &'a Value, depth: usize) -> Vec<String> {
        let node = self.resolve(node);
        let inner = text::map_value(node).unwrap_or(node);
        self.objects(inner, depth).into_iter().flat_map(|p| p.keys().cloned()).collect()
    }
}

#[cfg(test)]
mod tests;
