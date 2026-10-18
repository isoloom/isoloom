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

/// The environment `target` plans for a lab of one machine `m` (its YAML, without networks),
/// with `templates` as the project's template texts.
pub(crate) fn machine(m: &str, target: Target, templates: &[(&str, &str)]) -> Environment {
    let yaml = format!("version: 1\nname: lab\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  m: {{ networks: {{ lan: 10 }}, {m} }}\n");
    let texts = templates
        .iter()
        .filter_map(|(path, text)| Some((path.parse().ok()?, (*text).to_owned())))
        .collect();
    let lab = lab(&yaml).with_templates(texts);
    Environment::build(&lab.for_target(target), target, None)
}

/// Each recipe step of machine `m` (as [`machine`] writes it) on `target`, as `render` renders it.
pub(crate) fn rendered(
    m: &str,
    target: Target,
    templates: &[(&str, &str)],
    render: impl Fn(&crate::configure::Configured<'_>, &isoloom_plan::configure::Resource) -> Option<String>,
) -> Vec<Option<String>> {
    let env = machine(m, target, templates);
    let c = crate::configure::Configured::of(&env, "m").expect("a recipe");
    c.recipe().resources().map(|r| render(&c, r)).collect()
}
