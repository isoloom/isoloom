//! What the integration tests share: environments written in a test, what an author sees of them, and
//! their plans.

use std::path::{Path, PathBuf};

use isoloom_plan::{Environment, Lab};
use isoloom_spec::model::Target;
use isoloom_spec::syntax::{self, Problem, Problems};

/// A test's outcome: the problems that stop its spec from being read are the report.
pub(crate) type Outcome<T = ()> = Result<T, Problems>;

/// The environment of `yaml`, or the problems that stop it from being read.
///
/// # Errors
///
/// The parser's problems.
pub(crate) fn lab(yaml: &str) -> Outcome<Lab> {
    Lab::parse(yaml)
}

/// The problems the parser refuses `yaml` with (none when it reads it).
#[must_use]
pub(crate) fn refused(yaml: &str) -> Vec<Problem> {
    syntax::parse(yaml).err().unwrap_or_default()
}

/// What an author sees for `yaml`: the parser's problems, or the environment's once the spec parses,
/// one line each.
#[must_use]
pub(crate) fn problems(yaml: &str) -> Vec<String> {
    let found = isoloom_plan::Lab::parse(yaml).map_or_else(|refused| refused, |lab| lab.problems());
    found.iter().map(Problem::line).collect()
}

/// Some problem of `yaml` is at `at` and its reason contains `fragment`.
///
/// # Panics
///
/// When none is: the test's report.
#[track_caller]
pub(crate) fn assert_problem(yaml: &str, at: &str, fragment: &str) {
    let found = problems(yaml);
    let prefix = format!("{at}: ");
    assert!(
        found.iter().any(|p| p.starts_with(&prefix) && p.contains(fragment)),
        "expected `{at}: …{fragment}…`, got {found:#?}\nspec:\n{yaml}"
    );
}

/// `lab` planned for `target`; a plan that fails is one problem saying why.
fn planned(lab: &Lab, target: Target) -> Outcome<Environment> {
    lab.plan(&lab.for_target(target), target, None)
        .map_err(|e| vec![Problem::new(syntax::Code::Conflict, syntax::FieldPath::new("machines"), e.to_string())])
}

/// The plan of `yaml` for `target`, whatever its problems: what the plan asks of a backend,
/// whatever else is wrong.
///
/// # Errors
///
/// The parser's problems, or why it can't be planned.
pub(crate) fn plan_anyway(yaml: &str, target: Target) -> Outcome<Environment> {
    let lab = lab(yaml)?;
    planned(&lab, target)
}

/// The plan of `yaml` for `target`, the spec checked valid.
///
/// # Errors
///
/// The parser's problems, or the environment's, or why it can't be planned.
pub(crate) fn plan(yaml: &str, target: Target) -> Outcome<Environment> {
    let lab = lab(yaml)?;
    let problems = lab.problems();
    if !problems.is_empty() {
        return Err(problems);
    }
    planned(&lab, target)
}

/// The targets' ids, in order.
#[must_use]
pub(crate) fn ids(targets: Vec<Target>) -> Vec<&'static str> {
    targets.into_iter().map(Target::id).collect()
}

/// The repository's `examples/` folder.
fn examples_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples")
}

/// The examples' names, sorted: the folders with their isoloom.yml (examples/paas groups its own,
/// out of this list).
///
/// # Errors
///
/// The folder can't be read.
pub(crate) fn example_names() -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(examples_root())? {
        let path = entry?.path();
        if path.join("isoloom.yml").is_file()
            && let Some(name) = path.file_name()
        {
            out.push(name.to_string_lossy().into_owned());
        }
    }
    out.sort();
    Ok(out)
}

/// The example `name`'s lab.
///
/// # Errors
///
/// Why it doesn't load, named after the example.
pub(crate) fn example(name: &str) -> Result<Lab, String> {
    isoloom_spec::load(&examples_root().join(name))
        .map(Lab::new)
        .map_err(|e| format!("{name}: {e}"))
}

/// Every example, as its name and its lab.
///
/// # Errors
///
/// The folder can't be read, or an example doesn't load.
pub(crate) fn examples() -> Result<Vec<(String, Lab)>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for name in example_names()? {
        let lab = example(&name)?;
        out.push((name, lab));
    }
    Ok(out)
}
