use serde_json::json;

use super::*;

#[test]
fn a_task_capitalizes_its_name_and_indents_its_sequences() {
    let task = Task::new("packages nginx")
        .module("ansible.builtin.apt", json!({ "name": ["nginx"], "update_cache": true }))
        .notify(&["nginx".to_owned()])
        .into_value();
    let text = yaml(vec![task]);
    // A sequence is indented under its key (yamllint's indent-sequences).
    assert_eq!(
        text,
        "- name: \"Packages nginx\"\n  ansible.builtin.apt:\n    name:\n      - nginx\n    update_cache: true\n  notify:\n    - \"Isoloom restart nginx\"\n"
    );
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
    // A path is plain, a mode stays a string, a Jinja value is quoted.
    assert_eq!(
        text,
        "- name: X\n  ansible.builtin.copy:\n    dest: /etc/app.conf\n    mode: \"0640\"\n    src: \"{{ isoloom_project }}/a\"\n"
    );
}

#[test]
fn a_string_yaml_would_read_as_a_number_a_boolean_null_or_a_date_is_quoted() {
    for text in [
        "1.10",
        "0x10",
        "1_000",
        "0o17",
        ".5",
        "1e3",
        "-1",
        "+1",
        "2024-01-02",
        "y",
        "N",
        "Off",
        "NULL",
        "yes",
        "~",
    ] {
        assert_eq!(scalar(text), quoted(text), "{text}");
        let read: serde_yaml_ng::Value = serde_yaml_ng::from_str(&scalar(text)).expect("YAML");
        assert_eq!(read.as_str(), Some(text), "{text} reads back as itself");
    }
    for text in ["nginx", "/etc/app.conf", "python3-pip=1.2", "ansible.posix", "_x"] {
        assert_eq!(scalar(text), text, "{text} stays plain");
    }
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
    // The spec's Jinja is `!unsafe` (a block scalar too; a lone brace is none); Isoloom's own
    // stays a template.
    assert_eq!(
        text,
        "- name: !unsafe \"Echo {{ x }}\"\n  ansible.builtin.shell:\n    cmd: !unsafe \"echo {{ lookup('pipe', 'id') }}\"\n    argv:\n      - !unsafe \"{% if 1 %}\"\n      - !unsafe \"{# c #}\"\n      - \"a}b{\"\n  vars:\n    own: \"{{ isoloom_project }}\"\n  content: !unsafe |2\n    a {{ b }}\n    c\n"
    );
}

#[test]
fn a_multiline_string_is_a_block_scalar() {
    let task = Task::new("x")
        .module("ansible.builtin.copy", json!({ "content": "a\nb\n", "dest": "/f" }))
        .into_value();
    let text = yaml(vec![task]);
    assert_eq!(text, "- name: X\n  ansible.builtin.copy:\n    content: |2\n      a\n      b\n    dest: /f\n");
}

/// `text` read back by a YAML parser, its `!unsafe` tags dropped, as JSON.
fn read_back(text: &str) -> Value {
    serde_json::to_value(crate::testing::untagged(text)).unwrap()
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
    // A first line that starts with spaces: the indentation indicator says where the text starts.
    assert_eq!(
        content("  a\nb\n").1,
        "- name: X\n  ansible.builtin.copy:\n    content: |2\n        a\n      b\n    dest: /f\n"
    );
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
