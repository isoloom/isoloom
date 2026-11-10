//! Why the `external` target's files aren't rendered, and how a backend reports it.

use isoloom_plan::Refusal;
use isoloom_render::Error as RenderError;
use isoloom_spec::model::Target;

/// Why the `external` target's files aren't rendered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The environment needs what hosts that already exist can't give (a network appliance,
    /// an IPv6-only network).
    #[error("{}: {refusal}", Target::External.id())]
    Refused {
        /// The first requirement the target can't meet.
        refusal: Refusal,
    },
    /// A `packages` step on a Windows host reached the renderer, which writes `ansible.builtin`
    /// and `ansible.posix` tasks only: the lab's lowering refuses such a step first, so this is a
    /// bug.
    #[error("{}: machine `{machine}`: a `packages` step on Windows has no Ansible task here", Target::External.id())]
    WindowsPackages {
        /// The machine.
        machine: String,
    },
    /// A firewall step needs a collection (`ansible.posix`) the embedded catalogue doesn't pin,
    /// or the catalogue doesn't load: its own tests fail first, so this is a bug.
    #[error("{}: the collection `{name}` a firewall step needs has no pin: {why}", Target::External.id())]
    Collection {
        /// The collection.
        name: &'static str,
        /// Why it has no pin.
        why: String,
    },
}

/// A result whose error is this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<Error> for RenderError {
    /// The error as every backend reports it: the plan's refusal, as the `external` target's.
    fn from(e: Error) -> RenderError {
        match e {
            Error::Refused { refusal } => RenderError::Refused {
                target: Target::External,
                refusal,
            },
            e @ (Error::WindowsPackages { .. } | Error::Collection { .. }) => {
                RenderError::internal(Target::External, "the Ansible project can't be rendered", e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use isoloom_plan::{Refusal, Requirement};

    use super::*;

    #[test]
    fn a_refusal_reads_the_same_as_the_backend_reports_it() {
        let e = Error::Refused {
            refusal: Refusal {
                requirement: Requirement::EnvironmentProvisioning,
            },
        };
        assert_eq!(e.to_string(), RenderError::from(e.clone()).to_string());
        assert!(e.to_string().starts_with("external: "), "{e}");
    }

    #[test]
    fn a_collection_without_a_pin_is_the_backends_failure_not_a_refusal() {
        let e = Error::Collection {
            name: "ansible.posix",
            why: "the embedded catalogue's collections.yml has none".into(),
        };
        assert_eq!(
            e.to_string(),
            "external: the collection `ansible.posix` a firewall step needs has no pin: the embedded catalogue's collections.yml has none"
        );
        assert!(matches!(RenderError::from(e), RenderError::Internal { .. }));
    }
}
