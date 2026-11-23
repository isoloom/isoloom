use isoloom_spec::probe::Url;
use proptest::prelude::*;

use isoloom_plan::checks::{HttpExpect, HttpRequest, Position, Probe, Resolved, TcpExchange, TcpExpect};

use super::{Host, Render, looks_expanded, script, single_quoted, with_directives};

proptest! {
    #[test]
    fn a_quoted_text_is_read_back_whole(text in "\\PC{0,30}") {
        prop_assert_eq!(single_quoted(&isoloom_spec::shell::quote(&text)).join("'"), text);
    }
}

#[test]
fn single_quoted_parts_are_read_as_sh_reads_them() {
    let exec = r"_exec 'sh -c '\''echo $x'\''' 'ok'";
    assert_eq!(single_quoted(exec), ["sh -c ", "echo $x", "", "ok"]);
    assert_eq!(single_quoted(r#"fail 'n' "it's $out" \'$y"#), ["n"], "double quotes and escapes hold none");
    assert_eq!(single_quoted("echo 'unclosed $x"), ["unclosed $x"]);
}

#[test]
fn what_shellcheck_reads_as_an_expansion_is_found() {
    for text in ["$x", "a ${b}", "$(c)", "$1", "`d`", "x``y`z`"] {
        assert!(looks_expanded(text), "{text}");
    }
    for text in ["%s", "$", "a $ b", "``", "one ` only", "$-x"] {
        assert!(!looks_expanded(text), "{text}");
    }
}

#[test]
fn a_line_quoting_an_expansion_gets_its_directive_and_others_none() {
    let lines = "if _retry 0 _exec 'echo $HOME' ''; then pass 'n'; fi\n  pass 'plain'\n";
    assert_eq!(with_directives(lines), format!("{DIRECTIVE}{lines}"));
    let indented = with_directives("  pass '`id`'\n");
    assert_eq!(indented, format!("  {DIRECTIVE}  pass '`id`'\n"));
}

#[test]
fn a_quoted_value_over_several_lines_gets_one_directive_before_it_and_nothing_inside_it() {
    // The spec's text, quoted, holds line breaks, a `$(…)` and a backticked word.
    let lines = "if _retry 0 _req 200 'a\n$(id)\n`id`\nEOF' 'http://web/'; then pass 'n'; fi\npass 'plain'\n";
    assert_eq!(with_directives(lines), format!("{DIRECTIVE}{lines}"));
    // A line inside the value is never read as a statement of its own.
    let two = "pass 'x\n$(a)'\npass 'y\n$(b)'\n";
    assert_eq!(with_directives(two), format!("{DIRECTIVE}pass 'x\n$(a)'\n{DIRECTIVE}pass 'y\n$(b)'\n"));
    // A comment's apostrophe opens no quote.
    let commented = "# the check's own line\npass '$(c)'\n";
    assert_eq!(with_directives(commented), format!("# the check's own line\n{DIRECTIVE}pass '$(c)'\n"));
}

/// The line before a check that single-quotes what looks like an expansion.
const DIRECTIVE: &str = "# shellcheck disable=SC2016 # the check's own text, quoted as written\n";

/// A machine's host as `<name>.<network>`, a literal as written.
fn host(h: &Host, _: &Position) -> String {
    match h {
        Host::Machine { name, network } => format!("{name}.{network}"),
        Host::Literal(l) => l.clone(),
    }
}

fn run_script(path: &str) -> String {
    format!("cd /opt/isoloom && sh {path}")
}

fn run_playbook(path: &str) -> String {
    format!("ansible-playbook {path}")
}

/// The runner of `checks` from `user`, the targets that run playbooks (`playbooks`) or not.
fn runner(checks: &[Resolved], playbooks: bool) -> String {
    let playbook: &dyn Fn(&str) -> String = &run_playbook;
    let r = Render {
        host: &host,
        script: Some(&run_script),
        playbook: playbooks.then_some(playbook),
    };
    script(&Position::Machine("user".into()), &checks.iter().collect::<Vec<_>>(), &r)
}

/// One check named `n`, written by the author, waiting `wait` seconds.
fn check(n: &str, wait: u32, probe: Probe) -> Resolved {
    Resolved {
        name: n.into(),
        position: Position::Machine("user".into()),
        probe,
        wait,
        derived: false,
    }
}

fn http(url: &str, expect: HttpExpect, request: HttpRequest) -> Probe {
    Probe::Http {
        url: Url::parse(url).expect("the test's URL parses"),
        expect,
        request,
    }
}

fn tcp(expect: TcpExpect, exchange: TcpExchange) -> Probe {
    Probe::Tcp {
        host: Host::Machine {
            name: "db".into(),
            network: "lan".into(),
        },
        port: 5432,
        expect,
        exchange,
    }
}

/// The lines of check `c` in its runner: between the helpers and the summary.
fn lines(c: Resolved) -> String {
    let all = runner(&[c], true);
    let after = all.split_once(&super::runtime()).map_or("", |(_, rest)| rest);
    after
        .strip_prefix('\n')
        .and_then(|rest| rest.split_once("\necho \"isoloom-check: END"))
        .map(|(check, _)| check.to_owned())
        .unwrap_or_default()
}

#[test]
fn a_runner_reports_each_check_and_fails_when_one_failed() {
    let mut derived = check("db:5432 from user", 30, tcp(TcpExpect::Open, TcpExchange::default()));
    derived.derived = true;
    let script = check("checks/a.sh", 0, Probe::Script { path: "checks/a.sh".into() });
    let sh = runner(&[script.clone(), derived.clone()], true);
    // The header, the counters and the helpers, then each check, then the summary.
    let runtime = super::runtime();
    let (head, checks) = sh.split_once(&runtime).expect("the helpers");
    assert_eq!(
        head,
        format!(
            "#!/bin/sh\n{}# The environment's checks, from user: each prints a PASS or FAIL line.\n{}",
            crate::header("#"),
            super::counters()
        )
    );
    assert_eq!(
        checks,
        format!(
            "\n{}\nif [ \"${{ISOLOOM_DERIVED:-1}}\" != 0 ]; then\n{}fi\n\necho \"isoloom-check: END $passed passed, $failed failed\"\n[ \"$failed\" -eq 0 ]\n",
            lines(script),
            lines(Resolved { derived: false, ..derived })
        ),
        "only the derived check can be skipped; the runner fails when one failed"
    );
    // The helpers' redirect limit and answer count are filled in.
    let filled = [
        "if [ -n \"$2\" ]; then set -- \"$1\" -L --max-redirs 5; else set -- \"$1\"; fi",
        "if [ -n \"$2\" ]; then _n=6; else _n=1; fi",
    ];
    for line in filled {
        assert!(runtime.lines().any(|l| l.trim() == line), "no `{line}` in {runtime}");
    }
    assert!(!runtime.contains("@MAX_REDIRS@") && !runtime.contains("@ANSWERS@"), "{runtime}");
}

/// An `exec` check named `n` running `command`.
#[cfg(unix)]
fn exec(n: &str, command: &str) -> Resolved {
    check(
        n,
        0,
        Probe::Exec {
            command: command.into(),
            expect: None,
        },
    )
}

#[cfg(unix)]
#[test]
fn what_a_runner_prints_reads_back_as_its_checks_lines() {
    use isoloom_spec::check_line::CheckLine;

    let sh = runner(&[exec("it's up", "true"), exec("db ready", "echo refused; false")], false);
    let out = std::process::Command::new("sh").arg("-c").arg(&sh).output().expect("sh runs");
    assert_eq!(out.status.code(), Some(1), "one failed");
    let read: Vec<CheckLine> = String::from_utf8_lossy(&out.stdout).lines().filter_map(CheckLine::parse).collect();
    assert_eq!(
        read,
        [
            CheckLine::Pass { name: "it's up".into() },
            CheckLine::Fail {
                name: "db ready".into(),
                reason: "refused".into()
            },
            CheckLine::End { passed: 1, failed: 1 },
        ],
        "{sh}"
    );
}

#[cfg(unix)]
#[test]
fn a_runner_with_every_kind_of_check_is_valid_sh() {
    let checks = [
        check("a", 30, http("http://web/x?$y", HttpExpect::Status(200), HttpRequest::default())),
        check("b", 0, http("https://web:8443/", HttpExpect::Blocked, HttpRequest::default())),
        check(
            "c",
            30,
            http(
                "http://web/",
                HttpExpect::Any,
                HttpRequest {
                    method: Some("post".into()),
                    body: Some("it's".into()),
                    ..HttpRequest::default()
                },
            ),
        ),
        check(
            "d",
            30,
            tcp(
                TcpExpect::Open,
                TcpExchange {
                    send: Some("PING".into()),
                    contains: Some("PONG".into()),
                },
            ),
        ),
        check(
            "e",
            0,
            Probe::Exec {
                command: "id -u".into(),
                expect: Some("0".into()),
            },
        ),
        check("f", 0, Probe::Playbook { path: "checks/f.yml".into() }),
    ];
    let sh = runner(&checks, false);
    let out = std::process::Command::new("sh").arg("-n").arg("-c").arg(&sh).output().expect("sh runs");
    assert!(out.status.success(), "{}\n{sh}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn an_http_check_compares_the_status_says_what_came_and_follows_when_asked() {
    let plain = lines(check("site", 30, http("http://web/a$b", HttpExpect::Status(200), HttpRequest::default())));
    // `$b` single-quoted reads as an expansion to shellcheck: the line says it is meant.
    assert_eq!(
        plain,
        DIRECTIVE.to_owned()
            + "if _retry 30 _http_is 'http://web/a$b' 200; then pass 'site'; else fail 'site' \"expected HTTP 200 from http://web/a\\$b, got $(_http 'http://web/a$b')\"; fi\n"
    );
    let follow = HttpRequest {
        follow: true,
        ..HttpRequest::default()
    };
    let followed = lines(check("site", 30, http("http://web/", HttpExpect::Status(302), follow)));
    assert_eq!(
        followed,
        "if _retry 30 _http_is 'http://web/' 302 follow; then pass 'site'; else fail 'site' \"expected HTTP 302 from http://web/ (after redirects), got $(_http 'http://web/' follow)\"; fi\n"
    );
    let any = lines(check("any", 30, http("http://web/", HttpExpect::Any, HttpRequest::default())));
    assert_eq!(
        any,
        "if _retry 30 _http_any 'http://web/'; then pass 'any'; else fail 'any' \"nothing answers at http://web/\"; fi\n"
    );
    let blocked = lines(check(
        "no",
        0,
        http(
            "http://web/",
            HttpExpect::Blocked,
            HttpRequest {
                method: Some("post".into()),
                ..HttpRequest::default()
            },
        ),
    ));
    assert_eq!(
        blocked,
        "c=$(_http 'http://web/'); if [ \"$c\" = 000 ]; then pass 'no'; else fail 'no' \"http://web/ answered (HTTP $c); it should be blocked\"; fi\n"
    );
}

#[test]
fn a_request_sends_its_method_headers_and_body_with_curl() {
    let request = |method: Option<&str>, body: Option<&str>, follow: bool| HttpRequest {
        method: method.map(Into::into),
        headers: vec![("X-A".into(), "1 2".into())],
        body: body.map(Into::into),
        contains: Some("ok".into()),
        follow,
    };
    let req = |expect, r| lines(check("r", 30, http("http://web/", expect, r)));
    assert_eq!(
        req(HttpExpect::Status(201), request(Some("put"), Some("x"), false)),
        "if _retry 30 _req 201 'ok' -X 'PUT' -H 'X-A: 1 2' --data-binary 'x' 'http://web/'; then pass 'r'; else fail 'r' \"$out\"; fi\n"
    );
    // Each request's curl arguments: a method curl has its own option for when it follows
    // redirects (a POST with no body, a HEAD), else `-X`.
    for (method, body, follow, args) in [
        (Some("post"), None, true, "-L --max-redirs 5 -d '' -H 'X-A: 1 2'"),
        (Some("post"), Some("x"), true, "-L --max-redirs 5 -H 'X-A: 1 2' --data-binary 'x'"),
        (Some("head"), None, true, "-L --max-redirs 5 -I -H 'X-A: 1 2'"),
        (Some("head"), None, false, "-X 'HEAD' -H 'X-A: 1 2'"),
        (Some("post"), None, false, "-X 'POST' -H 'X-A: 1 2'"),
        (None, None, false, "-H 'X-A: 1 2'"),
    ] {
        assert_eq!(
            req(HttpExpect::Any, request(method, body, follow)),
            format!("if _retry 30 _req any 'ok' {args} 'http://web/'; then pass 'r'; else fail 'r' \"$out\"; fi\n"),
            "{method:?}, body {body:?}, follow {follow}"
        );
    }
}

#[test]
fn a_tcp_check_connects_or_reads_back_and_a_blocked_one_must_not_connect() {
    assert_eq!(
        lines(check("db", 30, tcp(TcpExpect::Open, TcpExchange::default()))),
        "if _retry 30 _tcp 'db.lan' 5432; then pass 'db'; else fail 'db' \"nothing listens at db.lan:5432\"; fi\n"
    );
    assert_eq!(
        lines(check(
            "db",
            0,
            tcp(
                TcpExpect::Blocked,
                TcpExchange {
                    send: None,
                    contains: Some("x".into())
                }
            )
        )),
        "if _tcp 'db.lan' 5432; then fail 'db' \"db.lan:5432 answered; it should be blocked\"; else pass 'db'; fi\n"
    );
    assert_eq!(
        lines(check(
            "db",
            30,
            tcp(
                TcpExpect::Open,
                TcpExchange {
                    send: Some("PING".into()),
                    contains: Some("PONG".into())
                }
            )
        )),
        "if _retry 30 _tcp_read 'db.lan' 5432 'PING' 'PONG'; then pass 'db'; else fail 'db' \"$out\"; fi\n"
    );
    assert_eq!(
        lines(check(
            "db",
            30,
            tcp(
                TcpExpect::Open,
                TcpExchange {
                    send: None,
                    contains: Some("+OK".into())
                }
            )
        )),
        "if _retry 30 _tcp_read 'db.lan' 5432 '' '+OK'; then pass 'db'; else fail 'db' \"$out\"; fi\n",
        "a greeting read without sending"
    );
}

#[test]
fn exec_scripts_and_playbooks_run_where_the_target_says() {
    assert_eq!(
        lines(check(
            "uid",
            0,
            Probe::Exec {
                command: "id -u".into(),
                expect: Some("0".into())
            }
        )),
        "if _retry 0 _exec 'id -u' '0'; then pass 'uid'; else fail 'uid' \"$(printf '%s' \"$out\" | tail -n 1)\"; fi\n"
    );
    assert_eq!(
        lines(check(
            "ok",
            0,
            Probe::Exec {
                command: "true".into(),
                expect: None
            }
        )),
        "if _retry 0 _exec 'true' ''; then pass 'ok'; else fail 'ok' \"$(printf '%s' \"$out\" | tail -n 1)\"; fi\n"
    );
    assert_eq!(
        lines(check("checks/a.sh", 10, Probe::Script { path: "checks/a.sh".into() })),
        "echo '== checks/a.sh'\nif _retry 10 sh -c 'cd /opt/isoloom && sh checks/a.sh'; then pass 'checks/a.sh'; else fail 'checks/a.sh' \"checks/a.sh exited non-zero\"; fi\n"
    );
    let playbook = check("checks/b.yml", 0, Probe::Playbook { path: "checks/b.yml".into() });
    assert_eq!(
        lines(playbook.clone()),
        "echo '== checks/b.yml'\nif _retry 0 sh -c 'ansible-playbook checks/b.yml'; then pass 'checks/b.yml'; else fail 'checks/b.yml' \"checks/b.yml failed\"; fi\n"
    );
    // Where the target runs neither, the check fails saying so.
    let nowhere = Render {
        host: &host,
        script: None,
        playbook: None,
    };
    let script = check("checks/a.sh", 0, Probe::Script { path: "checks/a.sh".into() });
    let sh = super::script(&Position::Machine("user".into()), &[&script, &playbook], &nowhere);
    let (_, checks) = sh.split_once(&super::runtime()).expect("the helpers");
    assert!(
        checks.starts_with(
            "\nfail 'checks/a.sh' \"project scripts don't run from this position\"\n\nfail 'checks/b.yml' \"Ansible checks don't run from this position\"\n\necho"
        ),
        "{sh}"
    );
}
