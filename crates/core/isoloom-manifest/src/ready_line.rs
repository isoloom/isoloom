//! The first line of a machine's ready file, written and read back: the `isoloom-ready/1`
//! grammar ([`ReadyGrammar::V1`](crate::ReadyGrammar::V1)).

use std::fmt;

/// The line of a set-up that is done.
const READY: &str = "ready";
/// What starts the line of a step that is running, before its name.
const RUNNING: &str = "running: ";
/// What starts the line of a step that failed, before its name.
const FAILED: &str = "failed: ";

/// The first line of a ready file, as the `isoloom-ready/1` grammar defines it
/// ([`ReadyGrammar::V1`](crate::ReadyGrammar::V1)): the set-up is done, or at a step, or stopped
/// at one. Read with [`ReadyLine::parse`], written with `Display`. No file, or any other line,
/// means the machine is still booting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadyLine {
    /// The set-up is done: `ready`.
    Ready,
    /// A step is running: `running: <step>`.
    Running {
        /// What the set-up calls the step.
        step: String,
    },
    /// A step failed, and the set-up stopped there: `failed: <step>`.
    Failed {
        /// What the set-up calls the step.
        step: String,
    },
}

impl ReadyLine {
    /// Reads a ready file's first line; `None` when it is none of the grammar's (still booting).
    ///
    /// # Example
    ///
    /// ```
    /// use isoloom_manifest::ReadyLine;
    ///
    /// assert_eq!(ReadyLine::parse("ready\n"), Some(ReadyLine::Ready));
    /// assert_eq!(ReadyLine::parse("failed: setup.sh"), Some(ReadyLine::Failed { step: "setup.sh".into() }));
    /// assert_eq!(ReadyLine::parse("booting"), None);
    /// ```
    #[must_use]
    pub fn parse(line: &str) -> Option<ReadyLine> {
        let line = line.trim_end();
        if line == READY {
            return Some(ReadyLine::Ready);
        }
        if let Some(step) = line.strip_prefix(RUNNING) {
            return Some(ReadyLine::Running { step: step.to_owned() });
        }
        line.strip_prefix(FAILED).map(|step| ReadyLine::Failed { step: step.to_owned() })
    }

    /// The step `step` running.
    #[must_use]
    pub fn running(step: &str) -> ReadyLine {
        ReadyLine::Running { step: step.to_owned() }
    }

    /// The step `step` failed.
    #[must_use]
    pub fn failed(step: &str) -> ReadyLine {
        ReadyLine::Failed { step: step.to_owned() }
    }
}

impl fmt::Display for ReadyLine {
    /// The line as a set-up writes it, which [`ReadyLine::parse`] reads back.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadyLine::Ready => f.write_str(READY),
            ReadyLine::Running { step } => write!(f, "{RUNNING}{step}"),
            ReadyLine::Failed { step } => write!(f, "{FAILED}{step}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ReadyLine;

    #[test]
    fn every_line_reads_back_as_written() {
        for line in [ReadyLine::Ready, ReadyLine::running("its configuration"), ReadyLine::failed("a: b.sh")] {
            assert_eq!(ReadyLine::parse(&line.to_string()), Some(line));
        }
        assert_eq!(ReadyLine::Ready.to_string(), "ready");
        assert_eq!(ReadyLine::running("x").to_string(), "running: x");
        assert_eq!(ReadyLine::failed("x").to_string(), "failed: x");
    }

    #[test]
    fn any_other_line_is_still_booting() {
        for line in ["", "readyish", "running:x", "Ready"] {
            assert_eq!(ReadyLine::parse(line), None, "{line}");
        }
    }
}
