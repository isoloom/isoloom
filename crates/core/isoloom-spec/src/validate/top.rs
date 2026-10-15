//! The top level of the spec: `version`, `name`, the declared `inputs` and the `message`.

use super::{Problems, kebab, variable_name};
use crate::model::Spec;
use crate::placeholders::{self, Piece};

/// The format version this isoloom reads, and a name every target can carry.
pub(super) fn version_and_name(spec: &Spec, p: &mut Problems) {
    if spec.version != 1 {
        p.add("version", format!("unsupported version {}; this isoloom reads version 1", spec.version));
    }
    if !kebab(&spec.name) {
        p.add("name", "use kebab-case: lowercase letters, digits and dashes (e.g. supplier-portal-api)");
    } else if spec.name.len() > 55 {
        // The name becomes the Kubernetes namespace `isoloom-<name>`, which must be a 63-character
        // DNS label; 8 characters are the prefix.
        p.add("name", "at most 55 characters: it becomes the Kubernetes namespace `isoloom-<name>`");
    }
}

/// Inputs are environment variables: their names must be ones a shell accepts.
pub(super) fn inputs(spec: &Spec, p: &mut Problems) {
    for (i, input) in spec.inputs.iter().enumerate() {
        if !variable_name(input) {
            p.add(
                format!("inputs[{i}]"),
                format!("`{input}`: inputs are environment variable names (letters, digits and `_`, not starting with a digit)"),
            );
        }
    }
}

/// The message: balanced placeholders with a path inside.
pub(super) fn message(spec: &Spec, p: &mut Problems) {
    let Some(m) = &spec.message else { return };
    for piece in placeholders::pieces(m) {
        match piece {
            Piece::Text(_) => {}
            Piece::Placeholder(inner) => {
                let path = inner.trim();
                if path.is_empty() || path.split('.').any(str::is_empty) {
                    p.add("message", format!("`{{{{ {path} }}}}` isn't a path like machines.web.addresses.front"));
                }
            }
            Piece::Unclosed(_) => p.add("message", "a `{{` without its `}}`"),
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

    /// The problems of a sound environment whose message is `message`.
    fn message(message: &str) -> Vec<String> {
        problems(&lab(&format!("{WEB}message: '{message}'\n")))
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
    fn inputs_are_environment_variable_names() {
        let found = problems(&lab(&format!("inputs: [_TOKEN, A_1, 1A, A-B]\n{WEB}")));
        let why = |i: usize, input: &str| {
            format!("inputs[{i}]: `{input}`: inputs are environment variable names (letters, digits and `_`, not starting with a digit)")
        };
        assert_eq!(found, [why(2, "1A"), why(3, "A-B")]);
    }

    #[test]
    fn a_messages_placeholders_are_closed_paths() {
        assert_eq!(message("Open {{ machines.web.addresses.lan }}"), Vec::<String>::new());
        for (bad, path) in [
            ("{{ }}", ""),
            ("{{ machines..web }}", "machines..web"),
            ("{{ machines.web. }}", "machines.web."),
        ] {
            let why = format!("message: `{{{{ {path} }}}}` isn't a path like machines.web.addresses.front");
            assert_eq!(message(&format!("Open {bad}")), [why], "{bad}");
        }
        assert_eq!(message("Open {{ machines.web"), ["message: a `{{` without its `}}`"]);
    }
}
