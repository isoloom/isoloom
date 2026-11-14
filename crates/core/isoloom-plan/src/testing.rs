//! What the unit tests share: environments written in a test, and their environments planned for a
//! target.

use isoloom_spec::model::{Spec, Target};

use crate::configure::DaemonTable;
use crate::{Environment, Lab};

/// The daemons of the catalogue built into Isoloom, as an environment is planned with them.
pub(crate) fn daemons() -> DaemonTable {
    DaemonTable::from(isoloom_catalogue::Catalogue::embedded().expect("the built-in catalogue loads"))
}

/// The environment of `yaml`, written in the authored format.
pub(crate) fn lab(yaml: &str) -> Lab {
    Lab::parse(yaml).unwrap_or_else(|problems| panic!("the test's spec parses: {problems:?}"))
}

/// The environment of `yaml`, checked valid.
pub(crate) fn valid(yaml: &str) -> Lab {
    let lab = lab(yaml);
    assert_eq!(lab.problems(), [], "the test's spec is valid");
    lab
}

/// The spec of `yaml` as the environment itself lowers it: every form, no target's own facts.
pub(crate) fn spec(yaml: &str) -> Spec {
    lab(yaml).spec()
}

/// The environment of `lab`, planned for `target`.
pub(crate) fn planned(lab: &Lab, target: Target) -> Environment {
    lab.plan(&lab.for_target(target), target, None).expect("the spec plans")
}

/// The environment of `yaml`, planned for `target`.
pub(crate) fn environment(yaml: &str, target: Target) -> Environment {
    planned(&lab(yaml), target)
}
