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
}
