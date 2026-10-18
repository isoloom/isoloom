//! What the unit tests share: labs written in a test, and their environments planned for a
//! target.

use isoloom_spec::model::{Spec, Target};

use crate::{Environment, Lab};

/// The lab of `yaml`, written in the authored format.
pub(crate) fn lab(yaml: &str) -> Lab {
    Lab::parse(yaml).unwrap_or_else(|problems| panic!("the test's spec parses: {problems:?}"))
}

/// The lab of `yaml`, checked valid.
pub(crate) fn valid(yaml: &str) -> Lab {
    let lab = lab(yaml);
    assert_eq!(lab.problems(), [], "the test's spec is valid");
    lab
}

/// The spec of `yaml` as the lab itself lowers it: every form, no target's own facts.
pub(crate) fn spec(yaml: &str) -> Spec {
    lab(yaml).spec()
}

/// The environment of `lab`, planned for `target`.
pub(crate) fn planned(lab: &Lab, target: Target) -> Environment {
    Environment::build(&lab.for_target(target), target, None)
}

/// The environment of `yaml`, planned for `target`.
pub(crate) fn environment(yaml: &str, target: Target) -> Environment {
    planned(&lab(yaml), target)
}
