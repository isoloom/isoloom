//! What the import tests share: the fixtures, a draft read back as the spec it is, what
//! `isoloom validate` shows for it, and a draft's notes and ports.

use std::path::{Path, PathBuf};

use isoloom_import::{Draft, NoteKind};
use isoloom_plan::Lab;
use isoloom_spec::syntax::{Kind, Problem, Publish, Recipe, Spec, Step};

/// A test's outcome: a failure is the report.
pub(crate) type Outcome<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// The fixture folder `name` (under `tests/fixtures/`).
pub(crate) fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

/// The spec a draft is, parsed as `isoloom validate` parses it.
pub(crate) fn spec(d: &Draft) -> Outcome<Spec> {
    isoloom_spec::parse(&d.yaml).map_err(|problems| format!("the draft doesn't parse: {problems:?}\n{}", d.yaml).into())
}

/// What `isoloom validate` shows for the draft written into the project folder `dir`: every
/// problem, the draft parsed, checked and lowered for every target, and every file it names
/// looked for in `dir`.
pub(crate) fn validate_in(d: &Draft, dir: &Path) -> Outcome<Vec<String>> {
    Ok(lab(d)?.problems_with_files(dir).iter().map(Problem::line).collect())
}

/// The draft's lab, allowed to reach the host as the Vagrantfile it comes from does (its synced
/// folders and public networks): whether to allow that is whoever runs it's choice, not the
/// import's.
fn lab(d: &Draft) -> Outcome<Lab> {
    use isoloom_spec::host_access::{Grants, HostAccess};
    Ok(Lab::new(spec(d)?).with_host_access(Grants::of(HostAccess::ALL)))
}

/// The scalar at `path` of the draft, read as YAML (`machines.web.hardware.gpus.count`).
pub(crate) fn value_at(d: &Draft, path: &str) -> String {
    isoloom_test_support::yaml_str(&isoloom_test_support::yaml(&d.yaml), path)
}

/// The list of scalars at `path` of the draft, read as YAML.
pub(crate) fn values_at(d: &Draft, path: &str) -> Vec<String> {
    isoloom_test_support::yaml_strs(&isoloom_test_support::yaml(&d.yaml), path)
}

/// What `isoloom validate --skip-files` shows for the draft: the files it names belong to a
/// project the test doesn't have.
pub(crate) fn validate(d: &Draft) -> Outcome<Vec<String>> {
    Ok(lab(d)?.problems().iter().map(Problem::line).collect())
}

/// Whether the draft has a note of `kind` at `at` whose whole text is `text`.
pub(crate) fn has(d: &Draft, kind: NoteKind, at: &str, text: &str) -> bool {
    d.notes.iter().any(|n| n.kind == kind && n.at == at && n.text == text)
}

/// The draft's notes, (where, kind, what), in the order the import said them.
pub(crate) fn notes(d: &Draft) -> Vec<(String, NoteKind, String)> {
    d.notes.iter().map(|n| (n.at.clone(), n.kind, n.text.clone())).collect()
}

/// Notes written as a table, to compare with [`notes`].
pub(crate) fn said(rows: &[(&str, NoteKind, &str)]) -> Vec<(String, NoteKind, String)> {
    rows.iter().map(|(at, kind, text)| ((*at).to_owned(), *kind, (*text).to_owned())).collect()
}

/// A machine's ports: (name, port, published host port, UDP).
pub(crate) fn ports(spec: &Spec, machine: &str) -> Vec<(String, u16, Option<u16>, bool)> {
    spec.machines
        .get(machine)
        .into_iter()
        .flat_map(|m| &m.ports)
        .map(|(name, p)| {
            let publish = p.publish.map(|published| match published {
                Publish::Preferred(h) | Publish::Fixed(h) => h.get(),
                Publish::Free => 0,
            });
            (name.to_string(), p.port.map_or(0, std::num::NonZeroU16::get), publish, !p.protocol.is_tcp())
        })
        .collect()
}

/// A machine's container image, when it has one.
pub(crate) fn image(spec: &Spec, machine: &str) -> Option<String> {
    match &spec.machines.get(machine)?.kind {
        Kind::App(a) => match &a.container {
            Some(Recipe::Image(i)) => Some(i.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// A machine's steps, each as its script or playbook (or its debug form).
pub(crate) fn steps(spec: &Spec, machine: &str) -> Vec<String> {
    match spec.machines.get(machine).map(|m| &m.kind) {
        Some(Kind::App(a)) => a
            .steps
            .iter()
            .map(|s| match s {
                Step::Run { script, .. } => script.to_string(),
                Step::Playbook(p) => p.to_string(),
                other => format!("{other:?}"),
            })
            .collect(),
        _ => vec![],
    }
}
