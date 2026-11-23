//! Checks: what the spec implies (derived), what the author declares, how both become runner
//! scripts, and what `isoloom validate` says about a check that's off.

use isoloom_plan::checks::{self, HttpExpect, Position, Probe, TcpExpect};
use isoloom_spec::parse;

use crate::testing::{Outcome, check_line, example, lab};

#[cfg(unix)]
mod hostile;
#[cfg(unix)]
mod probes;
mod runner;

fn names(plan: &[checks::Resolved], from: &str) -> Vec<String> {
    plan.iter()
        .filter(|c| c.position == Position::Machine(from.into()))
        .map(|c| c.name.clone())
        .collect()
}

#[test]
fn derived_checks_follow_reach_and_shared_networks() -> Outcome {
    // segmented: access -> front (all ports), front -> back on 6379 only.
    let derived = checks::derived(&example("segmented")?.spec());
    // web's dormant 4444 isn't expected to answer from the user (only once something starts it).
    assert_eq!(names(&derived, "user"), ["cache:6379 blocked from user", "web:80 from user"]);
    assert_eq!(names(&derived, "web"), ["cache:6379 from web"]);
    // The cache can't reach the front network (dormant or not), and its network is offline.
    assert_eq!(
        names(&derived, "cache"),
        ["web:80 blocked from cache", "web:4444 blocked from cache", "no internet from cache"]
    );
    let blocked = derived.iter().find(|c| c.name == "cache:6379 blocked from user").ok_or("no blocked check")?;
    assert!(matches!(
        &blocked.probe,
        Probe::Tcp {
            expect: TcpExpect::Blocked,
            port: 6379,
            ..
        }
    ));
    assert_eq!(blocked.wait, 0);
    let web = derived.iter().find(|c| c.name == "web:80 from user").ok_or("no web check")?;
    assert!(matches!(&web.probe, Probe::Http { expect: HttpExpect::Any, .. }));
    assert_eq!(web.wait, 30);
    Ok(())
}

#[test]
fn a_closed_path_to_a_multihomed_machine_is_not_asserted() -> Outcome {
    // edge-firewall: fw is on outside (.2), dmz (.1) and lan (.1). From web (dmz), fw's dmz address
    // answers; its outside and lan addresses would be reached through fw itself, so nothing is
    // promised about them. From the user (outside), the lan cache is unreachable everywhere.
    let derived = checks::derived(&example("edge-firewall")?.spec());
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
    Ok(())
}

#[test]
fn windows_machines_are_not_vantage_points() -> Outcome {
    let derived = checks::derived(&example("corp-ad-basics")?.spec());
    isoloom_test_support::assert_every!(&derived, |c: &&checks::Resolved| {
        c.position != Position::Machine("dc01".into()) && c.position != Position::Machine("ws-01".into())
    });
    // The access machine (Linux) still is.
    assert_ne!(names(&derived, "user"), Vec::<String>::new());
    // A machine can run them, so nothing stands on the networks instead.
    assert!(!derived.iter().any(|c| c.position == Position::Networks));
    Ok(())
}

/// Windows only: a domain controller on two networks and a workstation, no `checks:` of their
/// own.
const WINDOWS_ONLY: &str = "version: 1\nname: win\nnetworks:\n  corp: { cidr: 10.9.0.0/24 }\n  vault: { cidr: 10.9.1.0/24 }\nmachines:\n  dc: { os: windows-server@2022, networks: { corp: 10, vault: 10 }, ports: { ldap: 389 }, app: { steps: [{ run: p.ps1 }] } }\n  ws: { os: windows-server@2025, networks: { corp: 20 }, ports: { web: { port: 443, protocol: https } }, app: { steps: [{ run: p.ps1 }] } }\n";

#[test]
fn without_a_machine_to_run_them_derived_checks_stand_on_the_networks() -> Outcome {
    // With an offline network too: nothing stands there to say the internet doesn't answer.
    let spec = lab(&WINDOWS_ONLY.replace("10.9.1.0/24 }", "10.9.1.0/24, internet: false }"))?;
    assert_eq!(spec.problems(), vec![]);
    let derived = checks::derived(&spec.spec());
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
    isoloom_test_support::assert_every!(&derived, |c: &&checks::Resolved| c.derived && c.position == Position::Networks && c.wait == 30);
    assert!(
        matches!(&derived[1].probe, Probe::Tcp { host: checks::Host::Machine { name, network }, port: 389, expect: TcpExpect::Open, .. } if name == "dc" && network == "vault")
    );
    assert!(matches!(&derived[2].probe, Probe::Http { url, expect: HttpExpect::Any, .. } if url.scheme == isoloom_spec::probe::Scheme::Https));

    // windows-hello: its page, besides the author's script.
    let plan = checks::plan(&example("windows-hello")?.spec());
    let page: Vec<&str> = plan.iter().filter(|c| c.derived).map(|c| c.name.as_str()).collect();
    assert_eq!(page, ["web01:80 from the networks"]);
    Ok(())
}

#[test]
fn the_controller_runs_the_derived_checks_of_a_windows_only_environment() -> Outcome {
    use isoloom_spec::Target;
    use isoloom_targets::generate;
    let spec = lab(WINDOWS_ONLY)?;
    let file = |files: &[isoloom_targets::GeneratedFile], path: &str| files.iter().find(|f| f.path == path).map(|f| f.contents.clone());

    // Vagrant: a controller VM with the runner (there was none: no playbooks, no checks).
    let vagrant = generate(&spec, Target::Vagrant)?;
    let vf = isoloom_test_support::ruby_blocks(&file(&vagrant, ".isoloom/vagrant/Vagrantfile").ok_or("no Vagrantfile")?);
    let installs = crate::testing::define(&vf, "isoloom-controller").every_statement();
    assert!(installs.iter().any(|s| s.contains("checks/isoloom-controller.sh")), "{installs:?}");
    let controller = file(&vagrant, ".isoloom/vagrant/checks/isoloom-controller.sh").ok_or("the controller's runner")?;
    let ldap = check_line(&controller, "dc:389 on vault from the networks")?;
    assert!(ldap.contains("_retry 30 _tcp '10.9.1.10' 389"), "{ldap}");
    let web = check_line(&controller, "ws:443 from the networks")?;
    assert!(web.contains("_http_any 'https://10.9.0.20:443/'"), "{web}");

    // AWS and Azure (a Windows machine on one network there): a controller, and the runner on it.
    let one_net = lab(&WINDOWS_ONLY.replace("corp: 10, vault: 10", "vault: 10"))?;
    let files = generate(&one_net, Target::CloudVm)?;
    let runner = file(&files, ".isoloom/cloud-vm/checks/isoloom-networks.sh").ok_or("the networks' runner")?;
    assert!(
        check_line(&runner, "dc:389 from the networks")?.contains("_retry 30 _tcp '10.9.1.10' 389"),
        "{runner}"
    );
    for (cloud, host) in [
        ("aws", "aws_eip.isoloom_controller.public_ip"),
        ("azure", "azurerm_public_ip.isoloom_controller.ip_address"),
    ] {
        let tf = file(&files, &format!(".isoloom/cloud-vm/{cloud}/main.tf")).ok_or("no module")?;
        // The networks' runner runs on the controller, reached at its public address.
        let entries = runner_entries(&tf);
        let entry = ("isoloom-networks".to_owned(), "isoloom-controller".to_owned(), host.to_owned());
        assert!(entries.contains(&entry), "{cloud}: {entries:?}");
    }
    Ok(())
}

/// The entries of a cloud module's `checks` output: each runner's position, the machine it runs
/// on, and the host Terraform reaches it at.
fn runner_entries(tf: &str) -> Vec<(String, String, String)> {
    use isoloom_test_support::hcl::{Expression, ObjectKey};
    let body = isoloom_test_support::hcl_body(tf);
    let outputs = isoloom_test_support::hcl_blocks(&body, "output", &["checks"]);
    let Some(Expression::Array(items)) = outputs.first().and_then(|o| isoloom_test_support::hcl_attribute(o.body(), "value")) else {
        return Vec::new();
    };
    let text = |e: &Expression| match e {
        Expression::String(s) => s.clone(),
        other => isoloom_test_support::hcl::format::to_string(other).unwrap_or_default(),
    };
    items
        .iter()
        .filter_map(|item| match item {
            Expression::Object(o) => {
                let field = |k: &str| {
                    o.iter()
                        .find(|(key, _)| matches!(key, ObjectKey::Identifier(id) if id.as_str() == k))
                        .map(|(_, v)| text(v))
                        .unwrap_or_default()
                };
                Some((field("position"), field("machine"), field("host")))
            }
            _ => None,
        })
        .collect()
}

const BASE: &str = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  web: { networks: { lab: 10 }, ports: { web: { port: 80, protocol: http } }, app: { image: nginx } }\n  user: { os: debian@12, access: true, networks: { lab: 20 } }\n";

/// The runner script of `checks` at `position`: hosts as written, scripts run with `sh`.
fn runner(position: &Position, checks: &[&checks::Resolved]) -> String {
    let host = |h: &checks::Host, _: &Position| h.to_string();
    let run = |p: &str| format!("sh {p}");
    let render = isoloom_render::checks::Render {
        host: &host,
        script: Some(&run),
        playbook: None,
    };
    isoloom_render::checks::script(position, checks, &render)
}

fn problems(yaml: &str) -> Outcome<Vec<String>> {
    Ok(crate::testing::problems(&lab(yaml)?))
}

#[test]
fn declared_checks_resolve_to_positions_probes_and_defaults() -> Outcome {
    let spec = lab(&format!(
        "{BASE}checks:\n  - checks/a.sh\n  - {{ http: http://web/ }}\n  - {{ name: sealed, from: web, tcp: db:5432, expect: blocked }}\n  - {{ from: web, exec: id, expect: root, wait: 9 }}\n"
    ))?;
    assert_eq!(spec.problems(), vec![]);
    let plan = checks::plan(&spec.spec());
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
    Ok(())
}

#[test]
fn derived_checks_follow_the_declared_ones() -> Outcome {
    let spec = lab(&format!("{BASE}checks:\n  - {{ http: http://web/ }}\n"))?.spec();
    let plan = checks::plan(&spec);
    let first_derived = plan.iter().position(|c| c.derived).ok_or("no derived check")?;
    let (declared, derived) = plan.split_at(first_derived);
    isoloom_test_support::assert_every!(declared, |c: &&checks::Resolved| !c.derived);
    isoloom_test_support::assert_every!(derived, |c: &&checks::Resolved| c.derived);
    assert!(plan.iter().any(|c| c.derived && c.name == "web:80 from user"));
    Ok(())
}

#[test]
fn a_check_that_is_off_names_its_field() -> Outcome {
    // Each check (a YAML list item), and the problem it gets.
    let cases = [
        (
            "{ name: x }",
            "checks[0]: say what to check with exactly one of `http`, `tcp`, `exec` or `script`",
        ),
        ("{ http: web }", "checks[0].http: `web` isn't a URL like http://web:8080/path"),
        ("{ tcp: web }", "checks[0].tcp: `web` isn't `host:port`, like cache:6379"),
        ("{ from: nope, http: http://web/ }", "checks[0].from: no machine named `nope`"),
        ("{ exec: id }", "checks[0]: `exec` runs inside a machine: say which with `from`"),
        ("{ tcp: web:80, expect: 200 }", "checks[0].expect: for `tcp`: `open` or `blocked`"),
        (
            "{ http: http://web/, expect: open }",
            "checks[0].expect: for `http`: a status code (100-599), `any` or `blocked`",
        ),
        ("{ exec: id, from: web, contains: x }", "checks[0]: `contains` goes with `http` or `tcp`"),
        (
            "{ tcp: web:80, send: PING }",
            "checks[0]: `send` goes with `tcp` and `contains` (the text the answer must contain)",
        ),
        (
            "{ http: http://web/, send: x, contains: y }",
            "checks[0]: `send` goes with `tcp` and `contains` (the text the answer must contain)",
        ),
        (
            "{ tcp: web:80, contains: x, expect: blocked }",
            "checks[0]: a `blocked` check sends nothing to look at: leave out `method`, `headers`, `body`, `send` and `contains`",
        ),
        ("{ tcp: web:80, method: GET }", "checks[0]: `method`, `headers` and `body` go with `http`"),
        (
            "{ tcp: web:80, contains: \"\" }",
            "checks[0].contains: give the text to look for, or leave `contains` out",
        ),
        (
            "{ tcp: web:80, send: \"a\\nb\", contains: x }",
            "checks[0].send: one line: Isoloom adds the newline",
        ),
        ("{ tcp: web:80, follow: true }", "checks[0]: `follow` (redirects) goes with `http`"),
        (
            "{ http: http://web/, follow: true, expect: blocked }",
            "checks[0]: a `blocked` check gets no answer, so no redirect to follow: leave out `follow`",
        ),
    ];
    for (check, problem) in cases {
        assert_eq!(problems(&format!("{BASE}checks:\n  - {check}\n"))?, [problem], "{check}");
    }
    // A typo in a declared check is reported as such, not as "no variant matched".
    let err = parse(&format!("{BASE}checks:\n  - {{ htp: http://web/ }}\n")).unwrap_err().to_string();
    assert!(
        err.contains("error[unknown-field] checks[0].htp") && err.contains("did you mean `http`?"),
        "{err}"
    );
    Ok(())
}
