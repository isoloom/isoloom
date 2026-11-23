use isoloom_test_support::assert_every;

use super::*;
use crate::syntax::{Code, Kind};

fn problems(yaml: &str) -> Vec<Problem> {
    parse(yaml).expect_err("problems")
}

#[test]
fn a_spec_reads_into_its_types() {
    let spec = parse(
        "version: 1\nname: help-desk\nnetworks:\n  lab: { cidr: 10.44.0.0/24, internet: false }\ninputs:\n  DEMO_SECRET: { description: the secret }\nmachines:\n  web:\n    networks: { lab: 10 }\n    app: { build: build/web }\n    ports: { site: { port: 8000, protocol: http, publish: 8080 } }\n    files: [{ path: /srv/secret.txt, input: DEMO_SECRET, uid: 1000, mode: \"0400\" }]\n",
    )
    .unwrap();
    assert_eq!(spec.name, "help-desk");
    assert!(matches!(spec.machines["web"].kind, Kind::App(_)));
    assert_eq!(spec.spans.find(&FieldPath::new("machines.web.files[0].input")).map(|s| s.line), Some(12));
}

#[test]
fn every_problem_is_reported_at_once_in_file_order() {
    let p = problems(
        "version: 1\nname: t\nmachines:\n  a:\n    os: debian@10\n  b:\n    os: debian@12\n    portz: { web: 80 }\n  c:\n    docker: { image: nginx }\n",
    );
    let found: Vec<(Code, &str, usize)> = p.iter().map(|p| (p.code, p.path.as_str(), p.span.map_or(0, |s| s.line))).collect();
    assert_eq!(
        found,
        [
            (Code::InvalidValue, "machines.a.os", 5),
            (Code::UnknownField, "machines.b.portz", 8),
            (Code::RemovedField, "machines.c.docker", 10),
        ]
    );
    assert_eq!(p[1].fix.as_deref(), Some("did you mean `ports`?"));
    assert!(p[0].reason.contains("11, 12, 13"), "{}", p[0].reason);
}

#[test]
fn missing_and_unknown_top_level_fields() {
    let p = problems("machine:\n  a: {}\n");
    let found: Vec<(Code, &str)> = p.iter().map(|p| (p.code, p.path.as_str())).collect();
    assert!(
        found.contains(&(Code::UnknownField, "machine")) && found.contains(&(Code::MissingField, "version")) && found.contains(&(Code::MissingField, "name")),
        "{found:?}"
    );
    assert_eq!(
        p.iter().find(|p| p.path.as_str() == "machine").and_then(|p| p.fix.as_deref()),
        Some("did you mean `machines`?")
    );
}

#[test]
fn a_conflict_inside_a_field_is_coded() {
    let p = problems("version: 1\nname: t\nmachines:\n  a: { os: debian@12, files: [{ path: /a, source: a, content: b }] }\n");
    assert_eq!((p[0].code, p[0].path.as_str()), (Code::Conflict, "machines.a.files[0]"));
}

#[test]
fn shared_fields_and_clones_fold_in() {
    let spec = parse(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.1.0.0/24 }\ncommon:\n  os: debian@12\ngroups:\n  big: { members: [ws], resources: { cpus: 2 } }\nmachines:\n  ws: { count: 2, networks: { lab: 10 } }\n",
    )
    .unwrap();
    assert_eq!(spec.machines.keys().collect::<Vec<_>>(), ["ws-01", "ws-02"]);
    assert_every!(spec.machines.values(), |m: &&crate::syntax::Machine| m.os.is_some()
        && m.resources.and_then(|r| r.cpus) == Some(2));
    assert_eq!(spec.clones["ws"], ["ws-01", "ws-02"]);
}

#[test]
fn a_yaml_error_is_placed_at_its_line() {
    let p = problems("version: 1\nname: [\n");
    assert_eq!((p[0].code, p[0].span.map(|s| s.line)), (Code::InvalidValue, Some(3)));
}

#[test]
fn machines_are_a_map_or_left_out() {
    let none = parse("version: 1\nname: t\nmachines:\n").unwrap();
    assert!(none.machines.is_empty());
    let wrong = problems("version: 1\nname: t\nmachines: 5\n");
    assert_eq!(
        (wrong[0].code, wrong[0].path.as_str(), wrong[0].reason.as_str()),
        (Code::InvalidValue, "machines", "machines are a map of name to machine")
    );
}

#[test]
fn an_unknown_field_suggests_the_closest_or_lists_them_all() {
    let at = FieldPath::new("machines.web");
    let fix = |field: &str, known: &[&str]| unknown(&at, field, known).fix;
    assert_eq!(fix("portz", &["ports", "files"]).as_deref(), Some("did you mean `ports`?"));
    assert_eq!(fix("zzz", &["ports", "files"]).as_deref(), Some("the fields here: `ports`, `files`"));
    assert_eq!(fix("zzz", &[]), None);
    assert_eq!(unknown(&at, "zzz", &[]).path.as_str(), "machines.web.zzz");
}

#[test]
fn a_typed_problem_beside_an_old_key_is_kept() {
    let p = problems("version: 1\nname: t\nmachines:\n  web:\n    docker: { image: nginx }\n  db:\n    os: debian@10\n");
    let found: Vec<(Code, &str)> = p.iter().map(|p| (p.code, p.path.as_str())).collect();
    assert_eq!(found, [(Code::RemovedField, "machines.web.docker"), (Code::InvalidValue, "machines.db.os")]);
}

#[test]
fn a_name_that_isnt_a_text_is_said_so_not_reported_as_an_empty_name() {
    let top = problems("version: 1\nname: t\n1: x\n");
    assert_eq!((top[0].path.as_str(), top[0].reason.as_str()), ("", "a field's name is a text"));
    let group = problems("version: 1\nname: t\ngroups:\n  1: { members: [] }\n");
    assert_eq!((group[0].path.as_str(), group[0].reason.as_str()), ("groups", "a group's name is a text"));
}

#[test]
fn a_spec_that_isnt_a_mapping_or_a_machine_name_that_isnt_a_text_is_said_so() {
    let list = problems("- version: 1\n");
    assert_eq!(
        (list[0].code, list[0].path.as_str(), list[0].reason.as_str()),
        (Code::InvalidValue, "", "a spec is a mapping of fields (`version`, `name`, `machines`, ...)")
    );
    let numbered = problems("version: 1\nname: t\nmachines:\n  1: { os: debian@12 }\n");
    assert_eq!(
        (numbered[0].code, numbered[0].path.as_str(), numbered[0].reason.as_str()),
        (Code::InvalidValue, "machines", "a machine's name is a text")
    );
}

#[test]
fn a_missing_field_inside_a_value_is_placed_at_that_field() {
    let p = problems("version: 1\nname: t\noverrides:\n  kubernetes: { volumes: { data: {} } }\n");
    assert_eq!(
        (p[0].code, p[0].path.as_str(), p[0].reason.as_str()),
        (Code::MissingField, "overrides.kubernetes.volumes.data.external", "`external` is required here")
    );
}

#[test]
fn tags_read_numbers_and_booleans_as_their_text() {
    let spec = parse("version: 1\nname: t\ntags: { owner: ops, cost-center: 1234, ratio: 0.5, managed: true, offset: -3 }\n").unwrap();
    let tags: Vec<(&str, &str)> = spec.tags.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(
        tags,
        [
            ("owner", "ops"),
            ("cost-center", "1234"),
            ("ratio", "0.5"),
            ("managed", "true"),
            ("offset", "-3")
        ]
    );
    let nested = problems("version: 1\nname: t\ntags: { owner: [a] }\n");
    assert!(nested[0].reason.contains("a tag's value: text, a number or true/false"), "{nested:?}");
}
