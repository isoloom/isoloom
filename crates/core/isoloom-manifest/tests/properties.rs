//! Property tests for the template grammar, the path rules and the program rule: what parses
//! writes back unchanged, what is refused is refused for the right reason, and the generated JSON
//! Schema accepts exactly what the types accept.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::sync::OnceLock;

use isoloom_manifest::{Part, PathTemplate, Placeholder, Program, RelPath, RunVar, Template};
use proptest::prelude::*;
use serde_json::{Value, json};

/// A validator for one definition of the generated schema.
fn validator(def: &str) -> jsonschema::Validator {
    let schema = isoloom_manifest::schema();
    jsonschema::validator_for(&json!({ "$defs": schema["$defs"], "$ref": format!("#/$defs/{def}") })).unwrap()
}

fn accepts(def: &'static str, text: &str) -> bool {
    static VALIDATORS: OnceLock<Vec<(&'static str, jsonschema::Validator)>> = OnceLock::new();
    let all = VALIDATORS.get_or_init(|| ["Template", "PathTemplate", "RelPath", "Program"].map(|d| (d, validator(d))).into());
    all.iter().find(|(d, _)| *d == def).unwrap().1.is_valid(&Value::String(text.to_string()))
}

/// Strings made of the pieces the grammar cares about, valid or not.
fn grammar_soup() -> impl Strategy<Value = String> {
    let pieces = prop_oneof![
        Just("$"),
        Just("$$"),
        Just("${"),
        Just("}"),
        Just("run.name"),
        Just("run.state_dir"),
        Just("run.title"),
        Just("params.CTF_FLAG"),
        Just("params.1X"),
        Just("outputs.web_portal"),
        Just("published.web.80"),
        Just("published.dns.53.udp"),
        Just("published.a.b.65535"),
        Just("published.web.0"),
        Just("published.web.65536"),
        Just("secrets.FLAG"),
        Just(" secrets.x "),
        Just("machine"),
        Just("value"),
        Just("/"),
        Just("\\"),
        Just(".."),
        Just("."),
        Just("C:"),
        Just("web"),
        Just(" "),
        Just("\n"),
        Just("{{.ServerVersion}}"),
    ];
    prop::collection::vec(pieces, 0..8).prop_map(|p| p.concat())
}

fn placeholder() -> impl Strategy<Value = Placeholder> {
    prop_oneof![
        Just(Placeholder::Run(RunVar::Name)),
        Just(Placeholder::Run(RunVar::StateDir)),
        Just(Placeholder::Machine),
        Just(Placeholder::Value),
        "[A-Z_][A-Z0-9_]{0,8}".prop_map(|n| Placeholder::Param(isoloom_manifest::EnvName::parse(&n).unwrap())),
        "[a-z][a-z0-9_]{0,8}".prop_map(|n| Placeholder::Output(isoloom_manifest::SnakeName::parse(&n).unwrap())),
        ("[a-z][a-z0-9.-]{0,6}", "[a-z][a-z0-9-]{0,30}").prop_map(|(m, port)| Placeholder::Published {
            machine: isoloom_manifest::Name::parse(&m).unwrap(),
            port: isoloom_manifest::Name::parse(&port).unwrap(),
        }),
    ]
}

fn part() -> impl Strategy<Value = Part> {
    prop_oneof![any::<String>().prop_map(Part::Text), placeholder().prop_map(Part::Placeholder)]
}

/// A path segment that breaks no rule.
fn segment() -> impl Strategy<Value = String> {
    "[A-Za-z0-9_][A-Za-z0-9_. -]{0,8}".prop_filter("not `..`", |s| s != ".." && !s.contains(':'))
}

const SHELLS: [&str; 8] = ["sh", "bash", "cmd", "powershell", "pwsh", "env", "sudo", "python3"];

proptest! {
    #[test]
    fn a_template_writes_back_to_itself(parts in prop::collection::vec(part(), 0..6)) {
        let t = Template::from_parts(parts);
        let written = t.to_string();
        prop_assert_eq!(Template::parse(&written).unwrap(), t);
        prop_assert!(accepts("Template", &written), "the schema refuses {written:?}");
    }

    #[test]
    fn text_without_a_dollar_is_its_own_template(text in "[^$]*") {
        let t = Template::parse(&text).unwrap();
        prop_assert_eq!(t.placeholders().count(), 0);
        prop_assert_eq!(t.to_string(), text);
    }

    #[test]
    fn a_secret_placeholder_is_refused_anywhere(before in "[^$]*", name in "[A-Za-z_.]*", after in "[^$]*") {
        let err = Template::parse(&format!("{before}${{secrets.{name}}}{after}")).unwrap_err();
        prop_assert_eq!(err.code(), "secret_placeholder");
    }

    #[test]
    fn a_lone_dollar_is_refused(before in "[^$]*", next in "[^${]", after in "[^$]*") {
        let err = Template::parse(&format!("{before}${next}{after}")).unwrap_err();
        prop_assert_eq!(err.code(), "lone_dollar");
    }

    #[test]
    fn the_schema_and_the_types_agree_on_templates(text in grammar_soup()) {
        prop_assert_eq!(Template::parse(&text).is_ok(), accepts("Template", &text), "{:?}", text);
        prop_assert_eq!(PathTemplate::parse(&text).is_ok(), accepts("PathTemplate", &text), "{:?}", text);
        prop_assert_eq!(RelPath::parse(&text).is_ok(), accepts("RelPath", &text), "{:?}", text);
    }

    #[test]
    fn segments_joined_by_slashes_are_a_relative_path(segments in prop::collection::vec(segment(), 1..5)) {
        let path = segments.join("/");
        prop_assert_eq!(RelPath::parse(&path).unwrap().to_string(), path.clone());
        prop_assert!(accepts("RelPath", &path));
        prop_assert_eq!(RelPath::parse(&format!("/{path}")).unwrap_err().code(), "absolute_path");
        prop_assert_eq!(RelPath::parse(&format!("C:/{path}")).unwrap_err().code(), "absolute_path");
        prop_assert_eq!(RelPath::parse(&format!("{path}\\x")).unwrap_err().code(), "backslash");
        prop_assert_eq!(RelPath::parse(&format!("{path}/../x")).unwrap_err().code(), "parent_path");
        prop_assert_eq!(RelPath::parse(&format!("../{path}")).unwrap_err().code(), "parent_path");
    }

    #[test]
    fn a_path_in_the_state_folder_stays_a_path(segments in prop::collection::vec(segment(), 1..4)) {
        let rel = RelPath::parse(&segments.join("/")).unwrap();
        let in_state = PathTemplate::in_state_dir(&rel);
        prop_assert_eq!(PathTemplate::parse(&in_state.to_string()).unwrap(), in_state);
    }

    #[test]
    fn shells_are_refused_in_any_case(index in 0..SHELLS.len(), upper in prop::collection::vec(any::<bool>(), 10), exe in any::<bool>()) {
        let name: String = SHELLS[index].chars().zip(upper.iter().cycle()).map(|(c, u)| if *u { c.to_ascii_uppercase() } else { c }).collect();
        let name = if exe { format!("{name}.EXE") } else { name };
        prop_assert_eq!(Program::parse(&name).unwrap_err().code(), "shell_program");
        prop_assert!(!accepts("Program", &name));
    }

    #[test]
    fn the_schema_and_the_types_agree_on_programs(name in "[A-Za-z./_-][A-Za-z0-9_./ -]{0,10}") {
        prop_assert_eq!(Program::parse(&name).is_ok(), accepts("Program", &name), "{:?}", name);
    }
}
