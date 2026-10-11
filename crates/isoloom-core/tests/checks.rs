//! Checks: what the spec implies (derived), what the author declares, how both become runner
//! scripts, and what `isoloom validate` says about a check that's off.

use std::path::Path;

use isoloom_core::checks::{self, HttpExpect, Line, Position, Probe, TcpExpect};
use isoloom_core::{load, parse, validate};

fn example(name: &str) -> isoloom_core::Spec {
    load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name)).expect("example parses")
}

fn names(plan: &[checks::Resolved], from: &str) -> Vec<String> {
    plan.iter()
        .filter(|c| c.position == Position::Machine(from.into()))
        .map(|c| c.name.clone())
        .collect()
}

#[test]
fn derived_checks_follow_reach_and_shared_networks() {
    // segmented: access -> front (all ports), front -> back on 6379 only.
    let derived = checks::derived(&example("segmented"));
    assert_eq!(names(&derived, "user"), ["cache:6379 blocked from user", "web:80 from user"]);
    assert_eq!(names(&derived, "web"), ["cache:6379 from web"]);
    // The cache can't reach the front network, and its network is offline.
    assert_eq!(names(&derived, "cache"), ["web:80 blocked from cache", "no internet from cache"]);
    let blocked = derived.iter().find(|c| c.name == "cache:6379 blocked from user").unwrap();
    assert!(matches!(
        &blocked.probe,
        Probe::Tcp {
            expect: TcpExpect::Blocked,
            port: 6379,
            ..
        }
    ));
    assert_eq!(blocked.wait, 0);
    let web = derived.iter().find(|c| c.name == "web:80 from user").unwrap();
    assert!(matches!(&web.probe, Probe::Http { expect: HttpExpect::Any, .. }));
    assert_eq!(web.wait, 30);
}

#[test]
fn a_closed_path_to_a_multihomed_machine_is_not_asserted() {
    // edge-firewall: fw is on outside (.2), dmz (.1) and lan (.1). From web (dmz), fw's dmz address
    // answers; its outside and lan addresses would be reached through fw itself, so nothing is
    // promised about them. From the user (outside), the lan cache is unreachable everywhere.
    let derived = checks::derived(&example("edge-firewall"));
    assert_eq!(names(&derived, "web"), ["fw:8080 on dmz from web", "cache:6379 from web"]);
    assert_eq!(
        names(&derived, "user"),
        ["fw:8080 on outside from user", "web:80 from user", "cache:6379 blocked from user"]
    );
    assert_eq!(
        names(&derived, "cache"),
        ["fw:8080 on lan from cache", "web:80 blocked from cache", "no internet from cache"]
    );
    // The firewall sees everything it is on; its outside network is online.
    assert_eq!(names(&derived, "fw"), ["web:80 from fw", "cache:6379 from fw"]);
}

#[test]
fn windows_machines_are_not_vantage_points() {
    let derived = checks::derived(&example("corp-ad-basics"));
    assert!(derived.iter().all(|c| c.position != Position::Machine("dc01".into())));
    assert!(derived.iter().all(|c| c.position != Position::Machine("ws-01".into())));
    // The access machine (no implementation) still is: the runner supplies it.
    assert!(!names(&derived, "user").is_empty());
}

const BASE: &str = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  web: { networks: { lab: 10 }, services: [{ port: 80, http: true }], docker: { image: nginx } }\n  user: { access: true, networks: { lab: 20 } }\n";

fn problems(yaml: &str) -> Vec<String> {
    validate(&parse(yaml).expect("parses")).into_iter().map(|p| p.to_string()).collect()
}

#[test]
fn declared_checks_resolve_to_positions_probes_and_defaults() {
    let spec = parse(&format!(
        "{BASE}checks:\n  - checks/a.sh\n  - {{ http: http://web/ }}\n  - {{ name: sealed, from: web, tcp: db:5432, expect: blocked }}\n  - {{ from: web, exec: id, expect: root, wait: 9 }}\n"
    ))
    .unwrap();
    assert_eq!(validate(&spec), vec![]);
    let plan = checks::plan(&spec);
    let own: Vec<&checks::Resolved> = plan.iter().filter(|c| !c.derived).collect();
    // A script and a check without `from` stand where the access machine stands.
    assert_eq!(own[0].position, Position::Machine("user".into()));
    assert!(matches!(&own[0].probe, Probe::Script { path } if path == "checks/a.sh"));
    assert_eq!(own[1].name, "http://web/ from user");
    assert!(matches!(
        &own[1].probe,
        Probe::Http {
            expect: HttpExpect::Status(200),
            ..
        }
    ));
    assert_eq!(own[1].wait, 30);
    assert_eq!((own[2].name.as_str(), &own[2].position), ("sealed", &Position::Machine("web".into())));
    assert!(matches!(&own[3].probe, Probe::Exec { command, expect: Some(e) } if command == "id" && e == "root"));
    assert_eq!(own[3].wait, 9);
    // Then the derived ones.
    assert!(plan.iter().any(|c| c.derived && c.name == "web:80 from user"));
}

#[test]
fn a_check_that_is_off_names_its_field() {
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ name: x }}\n")),
        ["checks[0]: say what to check with exactly one of `http`, `tcp`, `exec` or `script`"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ http: web }}\n")),
        ["checks[0].http: `web` isn't a URL like http://web:8080/path"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ tcp: web }}\n")),
        ["checks[0].tcp: `web` isn't `host:port`, like cache:6379"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ from: nope, http: http://web/ }}\n")),
        ["checks[0].from: no machine named `nope`"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ exec: id }}\n")),
        ["checks[0]: `exec` runs inside a machine: say which with `from`"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ tcp: web:80, expect: 200 }}\n")),
        ["checks[0].expect: for `tcp`: `open` or `blocked`"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ http: http://web/, expect: open }}\n")),
        ["checks[0].expect: for `http`: a status code (100-599), `any` or `blocked`"]
    );
    // A typo in a declared check is reported as such, not as "no variant matched".
    let err = parse(&format!("{BASE}checks:\n  - {{ htp: http://web/ }}\n")).unwrap_err().to_string();
    assert!(err.contains("unknown field `htp`"), "{err}");
}

#[test]
fn runner_scripts_print_lines_the_cli_reads_back() {
    let spec = example("segmented");
    let plan = checks::plan(&spec);
    let groups = checks::by_position(&spec, &plan);
    // The default position (the access machine) comes first, then machines in spec order.
    let order: Vec<&str> = groups.iter().map(|(p, _)| p.id()).collect();
    assert_eq!(order, ["user", "cache", "web"]);
    let host = |h: &checks::Host, _: &Position| h.to_string();
    let run = |p: &str| format!("sh {p}");
    let render = checks::Render {
        host: &host,
        script: &run,
        playbook: None,
    };
    let script = checks::script(&groups[0].0, &groups[0].1, &render);
    assert!(script.starts_with("#!/bin/sh\n"));
    assert!(script.contains("_retry 60 _http_is 'http://web/' 200"), "{script}");
    assert!(script.contains("if _tcp 'cache' 6379; then fail"), "{script}");
    // Derived checks can be switched off at run time.
    assert!(script.contains("if [ \"${ISOLOOM_DERIVED:-1}\" != 0 ]; then\nif _tcp 'cache' 6379"), "{script}");
    assert!(script.ends_with("echo \"isoloom-check: END $passed passed, $failed failed\"\n[ \"$failed\" -eq 0 ]\n"));

    assert_eq!(
        checks::parse_line("isoloom-check: PASS web:80 from user"),
        Some(Line::Pass("web:80 from user".into()))
    );
    assert_eq!(
        checks::parse_line("    web: isoloom-check: FAIL cache:6379 from web: nothing listens at 10.61.20.20:6379"),
        Some(Line::Fail("cache:6379 from web".into(), "nothing listens at 10.61.20.20:6379".into()))
    );
    assert_eq!(checks::parse_line("isoloom-check: END 3 passed, 1 failed"), Some(Line::End(3, 1)));
    assert_eq!(checks::parse_line("== checks/x.sh"), None);
}

#[test]
fn exec_checks_run_inside_the_machine_on_docker_and_kubernetes() {
    let spec = example("arm-vm");
    assert_eq!(
        isoloom_core::refusal(&spec, isoloom_core::Target::Vagrant),
        None,
        "a VM runs exec checks on the machine itself"
    );
    let with_docker = parse(&format!("{BASE}checks:\n  - {{ from: web, exec: id, expect: uid }}\n")).unwrap();
    assert_eq!(isoloom_core::refusal(&with_docker, isoloom_core::Target::Docker), None);
    assert_eq!(isoloom_core::refusal(&with_docker, isoloom_core::Target::Kubernetes), None);
    // From where no container of its own stands, there's nothing to run it inside (`validate`
    // says so first, and `refusal` reports the spec's problem).
    let from_user = parse(&format!("{BASE}checks:\n  - {{ exec: id }}\n")).unwrap();
    let why = isoloom_core::refusal(&from_user, isoloom_core::Target::Docker).expect("refused");
    assert!(why.contains("say which with `from`"), "{why}");
    // Its own runner, piped into the machine; the runner beside it doesn't run it.
    let files = isoloom_core::generate(&with_docker, isoloom_core::Target::Docker).unwrap();
    let exec = files
        .iter()
        .find(|f| f.path.ends_with("docker/checks/exec-web.sh"))
        .expect("an exec runner for web");
    assert!(exec.contents.contains("_exec 'id' 'uid'"));
    let beside = files.iter().find(|f| f.path.ends_with("docker/checks/web.sh")).unwrap();
    assert!(!beside.contents.contains("_exec 'id'"));
}

/// A TLS service's derived check goes over HTTPS.
#[test]
fn a_tls_service_is_probed_over_https() {
    let spec = parse(&BASE.replace("http: true }", "http: true, tls: true }")).unwrap();
    let plan = checks::plan(&spec);
    let derived = plan
        .iter()
        .find(|c| c.derived && matches!(&c.probe, checks::Probe::Http { .. }))
        .expect("a derived http check");
    let checks::Probe::Http { url, .. } = &derived.probe else { unreachable!() };
    assert!(url.https);
}
