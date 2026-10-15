//! The command line's errors as JSON: a [`Failure`] carries its stable code (and the machine,
//! a hint) to the error document `--json` prints; other errors get their code from their type.
//! The exit code is the same for every error (2, in `main`).

use isoloom_runtime::events::{ErrorBody, ErrorCode, FieldProblem};

/// An error with its stable code. Printed as its message alone, as every error is.
#[derive(Debug)]
pub struct Failure(pub ErrorBody);

impl Failure {
    /// A failure of kind `code`.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self(ErrorBody::new(code, message))
    }

    /// With what to do about it.
    #[must_use]
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.0.hint = Some(hint.into());
        self
    }

    /// About this machine.
    #[must_use]
    pub fn machine(mut self, machine: &str) -> Self {
        self.0.machine = Some(machine.to_string());
        self
    }

    /// An invalid spec, with its problems.
    pub fn invalid(problems: &[isoloom_spec::Problem]) -> Self {
        let mut f = Self::new(ErrorCode::SpecInvalid, "fix the spec first (`isoloom validate`)");
        f.0.problems = problems
            .iter()
            .map(|p| FieldProblem {
                at: p.at.clone(),
                message: p.message.clone(),
            })
            .collect();
        f
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.message)
    }
}

impl std::error::Error for Failure {}

/// A failure of kind `code`, boxed as the commands return errors.
pub fn fail(code: ErrorCode, message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(Failure::new(code, message))
}

/// The error document's body for any error a command returns: a [`Failure`]'s own, else a code
/// from the error's type.
pub fn body_of(e: &(dyn std::error::Error + 'static)) -> ErrorBody {
    if let Some(f) = e.downcast_ref::<Failure>() {
        return f.0.clone();
    }
    let code = if let Some(l) = e.downcast_ref::<isoloom_spec::LoadError>() {
        match l {
            isoloom_spec::LoadError::Profile { .. } => ErrorCode::Profile,
            _ => ErrorCode::SpecLoad,
        }
    } else if e.downcast_ref::<isoloom_targets::GenerateError>().is_some() {
        ErrorCode::GenerateFailed
    } else if e.downcast_ref::<std::io::Error>().is_some() || e.downcast_ref::<isoloom_runtime::RegistryError>().is_some() {
        ErrorCode::Io
    } else {
        ErrorCode::Internal
    };
    ErrorBody::new(code, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_keeps_its_code_and_prints_its_message() {
        let e: Box<dyn std::error::Error> = Failure::new(ErrorCode::NotRunning, "nothing here").hint("run it").into();
        assert_eq!(e.to_string(), "nothing here");
        let body = body_of(e.as_ref());
        assert_eq!(body.code, ErrorCode::NotRunning);
        assert_eq!(body.hint.as_deref(), Some("run it"));
    }

    #[test]
    fn other_errors_get_a_code_from_their_type() {
        let io: Box<dyn std::error::Error> = Box::new(std::io::Error::other("disk"));
        assert_eq!(body_of(io.as_ref()).code, ErrorCode::Io);
        let text: Box<dyn std::error::Error> = "something".into();
        assert_eq!(body_of(text.as_ref()).code, ErrorCode::Internal);
    }
}
