//! What the crate's tests share: a lab written in YAML, as the spec the plan reads and as the
//! environment a target plans.

use isoloom_plan::{Environment, Lab};
use isoloom_spec::model::{Spec, Target};

/// The lab of `yaml`, which must parse and be valid.
pub(crate) fn lab(yaml: &str) -> Lab {
    let lab = Lab::parse(yaml).unwrap_or_else(|p| panic!("the test spec parses: {p:?}"));
    assert_eq!(lab.problems(), [], "the test spec is valid");
    lab
}

/// The lab of `yaml` as a whole (every form, no target's own facts).
pub(crate) fn spec(yaml: &str) -> Spec {
    lab(yaml).spec()
}

/// The environment `target` plans for the lab of `yaml`.
pub(crate) fn env(yaml: &str, target: Target) -> Environment {
    Environment::build(&lab(yaml).for_target(target), target, None)
}
