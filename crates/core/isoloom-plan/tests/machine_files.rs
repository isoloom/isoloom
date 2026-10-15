//! `machines.*.files` through parsing and the lab's checks: one mistake per spec, each problem at
//! its field with what to do.

use isoloom_spec::syntax::{FileSource, Problem};
use testing::{assert_problem, lab, problems};

pub mod testing;

/// A spec with one machine whose body is `body`.
fn spec(body: &str) -> String {
    format!("version: 1\nname: t\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  m: {body}\n")
}

/// A container machine with these files.
fn container(files: &str) -> String {
    spec(&format!(
        "{{ networks: {{ lab: 5 }}, app: {{ image: nginx }}, volumes: {{ data: /srv/data }}, files: {files} }}"
    ))
}

#[test]
fn a_valid_file_parses_with_its_fields() {
    let yaml = container("[{ path: /etc/app.ini, content: \"a=1\\n\", uid: 1000, gid: 1000, mode: 0600 }, { path: /etc/b.conf, source: conf/b.conf }]");
    assert_eq!(problems(&yaml), Vec::<String>::new());
    let lab = lab(&yaml).unwrap();
    let f = &lab.source().machines["m"].files;
    assert_eq!(
        (f[0].uid, f[0].gid, f[0].mode.map(|m| m.to_string()).as_deref()),
        (Some(1000), Some(1000), Some("0600"))
    );
    assert_eq!(f[1].source, FileSource::Source("conf/b.conf".into()));
}

#[test]
fn a_file_has_one_text_source() {
    let at = "machines.m.files[0]";
    assert_problem(&container("[{ path: /a, source: a, content: x }]"), at, "both given: keep one source");
    assert_problem(
        &container("[{ path: /a }]"),
        at,
        "a file needs one of `source`, `content`, `input` and `secret`",
    );
    assert_problem(&container("[{ path: /a, source: 'my file.txt' }]"), at, "rename the file");
}

#[test]
fn a_path_is_absolute_names_a_file_and_is_placed_once() {
    let at = "machines.m.files[0].path";
    assert_problem(&container("[{ path: etc/a, content: x }]"), at, "absolute path");
    assert_problem(&container("[{ path: /etc/, content: x }]"), at, "must name a file");
    assert_problem(&container("[{ path: /etc/../a, content: x }]"), at, "must name a file");
    assert_problem(
        &container("[{ path: /a, content: x }, { path: /a, content: y }]"),
        "machines.m.files[1].path",
        "placed twice",
    );
    assert_problem(&container("[{ path: /srv/data/a.ini, content: x }]"), at, "inside volume `data`");
}

#[test]
fn a_mode_is_octal() {
    assert_problem(&container("[{ path: /a, content: x, mode: 0800 }]"), "machines.m.files[0].mode", "octal digits");
    assert_problem(&container("[{ path: /a, content: x, mode: rw }]"), "machines.m.files[0].mode", "octal digits");
    assert_eq!(problems(&container("[{ path: /a, content: x, mode: 755 }]")), Vec::<String>::new());
}

#[test]
fn windows_paths_and_no_owner_on_windows() {
    let win = |files: &str| {
        spec(&format!(
            "{{ networks: {{ lab: 5 }}, os: windows-server@2022, app: {{ steps: [{{ run: a.ps1 }}] }}, files: {files} }}"
        ))
    };
    let ok = problems(&win("[{ path: 'C:\\app\\a.ini', content: x }]"));
    assert_eq!(ok, Vec::<String>::new());
    assert_problem(&win("[{ path: /etc/a, content: x }]"), "machines.m.files[0].path", "C:\\app\\app.ini");
    assert_problem(
        &win("[{ path: 'C:\\a.ini', content: x, uid: 1 }]"),
        "machines.m.files[0].uid",
        "Windows has no owner ids or modes",
    );
}

#[test]
fn an_appliance_has_no_files() {
    let yaml = spec("{ networks: { lab: 5 }, app: { image: vrnetlab/cisco_iol, appliance: cisco-iol }, files: [{ path: /a, content: x }] }");
    assert_problem(&yaml, "machines.m.files", "app.config");
}

#[test]
fn a_source_is_a_file_of_the_project() {
    let dir = std::env::temp_dir().join(format!("isoloom-machine-files-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("conf/sub")).unwrap();
    std::fs::write(dir.join("conf/a.ini"), "a=1\n").unwrap();
    let yaml =
        container("[{ path: /a, source: conf/a.ini }, { path: /b, source: conf/missing.ini }, { path: /c, source: conf/sub }, { path: /d, source: ../x }]");
    let found: Vec<String> = lab(&yaml).unwrap().problems_with_files(&dir).iter().map(Problem::line).collect();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(found.len(), 3, "{found:#?}");
    assert!(
        found[0].starts_with("machines.m.files[1]: ") && found[0].contains("doesn't exist"),
        "{found:#?}"
    );
    assert!(found[1].starts_with("machines.m.files[2]: ") && found[1].contains("is a folder"), "{found:#?}");
    assert!(
        found[2].starts_with("machines.m.files[3]: ") && found[2].contains("inside the project"),
        "{found:#?}"
    );
}
