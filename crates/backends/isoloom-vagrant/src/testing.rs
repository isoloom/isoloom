//! What the unit tests share: environments planned for the `vagrant` target from YAML.

use isoloom_plan::{Environment, Lab};
use isoloom_spec::model::Target;

/// The environment of the spec `yaml` (the authored format), planned for the `vagrant` target.
pub(crate) fn environment(yaml: &str) -> Environment {
    let lab = Lab::parse(yaml).expect("the test's spec parses");
    Environment::build(&lab.for_target(Target::Vagrant), Target::Vagrant, None)
}
