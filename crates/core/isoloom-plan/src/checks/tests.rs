use isoloom_spec::model::{Check, Declared, Expect, Spec};

use super::*;
use crate::testing;

/// A lab with an access machine (`user`), a web server and its checks: `checks` is the YAML
/// list, `checks/` scripts and playbooks included.
fn spec(checks: &str) -> Spec {
    testing::spec(&format!(
        "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  user: {{ access: true, networks: {{ lan: 5 }} }}\n  web: {{ networks: {{ lan: 10 }}, ports: {{ redis: 6379 }}, app: {{ image: redis }} }}\nchecks: {checks}\n"
    ))
}

fn declared(d: &Declared) -> Option<Resolved> {
    super::declared(d, &Position::Machine("user".into()))
}

#[test]
fn an_http_check_expects_200_and_waits_30_seconds_by_default() {
    let c = declared(&Declared {
        http: Some("http://web/".into()),
        ..Declared::default()
    })
    .expect("resolved");
    assert_eq!((c.name.as_str(), c.wait, c.derived), ("http://web/ from user", 30, false));
    assert!(matches!(c.probe, Probe::Http { expect: HttpExpect::Status(200), ref request, .. } if *request == HttpRequest::default()));
}

#[test]
fn an_http_check_takes_a_status_any_or_blocked() {
    let http = |expect: Expect| {
        declared(&Declared {
            http: Some("http://web/".into()),
            expect: Some(expect),
            ..Declared::default()
        })
    };
    let probe = |c: Option<Resolved>| c.map(|c| (c.probe, c.wait, c.name));
    assert!(matches!(
        probe(http(Expect::Status(404))),
        Some((
            Probe::Http {
                expect: HttpExpect::Status(404),
                ..
            },
            30,
            _
        ))
    ));
    assert!(matches!(
        probe(http(Expect::Text("any".into()))),
        Some((Probe::Http { expect: HttpExpect::Any, .. }, 30, _))
    ));
    let blocked = probe(http(Expect::Text("blocked".into())));
    assert!(matches!(&blocked, Some((Probe::Http { expect: HttpExpect::Blocked, .. }, 0, name)) if name == "http://web/ blocked from user"));
    assert_eq!(http(Expect::Text("open".into())), None);
    assert_eq!(
        declared(&Declared {
            http: Some("ftp://web/".into()),
            ..Declared::default()
        }),
        None
    );
}

#[test]
fn an_http_request_carries_its_method_headers_body_and_text() {
    let c = declared(&Declared {
        name: Some("login".into()),
        http: Some("http://web/login".into()),
        method: Some("post".into()),
        headers: [("X-A".to_owned(), "1".to_owned())].into_iter().collect(),
        body: Some("u=a".into()),
        contains: Some("welcome".into()),
        follow: true,
        wait: Some(5),
        ..Declared::default()
    })
    .expect("resolved");
    assert_eq!((c.name.as_str(), c.wait), ("login", 5));
    let Probe::Http { request, .. } = c.probe else { panic!("an http probe") };
    let want = HttpRequest {
        method: Some("post".into()),
        headers: vec![("X-A".into(), "1".into())],
        body: Some("u=a".into()),
        contains: Some("welcome".into()),
        follow: true,
    };
    assert_eq!(request, want);
    let named = declared(&Declared {
        http: Some("http://web/login".into()),
        method: Some("post".into()),
        ..Declared::default()
    });
    assert_eq!(named.map(|resolved| resolved.name), Some("POST http://web/login from user".to_owned()));
}

#[test]
fn a_tcp_check_needs_a_port_and_expects_open_or_blocked() {
    let tcp = |target: &str, expect: Option<&str>| {
        declared(&Declared {
            tcp: Some(target.into()),
            expect: expect.map(|e| Expect::Text(e.into())),
            send: Some("PING".into()),
            contains: Some("PONG".into()),
            ..Declared::default()
        })
    };
    let open = tcp("web:6379", None).expect("resolved");
    assert_eq!((open.name.as_str(), open.wait), ("web:6379 from user", 30));
    let Probe::Tcp { host, port, expect, exchange } = open.probe else {
        panic!("a tcp probe")
    };
    assert_eq!((host, port, expect), (Host::Literal("web".into()), 6379, TcpExpect::Open));
    assert_eq!(
        exchange,
        TcpExchange {
            send: Some("PING".into()),
            contains: Some("PONG".into())
        }
    );
    assert!(tcp("web:6379", Some("open")).is_some_and(|c| matches!(c.probe, Probe::Tcp { expect: TcpExpect::Open, .. })));
    let blocked = tcp("web:22", Some("blocked")).expect("resolved");
    assert_eq!((blocked.name.as_str(), blocked.wait), ("web:22 blocked from user", 0));
    assert_eq!(tcp("web:0", None), None);
    assert_eq!(tcp("web", None), None);
    assert_eq!(tcp("web:22", Some("closed")), None);
}

#[test]
fn an_exec_check_looks_for_text_and_a_script_runs_once() {
    let exec = |expect: Option<Expect>| {
        declared(&Declared {
            from: Some("web".into()),
            exec: Some("id -u".into()),
            expect,
            ..Declared::default()
        })
    };
    let c = exec(Some(Expect::Text("0".into()))).expect("resolved");
    assert_eq!((c.name.as_str(), c.position, c.wait), ("id -u from web", Position::Machine("web".into()), 0));
    assert_eq!(
        c.probe,
        Probe::Exec {
            command: "id -u".into(),
            expect: Some("0".into())
        }
    );
    assert!(exec(None).is_some_and(|resolved| resolved.probe
        == Probe::Exec {
            command: "id -u".into(),
            expect: None
        }));
    assert_eq!(exec(Some(Expect::Status(0))), None);
    let script = super::declared(
        &Declared {
            script: Some("checks/a.sh".into()),
            ..Declared::default()
        },
        &Position::Networks,
    )
    .expect("resolved");
    assert_eq!(
        (script.name.as_str(), script.probe),
        ("checks/a.sh", Probe::Script { path: "checks/a.sh".into() })
    );
    assert_eq!(super::declared(&Declared::default(), &Position::Networks), None);
}

#[test]
fn the_authors_checks_come_first_from_the_access_machine_then_the_derived_ones() {
    let s = spec("[checks/a.sh, checks/b.yml, { tcp: 'web:22', expect: blocked }]");
    let planned = plan(&s);
    let names: Vec<(&str, bool)> = planned.iter().map(|c| (c.name.as_str(), c.derived)).collect();
    assert_eq!(
        names,
        [
            ("checks/a.sh", false),
            ("checks/b.yml", false),
            ("web:22 blocked from user", false),
            ("web:6379 from user", true)
        ]
    );
    assert_eq!(planned[0].probe, Probe::Script { path: "checks/a.sh".into() });
    assert_eq!(planned[1].probe, Probe::Playbook { path: "checks/b.yml".into() });
    assert!(planned.iter().all(|c| c.position == Position::Machine("user".into())));
    assert_eq!(default_position(&s), Position::Machine("user".into()));
    assert!(matches!(&s.checks[0], Check::Script(_)));
}

#[test]
fn without_an_access_machine_checks_run_from_the_networks() {
    let s = testing::spec(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: windows-server@2022, ports: { site: 80 } }\n",
    );
    assert_eq!(default_position(&s), Position::Networks);
    let planned = plan(&s);
    assert_eq!(planned.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["web:80 from the networks"]);
    assert_eq!(
        (Position::Networks.id(), Position::Networks.label()),
        ("networks", "from the environment's networks".to_owned())
    );
    let web = Position::Machine("web".into());
    assert_eq!((web.id(), web.label()), ("web", "from web".to_owned()));
}

#[test]
fn checks_group_by_position_the_default_first_then_machines_in_order() {
    let s = spec("[{ exec: id, from: web }, checks/a.sh]");
    let planned = plan(&s);
    let groups: Vec<(Position, Vec<&str>)> = by_position(&s, &planned)
        .into_iter()
        .map(|(p, cs)| (p, cs.iter().map(|c| c.name.as_str()).collect()))
        .collect();
    assert_eq!(
        groups,
        [
            (Position::Machine("user".into()), vec!["checks/a.sh", "web:6379 from user"]),
            (Position::Machine("web".into()), vec!["id from web"]),
        ]
    );
}

#[test]
fn a_request_is_plain_with_nothing_beyond_a_get_and_its_status() {
    assert!(HttpRequest::default().is_plain());
    assert!(
        HttpRequest {
            follow: true,
            ..HttpRequest::default()
        }
        .is_plain(),
        "following redirects is still a GET"
    );
    let one_more = [
        HttpRequest {
            method: Some("POST".into()),
            ..HttpRequest::default()
        },
        HttpRequest {
            headers: vec![("A".into(), "1".into())],
            ..HttpRequest::default()
        },
        HttpRequest {
            body: Some(String::new()),
            ..HttpRequest::default()
        },
        HttpRequest {
            contains: Some("ok".into()),
            ..HttpRequest::default()
        },
    ];
    for r in one_more {
        assert!(!r.is_plain(), "{r:?}");
    }
}

#[test]
fn checks_from_the_networks_have_a_group_beside_the_access_machines() {
    // A Windows access machine runs no checks: what the spec implies is checked from the networks.
    let s = testing::spec(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  user: { access: true, os: windows@11, networks: { lan: 5 } }\n  web: { networks: { lan: 10 }, os: windows-server@2022, ports: { site: 80 } }\nchecks: [checks/a.sh]\n",
    );
    let planned = plan(&s);
    let groups: Vec<(Position, usize)> = by_position(&s, &planned).into_iter().map(|(p, cs)| (p, cs.len())).collect();
    assert_eq!(groups, [(Position::Machine("user".into()), 1), (Position::Networks, 1)]);
}
