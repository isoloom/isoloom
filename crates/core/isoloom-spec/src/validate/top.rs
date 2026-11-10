//! The top level of the spec: `version`, `name`, the declared `inputs` and the `tags` (the
//! `message`'s placeholders are the placeholder grammar's, [`crate::syntax::placeholder`]).

use isoloom_manifest::is_kebab;

use super::{Problems, is_variable};
use crate::model::Spec;
use crate::tags;

/// The format version this isoloom reads, and a name every target can carry.
pub(super) fn version_and_name(spec: &Spec, p: &mut Problems) {
    if spec.version != 1 {
        p.add("version", format!("unsupported version {}; this isoloom reads version 1", spec.version));
    }
    if !is_kebab(&spec.name) {
        p.add("name", "use kebab-case: lowercase letters, digits and dashes (e.g. supplier-portal-api)");
    } else if spec.name.len() > 55 {
        // The name becomes the Kubernetes namespace `isoloom-<name>`, which must be a 63-character
        // DNS label; 8 characters are the prefix.
        p.add("name", "at most 55 characters: it becomes the Kubernetes namespace `isoloom-<name>`");
    }
}

/// The tags every cloud takes, by the strictest cloud's rules ([`tags::key_refusal`],
/// [`tags::value_refusal`]), none clashing with another where a cloud carries them
/// ([`tags::clash`]).
pub(super) fn tags(spec: &Spec, p: &mut Problems) {
    for (key, value) in &spec.tags {
        let at = format!("tags.{key}");
        if let Some(why) = tags::key_refusal(key) {
            p.add(at.clone(), why);
        }
        if let Some(why) = tags::value_refusal(key, value) {
            p.add(at, why);
        }
    }
    if let Some(c) = tags::clash(&spec.tags) {
        p.add(format!("tags.{}", c.key), format!("{}: name them apart", c.why));
    }
}

/// Inputs are environment variables: their names must be ones a shell accepts.
pub(super) fn inputs(spec: &Spec, p: &mut Problems) {
    for (i, input) in spec.inputs.iter().map(|i| &i.name).enumerate() {
        if !is_variable(input) {
            p.add(
                format!("inputs[{i}]"),
                format!("`{input}`: inputs are environment variable names (letters, digits and `_`, not starting with a digit)"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{lab, problems};

    /// One sound machine on `lan`.
    const WEB: &str = "machines: { web: { networks: { lan: 10 }, docker: { image: nginx } } }\n";

    /// The problems of a sound environment named `name`.
    fn named(name: &str) -> Vec<String> {
        problems(&format!("version: 1\nname: {name}\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\n{WEB}"))
    }

    #[test]
    fn a_name_is_kebab_case_and_fits_a_kubernetes_namespace() {
        assert_eq!(named(&"a".repeat(55)), Vec::<String>::new());
        assert_eq!(
            named(&"a".repeat(56)),
            ["name: at most 55 characters: it becomes the Kubernetes namespace `isoloom-<name>`"]
        );
        assert_eq!(
            named("Portal"),
            ["name: use kebab-case: lowercase letters, digits and dashes (e.g. supplier-portal-api)"]
        );
    }

    #[test]
    fn each_tag_follows_every_clouds_rules_and_none_clashes() {
        let tags = "tags: { cost-center: \"1234\", 9x: a, Managed-By: me, team: \"a\\nb\", Team: ops, x: \"\" }\n";
        assert_eq!(
            problems(&lab(&format!("{tags}{WEB}"))),
            [
                "tags.9x: a key of at most 63 letters, digits and `_ . : -`, starting with a letter, e.g. cost-center",
                "tags.Managed-By: `Managed-By` is Isoloom's own tag: name yours otherwise",
                "tags.team: a value on one line",
                "tags.x: `x:<value>` is at most 50 characters and at least 3 (Linode's tags): a value of 1 to 48 characters here",
                "tags.Team: `team` and `Team` are one tag on Azure, whose tag names ignore case: name them apart",
            ]
        );
    }

    #[test]
    fn inputs_are_environment_variable_names() {
        let found = problems(&lab(&format!("inputs: [_TOKEN, A_1, 1A, A-B]\n{WEB}")));
        let why = |i: usize, input: &str| {
            format!("inputs[{i}]: `{input}`: inputs are environment variable names (letters, digits and `_`, not starting with a digit)")
        };
        assert_eq!(found, [why(2, "1A"), why(3, "A-B")]);
    }
}
