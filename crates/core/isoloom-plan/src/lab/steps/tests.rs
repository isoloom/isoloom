use std::path::PathBuf;

use crate::testing;

/// A fresh project folder holding `files` (path, text).
fn project(name: &str, files: &[(&str, &str)]) -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("isoloom-step-files-{}-{name}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    for (path, text) in files {
        let file = dir.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, text)?;
    }
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// A lab whose machine `web` has `steps`, beside `db`.
fn lab(steps: &str) -> String {
    format!(
        "version: 1\nname: t\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  web: {{ os: debian@12, networks: {{ lab: 10 }}, command: x, app: {{ steps: {steps} }} }}\n  db: {{ os: debian@12, networks: {{ lab: 20 }}, command: y, ports: {{ pg: 5432 }} }}\n"
    )
}

#[test]
fn the_files_steps_name_are_checked_in_the_project() {
    let dir = project("missing", &[("conf/a.ini", "a")]).unwrap();
    let lab = testing::lab(&lab(
        "[{ copy: { from: conf/a.ini, to: /a } }, { copy: { from: nope, to: /b } }, { template: { from: t.conf, to: /t } }]",
    ));
    let found: Vec<String> = lab.problems_with_files(&dir).iter().map(isoloom_spec::syntax::Problem::line).collect();
    assert_eq!(
        found,
        [
            "machines.web.app.steps[1].copy.from: `nope` doesn't exist in the project folder",
            "machines.web.app.steps[2].template.from: `t.conf` doesn't exist in the project folder",
        ]
    );
}

#[test]
fn a_templates_placeholders_resolve_against_the_spec() {
    let dir = project(
        "placeholders",
        &[
            (
                "ok.conf",
                "{{ machine.name }} {{ machine.addresses.lab }} {{ machines.db.addresses.lab }}:{{ machines.db.ports.pg.port }}",
            ),
            (
                "bad.conf",
                "{{ machines.db.ports.nope.port }} {{ machines.db.ports.pg.published }} {{ with.x }}",
            ),
        ],
    )
    .unwrap();
    let lab = testing::lab(&lab("[{ template: { from: ok.conf, to: /ok } }, { template: { from: bad.conf, to: /bad } }]"));
    let found = lab.problems_with_files(&dir);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].path.as_str(), "machines.web.app.steps[1].template.from");
    let reason = &found[0].reason;
    assert!(
        reason.starts_with("`bad.conf`: `{{ machines.db.ports.nope.port }}`: `db` has no port named `nope`"),
        "{reason}"
    );
    assert!(
        reason.contains("known only once the lab runs") && reason.contains("only a catalogue entry has it"),
        "{reason}"
    );
}

#[test]
fn a_daemon_installed_without_its_service_step_is_a_warning_not_a_problem() {
    let lab = testing::lab(&lab("[{ run: a.sh }, { packages: [curl, nginx] }]"));
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
    let yaml = "version: 1\nname: t\nnetworks: { sealed: { cidr: 10.9.0.0/24, internet: false } }\nmachines:\n  pwn: { networks: { sealed: 10 }, app: { image: x }, privileged: true, seal: best-effort }\n";
    let warnings = testing::lab(yaml).warnings();
    assert_eq!(warnings.iter().map(|w| w.path.as_str()).collect::<Vec<_>>(), ["machines.pwn.seal"]);
    assert!(warnings[0].reason.starts_with("`seal: best-effort` makes this machine's `internet: false` seal advisory"), "{}", warnings[0].reason);
}
