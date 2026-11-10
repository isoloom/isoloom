//! What a lab's steps say beyond the spec's own checks: the project files they name (each
//! `copy`'s source, each `template` and its placeholders, each folder or file a catalogue
//! service's settings name), present in the project folder.

use std::path::Path;

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
                Step::Copy(c) if !dir.join(c.from.as_str()).exists() => out.push(missing(at.key("copy").key("from"), c.from.as_str())),
                Step::Template(t) => out.extend(template(
                    source,
                    name,
                    &at.key("template").key("from"),
                    &dir.join(t.from.as_str()),
                    t.from.as_str(),
                )),
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
            if !dir.join(path.as_str()).exists() {
                out.push(missing(FieldPath::new("machines").key(name).key("with").key(setting.as_str()), path.as_str()));
            }
        }
    }
    out
}

/// A project file the spec names that isn't there.
fn missing(at: FieldPath, path: &str) -> Problem {
    Problem::new(Code::InvalidValue, at, format!("`{path}` doesn't exist in the project folder"))
}

/// Template `file` (`from`, as written) of machine `machine`: present, text, and each of its
/// placeholders one a template may name and the spec has.
fn template(source: &syntax::Spec, machine: &str, at: &FieldPath, file: &Path, from: &str) -> Vec<Problem> {
    if !file.exists() {
        return vec![missing(at.clone(), from)];
    }
    let Ok(text) = std::fs::read_to_string(file) else {
        return vec![Problem::new(
            Code::InvalidValue,
            at.clone(),
            format!("`{from}` isn't a text file: a template is filled in as text"),
        )];
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
