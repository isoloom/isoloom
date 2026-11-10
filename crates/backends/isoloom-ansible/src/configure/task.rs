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
                json!({ "dest": "/etc/app.conf", "mode": "0640", "src": jinja("{{ isoloom_project }}/a") }),
            )
            .into_value();
        let text = yaml(vec![task]);
        assert!(text.contains("dest: /etc/app.conf\n"), "a path is plain: {text}");
        assert!(text.contains("mode: \"0640\"\n"), "a mode stays a string: {text}");
        assert!(text.contains("src: \"{{ isoloom_project }}/a\"\n"), "a jinja value is quoted: {text}");
    }

    #[test]
    fn a_string_holding_jinja_is_unsafe_unless_isoloom_wrote_it() {
        let task = Task::new("echo {{ x }}")
            .module(
                "ansible.builtin.shell",
                json!({ "cmd": "echo {{ lookup('pipe', 'id') }}", "argv": ["{% if 1 %}", "{# c #}", "a}b{"] }),
            )
            .set("vars", json!({ "own": jinja("{{ isoloom_project }}") }))
            .set("content", json!("a {{ b }}\nc\n"))
            .into_value();
        let text = yaml(vec![task]);
        assert!(text.contains("- name: !unsafe \"Echo {{ x }}\"\n"), "{text}");
        assert!(text.contains("cmd: !unsafe \"echo {{ lookup('pipe', 'id') }}\"\n"), "{text}");
        assert!(text.contains("- !unsafe \"{% if 1 %}\"\n"), "{text}");
        assert!(text.contains("- !unsafe \"{# c #}\"\n"), "{text}");
        assert!(text.contains("- \"a}b{\"\n"), "a lone brace is no Jinja: {text}");
        assert!(text.contains("content: !unsafe |2\n"), "a block scalar is tagged too: {text}");
        assert!(
            text.contains("own: \"{{ isoloom_project }}\"\n"),
            "Isoloom's own Jinja stays a template: {text}"
        );
    }

    #[test]
    fn a_multiline_string_is_a_block_scalar() {
        let task = Task::new("x")
            .module("ansible.builtin.copy", json!({ "content": "a\nb\n", "dest": "/f" }))
            .into_value();
        let text = yaml(vec![task]);
        assert!(text.contains("content: |2\n      a\n      b\n"), "{text}");
    }

    /// `text` read back by a YAML parser, its `!unsafe` tags dropped, as JSON.
    fn read_back(text: &str) -> Value {
        fn untag(v: serde_yaml_ng::Value) -> serde_yaml_ng::Value {
            match v {
                serde_yaml_ng::Value::Tagged(t) => untag(t.value),
                serde_yaml_ng::Value::Sequence(items) => serde_yaml_ng::Value::Sequence(items.into_iter().map(untag).collect()),
                serde_yaml_ng::Value::Mapping(m) => serde_yaml_ng::Value::Mapping(m.into_iter().map(|(k, item)| (k, untag(item))).collect()),
                other => other,
            }
        }
        let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"));
        serde_json::to_value(untag(yaml)).unwrap()
    }

    /// A task whose `content` is `s`, as YAML.
    fn content(s: &str) -> (Value, String) {
        let task = Task::new("x")
            .module("ansible.builtin.copy", json!({ "content": s, "dest": "/f" }))
            .into_value();
        let text = yaml(vec![task.clone()]);
        (Value::Array(vec![task]), text)
    }

    #[test]
    fn a_block_scalar_reads_back_as_the_text_it_holds() {
        for s in [
            "  indented first\nthen not\n",
            "a  \nb\n",
            "a\n\n",
            "a\nb",
            "a\n\n\nb\n\n\n",
            "\n",
            "\n\n",
            "\tx\ny\n",
            "a\r\nb\n",
            "# not a comment\n- not a list\n",
            "{{ x }}\n  y\n",
        ] {
            let (value, text) = content(s);
            assert_eq!(read_back(&text), value, "{s:?} as {text}");
            assert!(!text.lines().any(|l| l.ends_with(' ')), "no trailing space in the file: {text}");
        }
        assert!(content("  a\nb\n").1.contains("content: |2\n        a\n      b\n"), "{}", content("  a\nb\n").1);
    }

    proptest::proptest! {
        #[test]
        fn any_text_reads_back_as_itself(s in "[ a-z{}#:\\-\"'\t\n\r\u{1b}\u{7f}\u{85}é]{0,40}") {
            let (value, text) = content(&s);
            proptest::prop_assert_eq!(read_back(&text), value, "{}", text);
        }
    }

    #[test]
    fn an_empty_list_is_valid_yaml() {
        assert_eq!(yaml(Vec::new()), "[]\n");
    }
}
