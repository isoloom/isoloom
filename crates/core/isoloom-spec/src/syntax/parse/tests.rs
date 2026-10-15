use super::*;
use crate::syntax::{Code, Kind};

fn problems(yaml: &str) -> Vec<Problem> {
    parse(yaml).expect_err("problems")
}

#[test]
fn a_spec_reads_into_its_types() {
    let spec = parse(
        "version: 1\nname: help-desk\nnetworks:\n  lab: { cidr: 10.44.0.0/24, internet: false }\ninputs:\n  FLAG: { description: the flag }\nmachines:\n  web:\n    networks: { lab: 10 }\n    app: { build: build/web }\n    ports: { site: { port: 8000, protocol: http, publish: 8080 } }\n    files: [{ path: /srv/flag.txt, input: FLAG, uid: 1000, mode: \"0400\" }]\n",
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
    assert!(spec.machines.values().all(|m| m.os.is_some() && m.resources.and_then(|r| r.cpus) == Some(2)));
    assert_eq!(spec.clones["ws"], ["ws-01", "ws-02"]);
}

#[test]
fn yaml_errors_have_their_line() {
    let p = problems("version: 1\nname: [\n");
    assert_eq!(p[0].code, Code::InvalidValue);
    assert!(p[0].span.is_some());
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
