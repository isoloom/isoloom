//! The command line's exit codes, worked out in one place: 0 when a command did its job and its
//! answer is yes; 1 when its answer is no (the spec has problems, generated files are out of
//! date, the target asked for refuses the spec, a profile can't run); 2 when it couldn't do its
//! job (a file it can't read, a name it doesn't know, a tool that isn't installed). A usage
//! error is clap's, also 2. Output whose reader stopped reading (`isoloom schema | head`) ends
//! the command quietly, with 0.

use std::process::ExitCode;

use crate::error::{CliError, Result};

/// What a command that ran to its end found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Yes: valid, up to date, generated.
    Pass,
    /// No, and the command's output says why: problems found, files out of date, a target
    /// refused.
    Fail,
}

impl Verdict {
    /// [`Verdict::Pass`] when `ok`, else [`Verdict::Fail`].
    pub(crate) fn of(ok: bool) -> Verdict {
        if ok { Verdict::Pass } else { Verdict::Fail }
    }
}

/// The exit code a command's outcome gives.
pub(crate) fn code(outcome: &Result<Verdict>) -> ExitCode {
    ExitCode::from(match outcome {
        Ok(Verdict::Pass) => 0,
        Err(e) if e.is_closed_output() => 0,
        // A spec with problems is an answer about the spec, as `isoloom validate` gives it.
        Ok(Verdict::Fail) | Err(CliError::InvalidSpec { .. }) => 1,
        Err(_) => 2,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_no_and_a_spec_with_problems_are_1_and_a_command_that_couldnt_run_is_2() {
        assert_eq!(code(&Ok(Verdict::Pass)), ExitCode::from(0));
        assert_eq!(code(&Ok(Verdict::Fail)), ExitCode::from(1));
        let invalid = CliError::InvalidSpec {
            file: "isoloom.yml".to_owned(),
            problems: vec![],
        };
        assert_eq!(code(&Err(invalid)), ExitCode::from(1));
        assert_eq!(code(&Err(CliError::UnknownTarget("x".to_owned()))), ExitCode::from(2));
    }

    #[test]
    fn output_its_reader_closed_is_0_and_output_that_cant_be_written_is_2() {
        let closed = CliError::Output(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
        assert_eq!(code(&Err(closed)), ExitCode::from(0));
        let full = CliError::Output(std::io::Error::other("no space left on device"));
        assert_eq!(code(&Err(full)), ExitCode::from(2));
    }
}
