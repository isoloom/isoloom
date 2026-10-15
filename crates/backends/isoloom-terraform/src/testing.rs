//! What the unit tests share: a plan from a spec written in the test.

use isoloom_plan::Environment;
use isoloom_spec::model::Target;

/// The plan of `yaml`, a valid spec, for `target`.
pub(crate) fn environment(yaml: &str, target: Target) -> Environment {
    let spec = isoloom_spec::parse(yaml).unwrap();
    assert_eq!(isoloom_spec::validate(&spec), vec![], "the spec is valid");
    Environment::build(&spec, target, None)
}
