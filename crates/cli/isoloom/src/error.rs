//! Why a command couldn't do its job: one error for the whole command line. A library's error
//! is kept whole as the cause (`#[source]`), so the cause chain is never lost; how it reads on
//! stderr is [`crate::diagnostic`], and the exit code each one gives is [`crate::exit`].

use std::path::PathBuf;

use isoloom_spec::syntax::Problem;

/// What stops a command.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
    /// The spec has problems: each one is shown with its field, reason and fix.
    #[error("fix the spec first (`isoloom validate`)")]
    InvalidSpec {
        /// The spec file the problems are placed in (`isoloom.yml`).
        file: String,
        /// The problems, in file order.
        problems: Vec<Problem>,
    },
    /// The spec can't be found or read.
    #[error(transparent)]
    Spec(isoloom_spec::LoadError),
    /// The profile asked for (`--profile`, `ISOLOOM_PROFILE`) can't be read, or pins another
    /// target than the one asked for.
    #[error("{0}")]
    Profile(String),
    /// The defaults in effect can't be read (a defaults file, the image table, a `-s` value).
    #[error("{0}")]
    Defaults(String),
    /// The instance asked for (`--instance`) can't be made of the spec.
    #[error("instance {number}: {reason}")]
    Instance {
        /// The instance's number.
        number: u8,
        /// Why it can't be made.
        reason: String,
    },
    /// No target has this id.
    #[error("unknown target `{0}`")]
    UnknownTarget(String),
    /// A target's files, the message or a report can't be made.
    #[error(transparent)]
    Targets(isoloom_targets::Error),
    /// A diagram format `isoloom graph` doesn't write.
    #[error("unknown format `{0}`; `d2` or `dot`")]
    UnknownFormat(String),
    /// A dotted path points at nothing in the resolved snapshot.
    #[error("nothing at `{0}` in the snapshot; try `isoloom inspect` to see it all")]
    NothingAt(String),
    /// A file or folder can't be read.
    #[error("can't read {}", path.display())]
    Read {
        /// The file or folder.
        path: PathBuf,
        /// The system's reason.
        #[source]
        source: std::io::Error,
    },
    /// A file or folder can't be written.
    #[error("can't write {}", path.display())]
    Write {
        /// The file or folder.
        path: PathBuf,
        /// The system's reason.
        #[source]
        source: std::io::Error,
    },
    /// The command's own output (stdout or stderr) can't be written.
    #[error("can't write the output")]
    Output(#[source] std::io::Error),
    /// A value can't be written as JSON or YAML.
    #[error("can't write {what}")]
    Serialize {
        /// What was being written.
        what: &'static str,
        /// The writer's reason.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// A tool the command runs (`d2`, `dot`, Ruby) isn't installed: what to install, and what
    /// was kept.
    #[error("`{program}` isn't installed, so {what}; see {install}")]
    ToolMissing {
        /// The program.
        program: String,
        /// What isn't done, and what was kept.
        what: String,
        /// Where to get it.
        install: &'static str,
    },
    /// A tool the command runs failed.
    #[error("`{program}` failed: {what}")]
    ToolFailed {
        /// The program.
        program: String,
        /// What it was doing, and what it said.
        what: String,
    },
    /// A tool the command runs couldn't be started.
    #[error("`{program}` couldn't start")]
    ToolStart {
        /// The program.
        program: String,
        /// The system's reason.
        #[source]
        source: std::io::Error,
    },
    /// `isoloom import` found nothing to read where it looked.
    #[error("{0}")]
    NothingToImport(String),
    /// The folder already has a spec, which an import doesn't replace unless told to.
    #[error("{} already has a spec; use --force to replace it, or --stdout", .0.display())]
    SpecExists(PathBuf),
    /// The source can't be drafted from.
    #[error(transparent)]
    Import(isoloom_import::Error),
}

impl From<isoloom_spec::LoadError> for CliError {
    /// A spec that doesn't load: its problems when it is invalid, else the reason it can't be
    /// read.
    fn from(e: isoloom_spec::LoadError) -> CliError {
        match e {
            isoloom_spec::LoadError::Invalid { file, problems } => CliError::InvalidSpec { file, problems },
            other => CliError::Spec(other),
        }
    }
}

impl From<isoloom_import::Error> for CliError {
    fn from(e: isoloom_import::Error) -> CliError {
        CliError::Import(e)
    }
}

impl From<isoloom_targets::Error> for CliError {
    fn from(e: isoloom_targets::Error) -> CliError {
        CliError::Targets(e)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(e: serde_json::Error) -> CliError {
        CliError::Serialize {
            what: "JSON",
            source: Box::new(e),
        }
    }
}

impl From<serde_yaml_ng::Error> for CliError {
    fn from(e: serde_yaml_ng::Error) -> CliError {
        CliError::Serialize {
            what: "YAML",
            source: Box::new(e),
        }
    }
}

impl CliError {
    /// A closure turning an I/O error on `path` into a read error, for `map_err`.
    pub(crate) fn reading(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> CliError {
        let path = path.into();
        move |source| CliError::Read { path, source }
    }

    /// A closure turning an I/O error on `path` into a write error, for `map_err`.
    pub(crate) fn writing(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> CliError {
        let path = path.into();
        move |source| CliError::Write { path, source }
    }

    /// Whether this is stdout or stderr closed by its reader (`isoloom schema | head`): the
    /// reader stopped reading, which isn't the command failing.
    pub(crate) fn is_closed_output(&self) -> bool {
        matches!(self, CliError::Output(e) if e.kind() == std::io::ErrorKind::BrokenPipe)
    }
}

/// What a command returns: its verdict, or the error that stopped it.
pub(crate) type Result<T, E = CliError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;

    #[test]
    fn each_error_names_what_to_fix() {
        assert_eq!(CliError::UnknownTarget("bogus".into()).to_string(), "unknown target `bogus`");
        assert_eq!(CliError::UnknownFormat("svg".into()).to_string(), "unknown format `svg`; `d2` or `dot`");
        assert_eq!(
            CliError::SpecExists(PathBuf::from("lab")).to_string(),
            "lab already has a spec; use --force to replace it, or --stdout"
        );
    }

    #[test]
    fn an_invalid_spec_keeps_its_problems_and_a_missing_one_its_reason() {
        let invalid = isoloom_spec::LoadError::Invalid {
            file: "isoloom.yml".into(),
            problems: vec![],
        };
        assert!(matches!(CliError::from(invalid), CliError::InvalidSpec { .. }));
        let missing = CliError::from(isoloom_spec::LoadError::NotFound(PathBuf::from("lab")));
        assert_eq!(missing.to_string(), "no isoloom.yml in lab");
    }

    #[test]
    fn an_io_error_is_the_cause_of_the_read_that_failed() {
        let e = CliError::reading("compose.yaml")(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert_eq!(e.to_string(), "can't read compose.yaml");
        assert!(e.source().is_some());
    }
}
