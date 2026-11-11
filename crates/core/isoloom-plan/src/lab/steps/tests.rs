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

/// An environment whose machine `web` has `steps`, beside `db`.
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
        reason.contains("known only once the environment runs") && reason.contains("only a catalogue entry has it"),
        "{reason}"
    );
}

#[test]
fn a_template_that_cant_be_read_says_why_and_only_bytes_that_arent_text_say_so() {
    let dir = project("unreadable", &[("binary.conf", "")]).unwrap();
    std::fs::write(dir.join("binary.conf"), [0xff, 0xfe, 0x00]).unwrap();
    std::fs::create_dir_all(dir.join("folder.conf")).unwrap();
    let lab = testing::lab(&lab(
        "[{ template: { from: binary.conf, to: /b } }, { template: { from: folder.conf, to: /f } }]",
    ));
    let found: Vec<String> = lab.problems_with_files(&dir).iter().map(isoloom_spec::syntax::Problem::line).collect();
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(
        found[0],
        "machines.web.app.steps[0].template.from: `binary.conf` isn't a text file: a template is filled in as text"
    );
    assert!(
        found[1].starts_with("machines.web.app.steps[1].template.from: `folder.conf` can't be read: "),
        "{}",
        found[1]
    );
}
