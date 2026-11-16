//! What the unit tests share: an environment planned for the `external` target from machines
//! written in YAML.

use isoloom_plan::Environment;
use isoloom_spec::model::Target;

/// The environment of a spec with one network, `lab` (10.9.0.0/24), and `machines`: the
/// `machines:` block's entries, indented by two spaces (then any other top-level key, such as
/// the hosts under `overrides.external`).
pub(crate) fn environment(machines: &str) -> Environment {
    let yaml = format!("version: 1\nname: t\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n{machines}");
    let lab = isoloom_plan::Lab::new(isoloom_spec::parse(&yaml).expect("the test's spec parses"));
    lab.plan(&lab.for_target(Target::External), Target::External, None).expect("the spec plans")
}

/// A YAML text read back, each `!unsafe` tag dropped (the value it tags kept).
pub(crate) fn untagged(text: &str) -> serde_yaml_ng::Value {
    fn untag(v: serde_yaml_ng::Value) -> serde_yaml_ng::Value {
        match v {
            serde_yaml_ng::Value::Tagged(t) => untag(t.value),
            serde_yaml_ng::Value::Sequence(items) => serde_yaml_ng::Value::Sequence(items.into_iter().map(untag).collect()),
            serde_yaml_ng::Value::Mapping(m) => serde_yaml_ng::Value::Mapping(m.into_iter().map(|(k, item)| (k, untag(item))).collect()),
            other => other,
        }
    }
    untag(serde_yaml_ng::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}")))
}
