//! Building one Ansible task as an ordered JSON object, and serializing a list of them to block
//! YAML a role's `tasks/main.yml` and `handlers/main.yml` hold. `serde_json`'s `preserve_order`
//! keeps a task's keys in the order they are set (`name`, the module, then the directives), and
//! the emitter below writes the block YAML `ansible-lint` and `yamllint` accept: sequence items
//! indented under their key, multi-line strings as block scalars, task names capitalized, no
//! trailing space. Deterministic: the same value renders the same bytes twice.
//!
//! Ansible evaluates `{{ }}`, `{% %}` and `{# #}` in every string it reads, so a string holding
//! one is written `!unsafe` (taken as it is) unless it is Jinja Isoloom wrote itself ([`jinja`]): a
//! spec's command, path, name or file contents is never run through Jinja, as the shell renderer's
//! quoting never runs it through the shell.

use serde_json::{Map, Value};

/// A handler's name, for the service `unit` a changed task restarts. Capitalized, so a `notify`
/// and the handler read the same and `ansible-lint`'s `name[casing]` holds.
#[must_use]
pub(super) fn restart(unit: &str) -> String {
    format!("Isoloom restart {unit}")
}

/// The key of the one-entry object that marks [`jinja`] text. No task key is ever this (a NUL
/// starts it), so the emitter tells a template from a mapping.
const JINJA_KEY: &str = "\0jinja";

/// `text` as a Jinja template Ansible evaluates: Isoloom's own text (the project folder, a project
/// file under it), never the spec's, which the emitter writes `!unsafe`.
#[must_use]
pub(super) fn jinja(text: impl Into<String>) -> Value {
    let mut map = Map::new();
    map.insert(JINJA_KEY.to_owned(), Value::String(text.into()));
    Value::Object(map)
}

/// The text of a [`jinja`] value, `None` for any other value.
fn as_jinja(value: &Value) -> Option<&str> {
    match value {
        Value::Object(map) if map.len() == 1 => map.get(JINJA_KEY).and_then(Value::as_str),
        _ => None,
    }
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
        _ if as_jinja(value).is_some() => push(out, &format!("{prefix}{key}: {}", inline(value))),
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
        Value::Object(map) if !map.is_empty() && as_jinja(value).is_none() => {
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

/// A multi-line string as a literal block scalar, its content at `child`, or double-quoted when
/// it is only newlines, a line ends in a space or a tab, or the text holds another control
/// character (which a block scalar can't carry exactly, or a file mustn't hold). The indentation indicator (`2`: `child`
/// is two past the key) keeps a first line that starts with spaces from setting the indentation;
/// the chomping keeps every final newline: none `-`, one (clip), more `+`. `!unsafe` leads one
/// holding Jinja.
fn block(out: &mut String, prefix: &str, key: &str, s: &str, child: usize) {
    let body = s.trim_end_matches('\n');
    let blockable =
        !body.is_empty() && body.split('\n').all(|line| !line.ends_with([' ', '\t'])) && !s.chars().any(|c| c.is_control() && c != '\n' && c != '\t');
    if !blockable {
        push(out, &format!("{prefix}{key}: {}{}", unsafe_tag(s), quoted(s)));
        return;
    }
    let chomp = match s.len() - body.len() {
        0 => "-",
        1 => "",
        _ => "+",
    };
    push(out, &format!("{prefix}{key}: {}|2{chomp}", unsafe_tag(s)));
    let lines = body.split('\n').chain(std::iter::repeat_n("", (s.len() - body.len()).saturating_sub(1)));
    for line in lines {
        if line.is_empty() {
            out.push('\n');
        } else {
            out.push_str(&pad(child));
            out.push_str(line);
            out.push('\n');
        }
    }
}

/// A scalar on the same line as its key or `- `: a number or bool as is, [`jinja`] text quoted, a
/// string plain when it is safe, else double-quoted, and `!unsafe` when it holds Jinja.
fn inline(value: &Value) -> String {
    if let Some(text) = as_jinja(value) {
        return quoted(text);
    }
    match value {
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("{}{}", unsafe_tag(s), scalar(s)),
        _ => "null".to_owned(),
    }
}

/// `!unsafe ` when `s` holds a Jinja delimiter Ansible would evaluate, else nothing.
fn unsafe_tag(s: &str) -> &'static str {
    if ["{{", "{%", "{#"].iter().any(|d| s.contains(d)) { "!unsafe " } else { "" }
}

/// A string scalar: plain when it holds no YAML-special character and reads back as itself, else
/// double-quoted.
fn scalar(s: &str) -> String {
    if plain(s) { s.to_owned() } else { quoted(s) }
}

/// `s` double-quoted, every control character escaped: as a JSON string, which a YAML
/// double-quoted scalar reads the same, with DEL and the C1 controls (which JSON leaves as they
/// are and a YAML file mustn't hold) escaped too.
fn quoted(s: &str) -> String {
    Value::String(s.to_owned())
        .to_string()
        .chars()
        .map(|c| if c.is_control() { format!("\\u{:04x}", u32::from(c)) } else { c.to_string() })
        .collect()
}

/// Whether `s` is safe as a plain scalar: a path, a name, a package spec or an FQCN, starting with
/// a letter, `_` or `/`. Never a word YAML 1.1 (Ansible's) reads as a boolean or null in any case
/// (`yes`, `n`, `Off`, `null`), never one starting with a digit, a sign or a dot, which it may read
/// as a number or a date (`1.10`, `0x10`, `1_000`, `.5`, `2024-01-02`; a mode must stay a
/// string), and nothing with a space, quote, colon or other indicator.
fn plain(s: &str) -> bool {
    let reserved = ["true", "false", "null", "yes", "no", "on", "off", "y", "n"];
    let first = s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '/');
    let safe = |c: char| c.is_ascii_alphanumeric() || "_/.=+@,-".contains(c);
    first && !reserved.contains(&s.to_ascii_lowercase().as_str()) && s.chars().all(safe)
}

/// Appends a structural `line` (a key, a `- `, a scalar on its line) and a newline, with no
/// trailing space (`yamllint`'s `trailing-spaces`); a block scalar's content is written as it is.
fn push(out: &mut String, line: &str) {
    out.push_str(line.trim_end());
    out.push('\n');
}

/// `n` spaces.
fn pad(n: usize) -> String {
    " ".repeat(n)
}

#[cfg(test)]
mod tests;
