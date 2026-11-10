//! The lines a check runner prints, written and read back: the `isoloom-check/1` protocol
//! ([`CheckProtocol::V1`](crate::CheckProtocol::V1)).

use std::fmt;

/// What marks a runner's line, wherever it starts in the line a tool prints.
const MARKER: &str = "isoloom-check: ";
/// What starts a passed check's line, after the marker.
const PASS: &str = "PASS ";
/// What starts a failed check's line, after the marker.
const FAIL: &str = "FAIL ";
/// What ends a failed check's name, before its reason.
const REASON: &str = ": ";
/// What starts the runner's last line, after the marker.
const END: &str = "END ";

/// One line of a check runner's output, as the `isoloom-check/1` protocol defines it
/// ([`CheckProtocol::V1`](crate::CheckProtocol::V1)). Read with [`CheckLine::parse`], written
/// with `Display`, or by a runner in another language from [`CheckLine::pass_line`],
/// [`CheckLine::fail_line`] and [`CheckLine::end_line`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckLine {
    /// A check passed: `isoloom-check: PASS <name>`.
    Pass {
        /// The check's name.
        name: String,
    },
    /// A check failed: `isoloom-check: FAIL <name>: <reason>`.
    Fail {
        /// The check's name.
        name: String,
        /// Why it failed; empty when the line gives no reason.
        reason: String,
    },
    /// The runner reached its end: `isoloom-check: END <p> passed, <f> failed`.
    End {
        /// How many checks passed, by the runner's own count.
        passed: u32,
        /// How many checks failed, by the runner's own count.
        failed: u32,
    },
}

impl CheckLine {
    /// Reads one line of a runner's output; `None` when it isn't a runner's line.
    ///
    /// The line is matched anywhere it starts: tools print their own prefix before it (Vagrant's
    /// `    web: `, Compose's `web-1  | `). A name ends at the line's end (`PASS`) or at the first
    /// `: ` (`FAIL`), trailing whitespace dropped; an `END` line is one only with both counts.
    ///
    /// # Example
    ///
    /// ```
    /// use isoloom_manifest::CheckLine;
    ///
    /// let failed = CheckLine::parse("    web: isoloom-check: FAIL db:5432 from web: nothing listens");
    /// let expected = CheckLine::Fail { name: "db:5432 from web".into(), reason: "nothing listens".into() };
    /// assert_eq!(failed, Some(expected));
    /// let end = CheckLine::parse("isoloom-check: END 3 passed, 1 failed");
    /// assert_eq!(end, Some(CheckLine::End { passed: 3, failed: 1 }));
    /// assert_eq!(CheckLine::parse("Starting checks..."), None);
    /// ```
    #[must_use]
    pub fn parse(line: &str) -> Option<CheckLine> {
        let (_, rest) = line.split_once(MARKER)?;
        if let Some(name) = rest.strip_prefix(PASS) {
            return Some(CheckLine::Pass {
                name: name.trim_end().to_owned(),
            });
        }
        if let Some(r) = rest.strip_prefix(FAIL) {
            let (name, reason) = r.split_once(REASON).unwrap_or((r, ""));
            return Some(CheckLine::Fail {
                name: name.trim_end().to_owned(),
                reason: reason.trim_end().to_owned(),
            });
        }
        if let Some(r) = rest.strip_prefix(END) {
            let mut counts = r.split_whitespace().filter_map(|w| w.parse::<u32>().ok());
            return Some(CheckLine::End {
                passed: counts.next()?,
                failed: counts.next()?,
            });
        }
        None
    }

    /// A passed check's line, its name given as text: `isoloom-check: PASS <name>`. A runner
    /// written in another language puts its own expression there (a shell's `$1`).
    #[must_use]
    pub fn pass_line(name: &str) -> String {
        format!("{MARKER}{PASS}{name}")
    }

    /// A failed check's line, its name and reason given as text:
    /// `isoloom-check: FAIL <name>: <reason>`.
    #[must_use]
    pub fn fail_line(name: &str, reason: &str) -> String {
        format!("{MARKER}{FAIL}{name}{REASON}{reason}")
    }

    /// The runner's last line, its counts given as text:
    /// `isoloom-check: END <passed> passed, <failed> failed`.
    #[must_use]
    pub fn end_line(passed: &str, failed: &str) -> String {
        format!("{MARKER}{END}{passed} passed, {failed} failed")
    }
}

impl fmt::Display for CheckLine {
    /// The line as a runner prints it, which [`CheckLine::parse`] reads back.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let line = match self {
            CheckLine::Pass { name } => CheckLine::pass_line(name),
            CheckLine::Fail { name, reason } => CheckLine::fail_line(name, reason),
            CheckLine::End { passed, failed } => CheckLine::end_line(&passed.to_string(), &failed.to_string()),
        };
        f.write_str(&line)
    }
}

#[cfg(test)]
mod tests;
