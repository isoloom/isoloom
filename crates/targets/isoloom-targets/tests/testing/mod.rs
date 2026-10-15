//! What the integration tests share: the repository's examples, planned for a target.

use std::error::Error;
use std::path::Path;

use isoloom_plan::Environment;
use isoloom_spec::model::Target;

/// A test's outcome: a failure is the report.
pub type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// The plan for `target` of the repository's example `name` (its folder under `examples/`),
/// once validation finds nothing wrong with it.
///
/// # Errors
///
/// When the example doesn't load, or isn't valid.
pub fn example(name: &str, target: Target) -> Outcome<Environment> {
    let spec = isoloom_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name))?;
    match isoloom_spec::validate(&spec) {
        problems if problems.is_empty() => Ok(Environment::build(&spec, target, None)),
        problems => Err(format!("example `{name}` isn't valid: {problems:?}").into()),
    }
}
