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
