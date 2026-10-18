//! Where a command writes: its output on stdout (what a script reads) and its notes on stderr
//! (what a person reads beside it). Commands write through it, so a closed pipe is an error
//! with its cause, never a panic, and a test reads what a command wrote.

use std::fmt::Display;
use std::io::Write;

use crate::error::{CliError, Result};

/// A command's two streams.
pub(crate) struct Console<'a> {
    out: &'a mut dyn Write,
    err: &'a mut dyn Write,
}

impl<'a> Console<'a> {
    /// The console writing its output to `out` and its notes to `err`.
    pub(crate) fn new(out: &'a mut dyn Write, err: &'a mut dyn Write) -> Self {
        Console { out, err }
    }

    /// `text` on the output, as it is.
    pub(crate) fn print(&mut self, text: impl Display) -> Result<()> {
        write!(self.out, "{text}").map_err(CliError::Output)
    }

    /// `text` on the output, then a line break.
    pub(crate) fn line(&mut self, text: impl Display) -> Result<()> {
        writeln!(self.out, "{text}").map_err(CliError::Output)
    }

    /// `value` on the output as pretty JSON (every command's `--json`).
    pub(crate) fn json(&mut self, value: &impl serde::Serialize) -> Result<()> {
        self.line(serde_json::to_string_pretty(value)?)
    }

    /// `text` on stderr, as it is: a note beside the output.
    pub(crate) fn note(&mut self, text: impl Display) -> Result<()> {
        write!(self.err, "{text}").map_err(CliError::Output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_and_notes_go_to_their_own_streams() -> Result<()> {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut c = Console::new(&mut out, &mut err);
        c.line("✓ web")?;
        c.json(&serde_json::json!({ "ok": true }))?;
        c.note("a note\n")?;
        assert_eq!(String::from_utf8_lossy(&out), "✓ web\n{\n  \"ok\": true\n}\n");
        assert_eq!(String::from_utf8_lossy(&err), "a note\n");
        Ok(())
    }

    #[test]
    fn a_stream_that_cant_be_written_is_an_output_error() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let (mut out, mut err) = (Closed, Vec::new());
        let e = Console::new(&mut out, &mut err).line("lost");
        assert!(matches!(e, Err(CliError::Output(_))));
    }
}
