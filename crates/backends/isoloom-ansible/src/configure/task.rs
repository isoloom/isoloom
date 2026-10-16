//! Building one Ansible task as an ordered JSON object, and serializing a list of them to block
//! YAML a role's `tasks/main.yml` and `handlers/main.yml` hold. `serde_json`'s `preserve_order`
//! keeps a task's keys in the order they are set (`name`, the module, then the directives), and
//! the emitter below writes the block YAML `ansible-lint` and `yamllint` accept: sequence items
//! indented under their key, multi-line strings as block scalars, task names capitalized, no
//! trailing space. Deterministic: the same value renders the same bytes twice.

use serde_json::{Map, Value};

/// A handler's name, for the service `unit` a changed task restarts. Capitalized, so a `notify`
/// and the handler read the same and `ansible-lint`'s `name[casing]` holds.
#[must_use]
pub(super) fn restart(unit: &str) -> String {
    format!("Isoloom restart {unit}")
}

/// One Ansible task, its keys kept in insertion order.
pub(super) struct Task(Map<String, Value>);

impl Task {
    /// A task named `name`, its first letter capitalized (`ansible-lint`'s `name[casing]`).
    pub(super) fn new(name: impl Into<String>) -> Task {
        let mut map = Map::new();
        map.insert("name".to_owned(), Value::String(capitalize(&name.into())));
        Task(map)
    }

    /// Its module and the module's arguments (`ansible.builtin.apt`, `{ name: [...] }`).
    #[must_use]
    pub(super) fn module(mut self, module: &str, args: Value) -> Task {
        self.0.insert(module.to_owned(), args);
        self
    }

    /// A task directive (`when`, `register`, `changed_when`, `failed_when`).
    #[must_use]
    pub(super) fn set(mut self, key: &str, value: Value) -> Task {
        self.0.insert(key.to_owned(), value);
        self
    }

    /// The service handlers this task restarts when it changes something (none: no `notify`).
    #[must_use]
    pub(super) fn notify(mut self, units: &[String]) -> Task {
        if !units.is_empty() {
            let names: Vec<Value> = units.iter().map(|u| Value::String(restart(u))).collect();
            self.0.insert("notify".to_owned(), Value::Array(names));
        }
        self
    }

    /// The task as a JSON object, for a list [`yaml`] serializes.
    #[must_use]
    pub(super) fn into_value(self) -> Value {
        Value::Object(self.0)
    }
}

/// `name` with its first letter capitalized.
fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// A list of tasks (or handlers, or plays) as block YAML: an empty list is `[]`, so the file
/// always parses.
#[must_use]
pub(super) fn yaml(tasks: Vec<Value>) -> String {
    document(&Value::Array(tasks))
}

/// Any value as the block YAML a generated file holds.
#[must_use]
pub(super) fn document(value: &Value) -> String {
    let mut out = String::new();
    match value {
        Value::Array(items) if items.is_empty() => out.push_str("[]\n"),
        Value::Array(items) => {
            for item in items {
                seq_item(&mut out, item, 0);
            }
        }
        Value::Object(map) if map.is_empty() => out.push_str("{}\n"),
        Value::Object(map) => emit_map(&mut out, map, 0),
        scalar => {
            out.push_str(&inline(scalar));
            out.push('\n');
        }
    }
    out
}

/// A mapping's entries at `indent`.
fn emit_map(out: &mut String, map: &Map<String, Value>, indent: usize) {
    for (key, value) in map {
        entry(out, key, value, &pad(indent), indent + 2);
    }
}

/// One `key: value` entry: `prefix` leads the key's line (a `- ` for the first key of a sequence
/// item), `child` is the indent of its nested content.
fn entry(out: &mut String, key: &str, value: &Value, prefix: &str, child: usize) {
    match value {
        Value::String(s) if s.contains('\n') => block(out, prefix, key, s, child),
        Value::Array(items) if items.is_empty() => push(out, &format!("{prefix}{key}: []")),
        Value::Object(map) if map.is_empty() => push(out, &format!("{prefix}{key}: {{}}")),
        Value::Array(items) => {
            push(out, &format!("{prefix}{key}:"));
            for item in items {
                seq_item(out, item, child);
            }
        }
        Value::Object(map) => {
            push(out, &format!("{prefix}{key}:"));
            emit_map(out, map, child);
        }
        scalar => push(out, &format!("{prefix}{key}: {}", inline(scalar))),
    }
}

/// One sequence item at `indent`: a mapping led by `- ` (its first key on that line, the rest
/// aligned under it), or a scalar.
fn seq_item(out: &mut String, value: &Value, indent: usize) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            let mut first = true;
            for (key, v) in map {
                let prefix = if first { format!("{}- ", pad(indent)) } else { pad(indent + 2) };
                // The key sits at `indent + 2` (after `- `), so its nested content is at `indent + 4`.
                entry(out, key, v, &prefix, indent + 4);
                first = false;
            }
        }
        _ => push(out, &format!("{}- {}", pad(indent), inline(value))),
    }
}

/// A multi-line string as a literal block scalar, its content at `child`; `|` keeps the trailing
/// newline the string ends with, `|-` writes none.
fn block(out: &mut String, prefix: &str, key: &str, s: &str, child: usize) {
    let (chomp, body) = match s.strip_suffix('\n') {
        Some(body) => ("|", body),
        None => ("|-", s),
    };
    push(out, &format!("{prefix}{key}: {chomp}"));
    for line in body.split('\n') {
        if line.is_empty() {
            out.push('\n');
        } else {
            push(out, &format!("{}{line}", pad(child)));
        }
    }
}

/// A scalar on the same line as its key or `- `: a number or bool as is, a string plain when it is
/// safe, else double-quoted.
fn inline(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => scalar(s),
        _ => "null".to_owned(),
    }
}

/// A string scalar: plain when it holds no YAML-special character and reads back as itself, else
/// double-quoted with the few escapes a double-quoted scalar needs.
fn scalar(s: &str) -> String {
    if plain(s) {
        s.to_owned()
    } else {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\t', "\\t"))
    }
}

/// Whether `s` is safe as a plain scalar: a path, a name, a package spec or an FQCN, never empty,
/// a reserved word, an all-digit string (a mode must stay a string) or anything with a space,
/// quote, colon or other indicator.
fn plain(s: &str) -> bool {
    let reserved = ["true", "false", "null", "yes", "no", "on", "off", "~"];
    if s.is_empty() || reserved.contains(&s.to_ascii_lowercase().as_str()) || s.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let safe = |c: char| c.is_ascii_alphanumeric() || "_/.=+@,-".contains(c);
    let first = s.chars().next().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '/');
    first && s.chars().all(safe)
}

/// Appends `line` and a newline (no trailing space: `yamllint`'s `trailing-spaces`).
fn push(out: &mut String, line: &str) {
    out.push_str(line.trim_end());
    out.push('\n');
}

/// `n` spaces.
fn pad(n: usize) -> String {
    " ".repeat(n)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_task_capitalizes_its_name_and_indents_its_sequences() {
        let task = Task::new("packages nginx")
            .module("ansible.builtin.apt", json!({ "name": ["nginx"], "update_cache": true }))
            .notify(&["nginx".to_owned()])
            .into_value();
        let text = yaml(vec![task]);
        assert!(text.starts_with("- name: \"Packages nginx\"\n"), "{text}");
        // The sequence under `name:` is indented four spaces (yamllint's indent-sequences).
        assert!(text.contains("  ansible.builtin.apt:\n    name:\n      - nginx\n"), "{text}");
        assert!(text.contains("  notify:\n    - \"Isoloom restart nginx\"\n"), "{text}");
    }

    #[test]
    fn a_mode_and_a_jinja_value_are_quoted_but_a_path_is_plain() {
        let task = Task::new("x")
            .module(
                "ansible.builtin.copy",
                json!({ "dest": "/etc/app.conf", "mode": "0640", "src": "{{ isoloom_project }}/a" }),
            )
            .into_value();
        let text = yaml(vec![task]);
        assert!(text.contains("dest: /etc/app.conf\n"), "a path is plain: {text}");
        assert!(text.contains("mode: \"0640\"\n"), "a mode stays a string: {text}");
        assert!(text.contains("src: \"{{ isoloom_project }}/a\"\n"), "a jinja value is quoted: {text}");
    }

    #[test]
    fn a_multiline_string_is_a_block_scalar() {
        let task = Task::new("x")
            .module("ansible.builtin.copy", json!({ "content": "a\nb\n", "dest": "/f" }))
            .into_value();
        let text = yaml(vec![task]);
        assert!(text.contains("content: |\n      a\n      b\n"), "{text}");
    }

    #[test]
    fn an_empty_list_is_valid_yaml() {
        assert_eq!(yaml(Vec::new()), "[]\n");
    }
}
