//! A machine's `files`: where each goes, where its text comes from, its owner and its mode.

use std::collections::BTreeSet;

use super::Problems;
use crate::model::{Machine, MachineFile, Spec};

/// Every machine's files.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    for (name, m) in &spec.machines {
        let windows = m.vm.as_ref().is_some_and(|v| crate::images::is_windows(&v.os));
        if !m.files.is_empty() && m.docker.as_ref().is_some_and(|d| d.appliance.is_some()) {
            p.add(
                format!("machines.{name}.files"),
                "a network appliance takes its configuration from `app.config`, not `files`",
            );
            continue;
        }
        let mut seen = BTreeSet::new();
        for (i, f) in m.files.iter().enumerate() {
            let at = format!("machines.{name}.files[{i}]");
            path(&at, f, windows, p);
            if !seen.insert(f.path.trim_end_matches(['/', '\\'])) {
                p.add(format!("{at}.path"), format!("`{}` is placed twice: keep one entry per path", f.path));
            }
            text(&at, f, p);
            owner(&at, f, windows, p);
        }
        under_volume(name, m, p);
    }
}

/// An absolute path on the machine's OS, naming a file.
fn path(at: &str, f: &MachineFile, windows: bool, p: &mut Problems) {
    let path = f.path.as_str();
    let absolute = if windows {
        crate::model::is_drive_path(path)
    } else {
        path.starts_with('/') && path.len() > 1
    };
    if !absolute {
        let example = if windows { "C:\\app\\app.ini" } else { "/etc/app/app.ini" };
        p.add(
            format!("{at}.path"),
            format!("`{path}` must be an absolute path on the machine, like `{example}`"),
        );
    } else if path.ends_with(['/', '\\']) || path.split(['/', '\\']).any(|s| s == ".." || s == ".") {
        p.add(format!("{at}.path"), format!("`{path}` must name a file: no trailing `/`, `.` or `..`"));
    }
}

/// Exactly one of `source` and `content`; a source the Kubernetes ConfigMap key can carry.
fn text(at: &str, f: &MachineFile, p: &mut Problems) {
    match (&f.source, &f.content) {
        (Some(_), Some(_)) => p.add(at, "give `source` (a file of the project) or `content` (the text), not both"),
        (None, None) => p.add(at, "give `source` (a file of the project) or `content` (the text)"),
        (Some(s), None) if !s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/')) => p.add(
            format!("{at}.source"),
            format!("`{s}`: a project path of letters, digits, `.`, `_`, `-` and `/` (rename the file)"),
        ),
        _ => {}
    }
}

/// Octal digits for the mode; no owner or mode on Windows, whose files have neither.
fn owner(at: &str, f: &MachineFile, windows: bool, p: &mut Problems) {
    if windows && f.sets_owner_or_mode() {
        p.add(at, "a Windows machine's files have no `uid`, `gid` or `mode`: remove them");
        return;
    }
    if let Some(mode) = &f.mode
        && !((3..=4).contains(&mode.len()) && mode.chars().all(|c| ('0'..='7').contains(&c)))
    {
        p.add(format!("{at}.mode"), format!("`{mode}` isn't a mode: three or four octal digits, like `0644`"));
    }
}

/// A file inside one of the machine's volumes would be hidden by the volume on some targets and
/// written into its data on others: it goes elsewhere.
fn under_volume(name: &str, m: &Machine, p: &mut Problems) {
    for (i, f) in m.files.iter().enumerate() {
        let inside = m
            .volumes
            .iter()
            .find(|(_, v)| f.path.starts_with(&format!("{}/", v.path().trim_end_matches('/'))));
        if let Some((v, dir)) = inside {
            p.add(
                format!("machines.{name}.files[{i}].path"),
                format!(
                    "`{}` is inside volume `{v}` ({}): a volume keeps its own data, place the file outside it",
                    f.path,
                    dir.path()
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{lab, problems};

    /// A Linux machine.
    const LINUX: &str = "docker: { image: nginx }";

    /// A Windows machine.
    const WINDOWS: &str = "vm: { os: windows-server@2022 }";

    /// The problems of machine `a`, implemented as `machine`, with `files`.
    fn of(machine: &str, files: &str) -> Vec<String> {
        problems(&lab(&format!("machines:\n  a: {{ networks: {{ lan: 10 }}, {machine}, files: {files} }}\n")))
    }

    #[test]
    fn sound_files_pass() {
        let linux = "[{ path: /etc/app/app.ini, source: config/app-1_x.ini, uid: 1000, gid: 1000, mode: '0640' }, { path: /a, content: x, mode: '755' }, { path: /etc/a..b/x.conf, content: y }]";
        assert_eq!(of(LINUX, linux), Vec::<String>::new());
        let windows = "[{ path: 'C:\\app\\app.ini', content: x }, { path: 'D:/data/x.txt', source: data/x.txt }]";
        assert_eq!(of(WINDOWS, windows), Vec::<String>::new());
    }

    #[test]
    fn a_path_is_absolute_on_the_machines_os() {
        for bad in ["etc/app.ini", "/", "''"] {
            let path = bad.trim_matches('\'');
            let why = format!("machines.a.files[0].path: `{path}` must be an absolute path on the machine, like `/etc/app/app.ini`");
            assert_eq!(of(LINUX, &format!("[{{ path: {bad}, content: x }}]")), [why], "{bad}");
        }
        assert_eq!(
            of(WINDOWS, "[{ path: /etc/app.ini, content: x }]"),
            ["machines.a.files[0].path: `/etc/app.ini` must be an absolute path on the machine, like `C:\\app\\app.ini`"]
        );
    }

    #[test]
    fn a_path_names_a_file() {
        for bad in ["/etc/app/", "/etc/../app.ini", "/etc/./app.ini"] {
            let why = format!("machines.a.files[0].path: `{bad}` must name a file: no trailing `/`, `.` or `..`");
            assert_eq!(of(LINUX, &format!("[{{ path: {bad}, content: x }}]")), [why], "{bad}");
        }
        assert_eq!(
            of(WINDOWS, "[{ path: 'C:\\app\\', content: x }]"),
            ["machines.a.files[0].path: `C:\\app\\` must name a file: no trailing `/`, `.` or `..`"]
        );
    }

    #[test]
    fn a_path_is_placed_once() {
        assert_eq!(
            of(LINUX, "[{ path: /etc/a.ini, content: x }, { path: /etc/a.ini, content: y }]"),
            ["machines.a.files[1].path: `/etc/a.ini` is placed twice: keep one entry per path"]
        );
    }

    #[test]
    fn a_file_has_either_a_source_or_a_content() {
        assert_eq!(
            of(LINUX, "[{ path: /etc/a.ini, source: a.ini, content: x }]"),
            ["machines.a.files[0]: give `source` (a file of the project) or `content` (the text), not both"]
        );
        assert_eq!(
            of(LINUX, "[{ path: /etc/a.ini }]"),
            ["machines.a.files[0]: give `source` (a file of the project) or `content` (the text)"]
        );
    }

    #[test]
    fn a_source_is_a_plain_project_path() {
        assert_eq!(
            of(LINUX, "[{ path: /etc/a.ini, source: 'my app.ini' }]"),
            ["machines.a.files[0].source: `my app.ini`: a project path of letters, digits, `.`, `_`, `-` and `/` (rename the file)"]
        );
    }

    #[test]
    fn a_windows_file_has_no_owner_or_mode() {
        assert_eq!(
            of(WINDOWS, "[{ path: 'C:\\app.ini', content: x, mode: '0644' }]"),
            ["machines.a.files[0]: a Windows machine's files have no `uid`, `gid` or `mode`: remove them"]
        );
    }

    #[test]
    fn a_mode_is_three_or_four_octal_digits() {
        for bad in ["0999", "12", "01234"] {
            let why = format!("machines.a.files[0].mode: `{bad}` isn't a mode: three or four octal digits, like `0644`");
            assert_eq!(of(LINUX, &format!("[{{ path: /etc/a.ini, content: x, mode: '{bad}' }}]")), [why], "{bad}");
        }
    }

    #[test]
    fn an_appliance_takes_no_files() {
        assert_eq!(
            of("docker: { image: x, appliance: cisco-iol }", "[{ path: etc/x, content: x }]"),
            ["machines.a.files: a network appliance takes its configuration from `app.config`, not `files`"]
        );
    }

    #[test]
    fn a_file_goes_outside_the_machines_volumes() {
        let files = "[{ path: /srv/data/x.ini, content: x }, { path: /srv/database.ini, content: y }]";
        assert_eq!(
            of(&format!("{LINUX}, volumes: {{ data: /srv/data }}"), files),
            ["machines.a.files[0].path: `/srv/data/x.ini` is inside volume `data` (/srv/data): a volume keeps its own data, place the file outside it"]
        );
    }
}
