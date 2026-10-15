//! The command line's own errors: what it refuses itself, around the libraries' errors (which
//! reach `main` as they are). Every error prints as `error: <message>`, with exit code 2.

/// What the command line refuses before or after a library ran.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// The profile asked for can't be read (`--profile`, `ISOLOOM_PROFILE`).
    #[error("{0}")]
    Profile(String),
    /// The spec has problems, printed above the error.
    #[error("fix the spec first (`isoloom validate`)")]
    SpecInvalid,
    /// No target has this id.
    #[error("unknown target `{0}`")]
    UnknownTarget(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_error_names_what_to_fix() {
        assert_eq!(CliError::UnknownTarget("bogus".into()).to_string(), "unknown target `bogus`");
        assert_eq!(CliError::SpecInvalid.to_string(), "fix the spec first (`isoloom validate`)");
        assert_eq!(CliError::Profile("no profile `x`".into()).to_string(), "no profile `x`");
    }
}
