use crate::testing;

/// Machine `web` installs `nginx` (a daemon) at its second step, with no `service` step for it.
const LAB: &str = "version: 1\nname: t\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { os: debian@12, networks: { lab: 10 }, command: x, app: { steps: [{ run: a.sh }, { packages: [curl, nginx] }] } }\n";

#[test]
fn a_daemon_installed_without_its_service_step_is_a_warning_not_a_problem() {
    let lab = testing::lab(LAB);
    assert_eq!(lab.problems(), []);
    let warnings = lab.warnings();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].path.as_str(), "machines.web.app.steps[1]");
    assert!(
        warnings[0].reason.starts_with("`nginx` installs a daemon that starts on Debian VMs only"),
        "{}",
        warnings[0].reason
    );
    assert_eq!(
        warnings[0].fix.as_deref(),
        Some("`service: nginx`, or `{ service: { name: nginx, enabled: false } }`")
    );
    assert!(warnings[0].span.is_some_and(|s| s.line == 5));
}

#[test]
fn a_counted_machines_warning_is_said_once_at_the_machine() {
    let yaml = "version: 1\nname: t\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  ws: { count: 3, os: rocky@9, networks: { lab: 10 }, app: { steps: [{ packages: [openssh-server] }] } }\n";
    let warnings = testing::lab(yaml).warnings();
    assert_eq!(warnings.iter().map(|w| w.path.as_str()).collect::<Vec<_>>(), ["machines.ws.app.steps[0]"]);
    assert!(warnings[0].fix.as_deref().is_some_and(|f| f.starts_with("`service: sshd`")));
}

#[test]
fn a_best_effort_seal_is_an_advisory_warning() {
    let yaml = "version: 1\nname: t\nnetworks: { sealed: { cidr: 10.9.0.0/24, internet: false } }\nmachines:\n  app: { networks: { sealed: 10 }, app: { image: x }, privileged: true, seal: best-effort }\n";
    let warnings = testing::lab(yaml).warnings();
    assert_eq!(warnings.iter().map(|w| w.path.as_str()).collect::<Vec<_>>(), ["machines.app.seal"]);
    assert!(
        warnings[0]
            .reason
            .starts_with("`seal: best-effort` makes this machine's `internet: false` seal advisory"),
        "{}",
        warnings[0].reason
    );
}

#[test]
fn a_best_effort_seal_of_a_machine_with_copies_is_one_warning_at_its_line() {
    let yaml = "version: 1\nname: t\nnetworks: { sealed: { cidr: 10.9.0.0/24, internet: false } }\nmachines:\n  ws:\n    count: 2\n    networks: { sealed: 10 }\n    app: { image: x }\n    seal: best-effort\n";
    let warnings = testing::lab(yaml).warnings();
    assert_eq!(warnings.iter().map(|w| w.path.as_str()).collect::<Vec<_>>(), ["machines.ws.seal"]);
    assert_eq!(warnings[0].span.map(|s| s.line), Some(9), "the spec's own line");
}
