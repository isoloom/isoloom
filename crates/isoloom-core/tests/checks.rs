//! Checks: what the spec implies (derived), what the author declares, how both become runner
//! scripts, and what `isoloom validate` says about a check that's off.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

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
    // web's dormant 4444 isn't expected to answer from the user (only once exploited).
    assert_eq!(names(&derived, "user"), ["cache:6379 blocked from user", "web:80 from user"]);
    assert_eq!(names(&derived, "web"), ["cache:6379 from web"]);
    // The cache can't reach the front network (dormant or not), and its network is offline.
    assert_eq!(
        names(&derived, "cache"),
        ["web:80 blocked from cache", "web:4444 blocked from cache", "no internet from cache"]
    );
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
    assert_ne!(names(&derived, "user"), [] as [std::string::String; 0]);
    // A machine can run them, so nothing stands on the networks instead.
    assert!(derived.iter().all(|c| c.position != Position::Networks));
}

/// Windows only: a domain controller on two networks and a workstation, no `checks:` of their
/// own.
const WINDOWS_ONLY: &str = "version: 1\nname: win\nnetworks:\n  corp: { cidr: 10.9.0.0/24 }\n  vault: { cidr: 10.9.1.0/24 }\nmachines:\n  dc: { networks: { corp: 10, vault: 10 }, services: [{ port: 389 }], vm: { os: windows-server-2022, provision: [p.ps1] } }\n  ws: { networks: { corp: 20 }, services: [{ port: 443, http: true, tls: true }], vm: { os: windows-server-2025, provision: [p.ps1] } }\n";

#[test]
fn without_a_machine_to_run_them_derived_checks_stand_on_the_networks() {
    // With an offline network too: nothing stands there to say the internet doesn't answer.
    let spec = parse(&WINDOWS_ONLY.replace("10.9.1.0/24 }", "10.9.1.0/24, internet: false }")).unwrap();
    assert_eq!(validate(&spec), vec![]);
    let derived = checks::derived(&spec);
    // Every service at each of its addresses; nothing blocked or offline to assert from there.
    let names: Vec<&str> = derived.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "dc:389 on corp from the networks",
            "dc:389 on vault from the networks",
            "ws:443 from the networks"
        ]
    );
    assert!(derived.iter().all(|c| c.derived && c.position == Position::Networks && c.wait == 30));
    assert!(
        matches!(&derived[1].probe, Probe::Tcp { host: checks::Host::Machine { name, network }, port: 389, expect: TcpExpect::Open, .. } if name == "dc" && network == "vault")
    );
    assert!(matches!(&derived[2].probe, Probe::Http { url, expect: HttpExpect::Any, .. } if url.https));

    // windows-hello: its page, besides the author's script.
    let plan = checks::plan(&example("windows-hello"));
    let derived: Vec<&str> = plan.iter().filter(|c| c.derived).map(|c| c.name.as_str()).collect();
    assert_eq!(derived, ["web01:80 from the networks"]);
}

#[test]
fn the_controller_runs_the_derived_checks_of_a_windows_only_environment() {
    use isoloom_core::{Target, generate};
    let spec = parse(WINDOWS_ONLY).unwrap();
    let file = |files: &[isoloom_core::GeneratedFile], path: &str| files.iter().find(|f| f.path == path).map(|f| f.contents.clone());

    // Vagrant: a controller VM with the runner (there was none: no playbooks, no checks).
    let files = generate(&spec, Target::Vagrant).unwrap();
    let vf = file(&files, ".isoloom/vagrant/Vagrantfile").unwrap();
    assert!(vf.contains("config.vm.define \"isoloom-controller\""));
    assert!(vf.contains("checks/controller.sh"));
    let runner = file(&files, ".isoloom/vagrant/checks/controller.sh").expect("the controller's runner");
    assert!(runner.contains("_retry 30 _tcp '10.9.1.10' 389"), "{runner}");
    assert!(runner.contains("_http_any 'https://10.9.0.20:443/'"), "{runner}");

    // AWS and Azure (a Windows machine on one network there): a controller, and the runner on it.
    let one_net = parse(&WINDOWS_ONLY.replace("corp: 10, vault: 10", "vault: 10")).unwrap();
    let files = generate(&one_net, Target::CloudVm).unwrap();
    let runner = file(&files, ".isoloom/cloud-vm/checks/networks.sh").expect("the networks' runner");
    assert!(runner.contains("_retry 30 _tcp '10.9.1.10' 389"), "{runner}");
    for (cloud, host) in [
        ("aws", "aws_eip.isoloom_controller.public_ip"),
        ("azure", "azurerm_public_ip.isoloom_controller.ip_address"),
    ] {
        let tf = file(&files, &format!(".isoloom/cloud-vm/{cloud}/main.tf")).unwrap();
        assert!(tf.contains("isoloom_controller"), "{cloud}");
        assert!(
            tf.contains(&format!("{{ position = \"networks\", machine = \"controller\", host = {host}")),
            "{cloud}: {tf}"
        );
    }
}

#[test]
fn hybrid_checks_from_the_controller_when_only_containers_could() {
    use isoloom_core::{Target, generate};
    let runner = |spec: &isoloom_core::Spec, dir: &str, target| {
        generate(spec, target)
            .unwrap()
            .into_iter()
            .find(|f| f.path == format!(".isoloom/{dir}/checks/controller.sh"))
            .map(|f| f.contents)
    };
    // mixed-office: a Windows VM and a Linux container. The container's derived checks don't run
    // on hybrid (the checks run from the VM side), so the controller checks both services.
    let spec = example("mixed-office");
    let hybrid = runner(&spec, "hybrid", Target::Hybrid).expect("the controller's runner");
    assert!(hybrid.contains("pass 'files01:445 from the networks'"), "{hybrid}");
    assert!(hybrid.contains("pass 'intranet:80 from the networks'"), "{hybrid}");
    // On Vagrant the intranet is a Linux VM and runs them: nothing changes there.
    let vagrant = runner(&spec, "vagrant", Target::Vagrant).expect("the controller's runner");
    assert!(!vagrant.contains("from the networks"), "{vagrant}");
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
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ exec: id, from: web, contains: x }}\n")),
        ["checks[0]: `contains` goes with `http` or `tcp`"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ tcp: web:80, send: PING }}\n")),
        ["checks[0]: `send` goes with `tcp` and `contains` (the text the answer must contain)"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ http: http://web/, send: x, contains: y }}\n")),
        ["checks[0]: `send` goes with `tcp` and `contains` (the text the answer must contain)"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ tcp: web:80, contains: x, expect: blocked }}\n")),
        ["checks[0]: a `blocked` check sends nothing to look at: leave out `method`, `headers`, `body`, `send` and `contains`"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ tcp: web:80, method: GET }}\n")),
        ["checks[0]: `method`, `headers` and `body` go with `http`"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ tcp: web:80, contains: \"\" }}\n")),
        ["checks[0].contains: give the text to look for, or leave `contains` out"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ tcp: web:80, send: \"a\\nb\", contains: x }}\n")),
        ["checks[0].send: one line: Isoloom adds the newline"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ tcp: web:80, follow: true }}\n")),
        ["checks[0]: `follow` (redirects) goes with `http`"]
    );
    assert_eq!(
        problems(&format!("{BASE}checks:\n  - {{ http: http://web/, follow: true, expect: blocked }}\n")),
        ["checks[0]: a `blocked` check gets no answer, so no redirect to follow: leave out `follow`"]
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

/// A server on 127.0.0.1 answering each connection with `answer(line read, if `wait_line`)`
/// after `delay_ms`, then holding the connection open a while (as a service does).
#[cfg(unix)]
fn serve(wait_line: bool, delay_ms: u64, answer: fn(&str) -> String) -> u16 {
    use std::io::{BufRead, BufReader, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut c in l.incoming().flatten() {
            std::thread::spawn(move || {
                let mut line = String::new();
                if wait_line {
                    let _ = BufReader::new(c.try_clone().unwrap()).read_line(&mut line);
                }
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                let _ = c.write_all(answer(line.trim_end_matches('\n')).as_bytes());
                std::thread::sleep(std::time::Duration::from_secs(8));
            });
        }
    });
    port
}

#[test]
#[cfg(unix)]
fn tcp_checks_read_a_greeting_and_send_a_line() {
    let banner = serve(false, 0, |_| "SSH-2.0-OpenSSH_9.6\r\n".into());
    let late = serve(false, 1500, |_| "220 mail ESMTP ready\r\n".into());
    let pong = serve(true, 0, |l| if l == "PING" { "+PONG\r\n".into() } else { format!("-ERR unknown '{l}'\r\n") });
    let silent = serve(false, 0, |_| String::new());
    let spec = parse(&format!(
        "{BASE}checks:\n  \
         - {{ name: banner, tcp: '127.0.0.1:{banner}', contains: SSH-2.0 }}\n  \
         - {{ name: late, tcp: '127.0.0.1:{late}', contains: ESMTP, wait: 0 }}\n  \
         - {{ name: ping, tcp: '127.0.0.1:{pong}', send: PING, contains: +PONG }}\n  \
         - {{ name: wrong, tcp: '127.0.0.1:{pong}', send: PING, contains: HELLO, wait: 0 }}\n  \
         - {{ name: silent, tcp: '127.0.0.1:{silent}', contains: x, wait: 0 }}\n"
    ))
    .unwrap();
    assert_eq!(validate(&spec), vec![]);
    let plan: Vec<checks::Resolved> = checks::plan(&spec).into_iter().filter(|c| !c.derived).collect();
    let host = |h: &checks::Host, _: &Position| h.to_string();
    let run = |p: &str| format!("sh {p}");
    let render = checks::Render {
        host: &host,
        script: &run,
        playbook: None,
    };
    let script = checks::script(&Position::Machine("user".into()), &plan.iter().collect::<Vec<_>>(), &render);
    assert!(script.contains(&format!("_retry 30 _tcp_read '127.0.0.1' {pong} 'PING' '+PONG'")), "{script}");
    let out = std::process::Command::new("sh").arg("-c").arg(&script).output().expect("sh runs");
    let lines: Vec<Line> = String::from_utf8_lossy(&out.stdout).lines().filter_map(checks::parse_line).collect();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    for ok in ["banner", "late", "ping"] {
        assert!(lines.contains(&Line::Pass(ok.into())), "{ok}: {text}");
    }
    assert!(
        lines.contains(&Line::Fail(
            "wrong".into(),
            format!("127.0.0.1:{pong} answered \"+PONG \", without the expected text")
        )),
        "{text}"
    );
    assert!(
        lines.contains(&Line::Fail("silent".into(), format!("nothing read from 127.0.0.1:{silent} within 5s"))),
        "{text}"
    );
    assert_eq!(lines.last(), Some(&Line::End(3, 2)), "{text}");
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

/// The runner's sh, with only these tools on its PATH (plus what the runtime itself needs): a
/// runner where curl, or wget, is all there is.
#[cfg(unix)]
fn run_with_only(script: &str, tools: &[(&str, &str)]) -> String {
    let dir = std::env::temp_dir().join(format!("isoloom-path-{}-{}", std::process::id(), tools[0].0));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let which = |t: &str| {
        let out = std::process::Command::new("sh").arg("-c").arg(format!("command -v {t}")).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    for t in ["sh", "sed", "head", "tail", "grep", "mktemp", "rm", "date", "sleep", "cat", "tr"] {
        std::os::unix::fs::symlink(which(t), dir.join(t)).unwrap();
    }
    for (name, target) in tools {
        std::os::unix::fs::symlink(target, dir.join(name)).unwrap();
    }
    let out = std::process::Command::new(dir.join("sh"))
        .arg("-c")
        .arg(script)
        .env("PATH", &dir)
        .output()
        .expect("sh runs");
    let _ = std::fs::remove_dir_all(&dir);
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
#[cfg(unix)]
fn http_checks_follow_redirects_when_asked() {
    // /old redirects to /new (200); /loop redirects to itself for ever.
    let port = serve(true, 0, |l| {
        let path = l.split_whitespace().nth(1).unwrap_or("/");
        match path {
            "/old" => "HTTP/1.1 302 Found\r\nLocation: /new\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            "/loop" => "HTTP/1.1 302 Found\r\nLocation: /loop\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            // A login form: a POST is answered with a redirect to a page that takes GET only.
            "/login" if l.starts_with("POST ") => "HTTP/1.1 302 Found\r\nLocation: /home\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            "/home" if !l.starts_with("GET ") => "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            _ => "HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nwelcome\n".into(),
        }
    });
    let u = format!("http://127.0.0.1:{port}");
    let spec = parse(&format!(
        "{BASE}checks:\n  \
         - {{ name: first, http: '{u}/old', expect: 302, wait: 0 }}\n  \
         - {{ name: unfollowed, http: '{u}/old', wait: 0 }}\n  \
         - {{ name: followed, http: '{u}/old', follow: true, wait: 0 }}\n  \
         - {{ name: looping, http: '{u}/loop', follow: true, wait: 0 }}\n  \
         - {{ name: followed body, http: '{u}/old', follow: true, contains: welcome, wait: 0 }}\n  \
         - {{ name: login, http: '{u}/login', method: post, body: 'user=a', follow: true, contains: welcome, wait: 0 }}\n  \
         - {{ name: empty post, http: '{u}/login', method: POST, follow: true, wait: 0 }}\n  \
         - {{ name: unfollowed post, http: '{u}/login', method: POST, body: 'user=a', expect: 302, wait: 0 }}\n"
    ))
    .unwrap();
    assert_eq!(validate(&spec), vec![]);
    let plan: Vec<checks::Resolved> = checks::plan(&spec).into_iter().filter(|c| !c.derived).collect();
    assert!(matches!(&plan[2].probe, Probe::Http { request, .. } if request.follow));
    let host = |h: &checks::Host, _: &Position| h.to_string();
    let run = |p: &str| format!("sh {p}");
    let render = checks::Render {
        host: &host,
        script: &run,
        playbook: None,
    };
    let position = Position::Machine("user".into());
    let script = checks::script(&position, &plan.iter().collect::<Vec<_>>(), &render);
    assert!(script.contains(&format!("_retry 0 _http_is '{u}/old' 200 follow;")), "{script}");
    assert!(script.contains(&format!("-L --max-redirs 5 '{u}/old'")), "{script}");
    let expect = |text: &str, with_body: bool| {
        let lines: Vec<Line> = text.lines().filter_map(checks::parse_line).collect();
        for ok in ["first", "followed"] {
            assert!(lines.contains(&Line::Pass(ok.into())), "{ok}: {text}");
        }
        assert!(
            lines.contains(&Line::Fail("unfollowed".into(), format!("expected HTTP 200 from {u}/old, got 302"))),
            "{text}"
        );
        assert!(
            lines.contains(&Line::Fail(
                "looping".into(),
                format!("expected HTTP 200 from {u}/loop (after redirects), got 302")
            )),
            "{text}"
        );
        if with_body {
            // A POST answered with a redirect goes on as a GET, as in a browser.
            for ok in ["followed body", "login", "empty post", "unfollowed post"] {
                assert!(lines.contains(&Line::Pass(ok.into())), "{ok}: {text}");
            }
        }
    };
    let which = |t: &str| {
        let out = std::process::Command::new("sh").arg("-c").arg(format!("command -v {t}")).output().unwrap();
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|p| !p.is_empty())
    };
    // curl, else GNU wget, else busybox's: the same answers (wget follows whatever it's told).
    if let Some(curl) = which("curl") {
        expect(&run_with_only(&script, &[("curl", &curl)]), true);
    }
    assert!(script.contains(&format!("-L --max-redirs 5 --data-binary 'user=a' '{u}/login'")), "{script}");
    assert!(script.contains(&format!("-L --max-redirs 5 -d '' '{u}/login'")), "{script}");
    assert!(script.contains(&format!("-X 'POST' --data-binary 'user=a' '{u}/login'")), "{script}");
    let plain: Vec<&checks::Resolved> = plan
        .iter()
        .filter(|c| matches!(&c.probe, Probe::Http { request, .. } if request.method.is_none() && request.contains.is_none()))
        .collect();
    let plain = checks::script(&position, &plain, &render);
    if let Some(wget) = which("wget") {
        expect(&run_with_only(&plain, &[("wget", &wget)]), false);
    }
    if let Some(busybox) = which("busybox") {
        expect(&run_with_only(&plain, &[("wget", &busybox)]), false);
    }
}
