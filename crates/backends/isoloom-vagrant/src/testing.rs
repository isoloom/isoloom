//! What the unit tests share: environments planned for the `vagrant` target from YAML.

use isoloom_plan::Environment;
use isoloom_spec::model::Target;

/// The environment of the spec `yaml`, planned for the `vagrant` target.
pub(crate) fn environment(yaml: &str) -> Environment {
    let spec = isoloom_spec::parse(yaml).expect("the test's spec parses");
    Environment::build(&spec, Target::Vagrant, None)
}
