//! What the unit tests share: an environment planned for the `external` target from machines
//! written in YAML.

use isoloom_plan::Environment;
use isoloom_spec::model::Target;

/// The environment of a spec with one network, `lab` (10.9.0.0/24), and `machines`: the
/// `machines:` block's entries, indented by two spaces.
pub(crate) fn environment(machines: &str) -> Environment {
    let yaml = format!("version: 1\nname: t\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n{machines}");
    let spec = isoloom_spec::parse(&yaml).expect("the test's spec parses");
    Environment::build(&spec, Target::External, None)
}
