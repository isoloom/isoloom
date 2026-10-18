//! The runner scripts the checks become: the lines they print, a machine's own checks run
//! inside it, and a TLS service probed over HTTPS.

use isoloom_plan::checks::{self, Line};

use super::{BASE, runner};
use crate::testing::{Outcome, example, lab};

#[test]
fn runner_scripts_print_lines_the_cli_reads_back() -> Outcome {
    let spec = example("segmented")?.spec();
    let plan = checks::plan(&spec);
    let groups = checks::by_position(&spec, &plan);
    // The default position (the access machine) comes first, then machines in spec order.
    let order: Vec<&str> = groups.iter().map(|(p, _)| p.id()).collect();
    assert_eq!(order, ["user", "cache", "web"]);
    let script = runner(&groups[0].0, &groups[0].1);
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
    Ok(())
}

#[test]
fn exec_checks_run_inside_the_machine_on_docker_and_kubernetes() -> Outcome {
    let spec = example("arm-vm")?;
    assert_eq!(
        isoloom_targets::refusal(&spec, isoloom_spec::Target::Vagrant),
        None,
        "a VM runs exec checks on the machine itself"
    );
    let with_docker = lab(&format!("{BASE}checks:\n  - {{ from: web, exec: id, expect: uid }}\n"))?;
    assert_eq!(isoloom_targets::refusal(&with_docker, isoloom_spec::Target::Docker), None);
    assert_eq!(isoloom_targets::refusal(&with_docker, isoloom_spec::Target::Kubernetes), None);
    // From where no container of its own stands, there's nothing to run it inside (`validate`
    // says so first, and `refusal` reports the spec's problem).
    let from_user = lab(&format!("{BASE}checks:\n  - {{ exec: id }}\n"))?;
    let why = isoloom_targets::refusal(&from_user, isoloom_spec::Target::Docker).expect("refused");
    assert!(why.contains("say which with `from`"), "{why}");
    // Its own runner, piped into the machine; the runner beside it doesn't run it.
    let files = isoloom_targets::generate(&with_docker, isoloom_spec::Target::Docker).unwrap();
    let exec = files
        .iter()
        .find(|f| f.path.ends_with("docker/checks/exec-web.sh"))
        .expect("an exec runner for web");
    assert!(exec.contents.contains("_exec 'id' 'uid'"));
    let beside = files.iter().find(|f| f.path.ends_with("docker/checks/web.sh")).unwrap();
    assert!(!beside.contents.contains("_exec 'id'"));
    Ok(())
}

/// A TLS service's derived check goes over HTTPS.
#[test]
fn a_tls_service_is_probed_over_https() -> Outcome {
    let spec = lab(&BASE.replace("protocol: http }", "protocol: https }"))?.spec();
    let plan = checks::plan(&spec);
    let derived = plan
        .iter()
        .find(|c| c.derived && matches!(&c.probe, checks::Probe::Http { .. }))
        .expect("a derived http check");
    let checks::Probe::Http { url, .. } = &derived.probe else { unreachable!() };
    assert!(url.https);
    Ok(())
}
