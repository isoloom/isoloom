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
                "a network appliance takes its configuration from `docker.config`, not `files`",
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
        let b = path.as_bytes();
        b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/')
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
