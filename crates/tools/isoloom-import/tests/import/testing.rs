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
    Ok(Lab::new(spec(d)?).problems_with_files(dir).iter().map(Problem::line).collect())
}

/// What `isoloom validate --skip-files` shows for the draft: the files it names belong to a
/// project the test doesn't have.
pub(crate) fn validate(d: &Draft) -> Outcome<Vec<String>> {
    Ok(Lab::new(spec(d)?).problems().iter().map(Problem::line).collect())
}

/// Whether the draft has a note of `kind` at `at` whose text says `text`.
pub(crate) fn has(d: &Draft, kind: NoteKind, at: &str, text: &str) -> bool {
    d.notes.iter().any(|n| n.kind == kind && n.at == at && n.text.contains(text))
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
                Step::Run { script, .. } => script.clone(),
                Step::Playbook(p) => p.clone(),
                other => format!("{other:?}"),
            })
            .collect(),
        _ => vec![],
    }
}
