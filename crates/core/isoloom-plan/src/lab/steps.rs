//! What an environment's steps say beyond the spec's own checks: the project files they name (each
//! `copy`'s source, each `template` and its placeholders, each folder or file a catalogue
//! service's settings name), present in the project folder.

use std::io::ErrorKind;
use std::path::Path;

use isoloom_spec::project_path;
use isoloom_spec::syntax::placeholder::Place;
use isoloom_spec::syntax::{self, Code, FieldPath, Problem, Step};

/// The project files the steps name, each present in `dir`, and each template's placeholders
/// resolving against the spec.
pub(super) fn step_files(source: &syntax::Spec, dir: &Path) -> Vec<Problem> {
    let mut out = Vec::new();
    for (name, m) in &source.machines {
        let Some(app) = m.app() else { continue };
        for (i, step) in app.steps.iter().enumerate() {
            let at = FieldPath::new("machines").key(name).key("app").key("steps").index(i);
            match step {
                Step::Copy(c) => {
                    if let Err(why) = project_path::inside(dir, c.from.as_str()) {
                        out.push(escaped(at.key("copy").key("from"), c.from.as_str(), &why));
                    }
                }
                Step::Template(t) => {
                    let at = at.key("template").key("from");
                    match project_path::inside(dir, t.from.as_str()) {
                        Ok(file) => out.extend(template(source, name, &at, &file, t.from.as_str())),
                        Err(why) => out.push(escaped(at, t.from.as_str(), &why)),
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// The project files and folders the catalogue services' settings name (`with: { root: site/
/// }`), each present in `dir`.
pub(super) fn setting_files(source: &syntax::Spec, catalogue: &isoloom_catalogue::Catalogue, dir: &Path) -> Vec<Problem> {
    let mut out = Vec::new();
    for name in source.machines.keys() {
        let Ok(resolved) = catalogue.resolve(source, name) else { continue };
        for (setting, value) in resolved.iter().flat_map(|r| r.settings.iter()) {
            let (isoloom_catalogue::SettingValue::Folder(path) | isoloom_catalogue::SettingValue::File(path)) = value else {
                continue;
            };
            if let Err(why) = project_path::inside(dir, path.as_str()) {
                out.push(escaped(
                    FieldPath::new("machines").key(name).key("with").key(setting.as_str()),
                    path.as_str(),
                    &why,
                ));
            }
        }
    }
    out
}

/// A project file the spec names that isn't there, or isn't inside the project once its links are
/// followed.
fn escaped(at: FieldPath, path: &str, why: &project_path::Escape) -> Problem {
    Problem::new(Code::InvalidValue, at, why.about(path))
}

/// Template `file` (`from`, as written, found inside the project) of machine `machine`: present, readable, text, and each of its
/// placeholders one a template may name and the spec has.
fn template(source: &syntax::Spec, machine: &str, at: &FieldPath, file: &Path, from: &str) -> Vec<Problem> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::InvalidData => {
            return vec![Problem::new(
                Code::InvalidValue,
                at.clone(),
                format!("`{from}` isn't a text file: a template is filled in as text"),
            )];
        }
        Err(e) => return vec![Problem::new(Code::InvalidValue, at.clone(), format!("`{from}` can't be read: {e}"))],
    };
    // One problem per template, every placeholder in it: a field holds one problem.
    let found = syntax::placeholder_problems(source, Some(machine), &text, Place::Template);
    let Some(first) = found.first() else { return Vec::new() };
    let code = if first.unclosed { Code::InvalidValue } else { Code::UnknownReference };
    let reasons: Vec<&str> = found.iter().map(|u| u.reason.as_str()).collect();
    vec![
        Problem::new(code, at.clone(), format!("`{from}`: {}", reasons.join("; ")))
            .fix("a template names `machine.name`, `machine.addresses.<network>`, `machines.<m>.addresses.<network>` and `machines.<m>.ports.<name>.port`"),
    ]
}

#[cfg(test)]
mod tests;
