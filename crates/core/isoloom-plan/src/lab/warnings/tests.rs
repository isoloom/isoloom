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

#[test]
fn a_sensitive_inputs_default_that_reads_like_a_real_secret_is_a_warning() {
    let yaml = "version: 1\nname: t\ninputs:\n  TOKEN: { default: dev-launch-token }\n  EMPTY: { default: '' }\n  KEY: { default: Xk29fQp7Lr0aZt }\n  AWS: { default: AKIAEXAMPLETEST1 }\n  URL: { sensitive: false, default: https://api.internal/v1 }\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { os: debian@12, networks: { lab: 10 }, command: x }\n";
    let lab = testing::lab(yaml);
    assert_eq!(lab.problems(), []);
    let warnings = lab.warnings();
    // A development value says so; empty, or a non-sensitive input's default, is no secret at all.
    assert_eq!(
        warnings.iter().map(|w| w.path.as_str()).collect::<Vec<_>>(),
        ["inputs.KEY.default", "inputs.AWS.default"]
    );
    assert!(
        warnings[0].reason.contains("`Xk29fQp7Lr0a…` doesn't read as a development value"),
        "{}",
        warnings[0].reason
    );
    assert_eq!(warnings[0].span.map(|s| s.line), Some(6));
}

/// `own` has a Dockerfile and steps (two recipes, written once for its 2 copies); `web` a
/// Dockerfile alone and `vm` steps alone.
const TWO_RECIPES: &str = "version: 1\nname: t\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  own: { count: 2, os: debian@12, networks: { lab: 10 }, app: { build: build/own, steps: [{ run: setup.sh }] } }\n  web: { networks: { lab: 20 }, app: { build: build/web } }\n  vm: { os: debian@12, networks: { lab: 30 }, app: { steps: [{ run: setup.sh }] } }\n";

/// The warnings about two recipes, as `path: reason | fix`.
fn two_recipes(lab: &crate::Lab) -> Vec<String> {
    lab.warnings()
        .iter()
        .filter(|w| w.reason.starts_with("`app.build` and `app.steps`"))
        .map(|w| format!("{}: {} | {}", w.path, w.reason, w.fix.as_deref().unwrap_or_default()))
        .collect()
}

fn with_dockerfile(text: &str) -> crate::Lab {
    testing::lab(TWO_RECIPES).with_dockerfiles([("build/own/Dockerfile".parse().unwrap(), text.to_owned())].into())
}

#[test]
fn a_machine_with_a_dockerfile_and_steps_is_one_warning_at_its_steps() {
    let lab = testing::lab(TWO_RECIPES);
    assert_eq!(lab.problems(), []);
    assert_eq!(
        two_recipes(&lab),
        [
            "machines.own.app.steps: `app.build` and `app.steps` are two recipes for one machine: containers run the Dockerfile, VMs the steps, and the two drift apart | describe the machine once, so every target comes from one description: a catalogue `service`, or `os` and portable `steps` alone (keep both only while the Dockerfile has no VM form)"
        ]
    );
    let w = lab.warnings().into_iter().find(|w| w.path.as_str() == "machines.own.app.steps").unwrap();
    assert_eq!(w.span.map(|s| s.line), Some(5), "the spec's own line");
}

#[test]
fn a_dockerfile_that_translates_makes_the_steps_unneeded() {
    let found = two_recipes(&with_dockerfile("FROM debian:12\nRUN apt-get update && apt-get install -y curl\n"));
    assert_eq!(
        found,
        [
            "machines.own.app.steps: `app.build` and `app.steps` are two recipes for one machine: containers run the Dockerfile, VMs the steps, and the two drift apart. Its Dockerfile translates to a VM form on its own, so the steps aren't needed | describe the machine once: `build` alone (VMs run its translation), a catalogue `service`, or `os` and portable `steps` alone"
        ]
    );
}

#[test]
fn a_dockerfile_with_no_vm_form_says_why_both_are_needed() {
    let found = two_recipes(&with_dockerfile(
        "FROM debian:12\nCOPY --from=mlocati/php-extension-installer /usr/bin/install-php-extensions /usr/bin/\n",
    ));
    assert_eq!(
        found,
        [
            "machines.own.app.steps: `app.build` and `app.steps` are two recipes for one machine: containers run the Dockerfile, VMs the steps, and the two drift apart. Both are needed here while its Dockerfile has no VM form: build/own/Dockerfile line 2 (`COPY`): `--from=mlocati/php-extension-installer` copies out of another image, which the VM doesn't have | describe the machine once, so every target comes from one description: a catalogue `service`, or `os` and portable `steps` alone; else keep the two in step"
        ]
    );
}
