//! What the integration tests share: labs written in a test, what an author sees of them, and
//! their plans.

use isoloom_plan::{Environment, Lab};
use isoloom_spec::model::Target;
use isoloom_spec::syntax::{self, Problem, Problems};

/// A test's outcome: the problems that stop its spec from being read are the report.
pub type Outcome<T = ()> = Result<T, Problems>;

/// The lab of `yaml`, or the problems that stop it from being read.
///
/// # Errors
///
/// The parser's problems.
pub fn lab(yaml: &str) -> Outcome<Lab> {
    Lab::parse(yaml)
}

/// The problems the parser refuses `yaml` with (none when it reads it).
#[must_use]
pub fn refused(yaml: &str) -> Vec<Problem> {
    syntax::parse(yaml).err().unwrap_or_default()
}

/// What an author sees for `yaml`: the parser's problems, or the lab's once the spec parses,
/// one line each.
#[must_use]
pub fn problems(yaml: &str) -> Vec<String> {
    isoloom_plan::problems_in(yaml).iter().map(Problem::line).collect()
}

/// Some problem of `yaml` is at `at` and its reason contains `fragment`.
///
/// # Panics
///
/// When none is: the test's report.
#[track_caller]
pub fn assert_problem(yaml: &str, at: &str, fragment: &str) {
    let found = problems(yaml);
    let prefix = format!("{at}: ");
    assert!(
        found.iter().any(|p| p.starts_with(&prefix) && p.contains(fragment)),
        "expected `{at}: …{fragment}…`, got {found:#?}\nspec:\n{yaml}"
    );
}

/// The plan of `yaml` for `target`, whatever its problems: what the plan asks of a backend,
/// whatever else is wrong.
///
/// # Errors
///
/// The parser's problems.
pub fn plan_anyway(yaml: &str, target: Target) -> Outcome<Environment> {
    let lab = lab(yaml)?;
    Ok(Environment::build(&lab.for_target(target), target, None))
}

/// The plan of `yaml` for `target`, the spec checked valid.
///
/// # Errors
///
/// The parser's problems, or the lab's.
pub fn plan(yaml: &str, target: Target) -> Outcome<Environment> {
    let lab = lab(yaml)?;
    let problems = lab.problems();
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(Environment::build(&lab.for_target(target), target, None))
}

/// The targets' ids, in order.
#[must_use]
pub fn ids(targets: Vec<Target>) -> Vec<&'static str> {
    targets.into_iter().map(Target::id).collect()
}
