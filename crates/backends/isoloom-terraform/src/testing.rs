//! What the unit tests share: a plan from a spec written in the test.

use isoloom_plan::Environment;
use isoloom_spec::model::Target;

/// The plan of `yaml`, a valid spec in the authored format, for `target`.
pub(crate) fn environment(yaml: &str, target: Target) -> Environment {
    let lab = isoloom_plan::Lab::new(isoloom_spec::parse(yaml).unwrap());
    assert_eq!(lab.problems(), [], "the spec is valid");
    Environment::build(&lab.for_target(target), target, None)
}
