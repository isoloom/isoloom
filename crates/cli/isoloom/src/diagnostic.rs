//! How an error reads on stderr, in one format for every command: a spec's problems as an
//! author reads them (each with its field, reason and fix), then `error: ` and the error, then
//! every cause under it, one `caused by:` line each, so a library's reason is never lost.
//!
//! ```text
//! ✗ error[invalid-value] networks.app.cidr  (isoloom.yml:4:9)
//!   `10.20.0.0/33` isn't a network address like 10.20.0.0/24
//! error: fix the spec first (`isoloom validate`)
//! ```
//!
//! ```text
//! error: can't read compose.yaml
//!   caused by: No such file or directory (os error 2)
//! ```

use std::error::Error;

use crate::error::CliError;

/// The error as stderr shows it, ending with a line break; nothing for output its reader closed,
/// since whoever stopped reading knows why.
pub(crate) fn render(e: &CliError) -> String {
    if e.is_closed_output() {
        return String::new();
    }
    let problems: Vec<String> = match e {
        CliError::InvalidSpec { file, problems } => problems.iter().map(|p| format!("✗ {}", p.render(file))).collect(),
        _ => Vec::new(),
    };
    let error = format!("error: {}", indented(&e.to_string()));
    let causes = causes(e).into_iter().map(|cause| format!("  caused by: {}", indented(&cause)));
    let lines: Vec<String> = problems.into_iter().chain(std::iter::once(error)).chain(causes).collect();
    format!("{}\n", lines.join("\n"))
}

/// The causes under `e`, outermost first; a cause whose text the error above it already shows
/// (a library that writes its source into its own message) is shown once.
fn causes(e: &dyn Error) -> Vec<String> {
    let mut shown = e.to_string();
    let mut out = Vec::new();
    let mut next = e.source();
    while let Some(cause) = next {
        let text = cause.to_string();
        if !shown.contains(&text) {
            out.push(text.clone());
        }
        shown = text;
        next = cause.source();
    }
    out
}

/// A message of several lines, its later lines indented under its first.
fn indented(text: &str) -> String {
    text.trim_end().replace('\n', "\n    ")
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use isoloom_spec::syntax::{Code, FieldPath, Problem};

    use super::*;

    /// An error with a message and, maybe, a cause.
    #[derive(Debug)]
    struct Layer(&'static str, Option<Box<Layer>>);

    impl fmt::Display for Layer {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.0)
        }
    }

    impl Error for Layer {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            let cause: &(dyn Error + 'static) = self.1.as_deref()?;
            Some(cause)
        }
    }

    #[test]
    fn every_cause_is_shown_under_the_error() {
        let e = CliError::Read {
            path: "compose.yaml".into(),
            source: std::io::Error::other(Layer("the disk is gone", Some(Box::new(Layer("cable", None))))),
        };
        assert_eq!(
            render(&e),
            "error: can't read compose.yaml\n  caused by: the disk is gone\n  caused by: cable\n"
        );
    }

    #[test]
    fn a_cause_the_message_already_shows_is_shown_once() {
        let e = Layer(
            "can't read lab/isoloom.yml: denied",
            Some(Box::new(Layer("denied", Some(Box::new(Layer("by policy", None)))))),
        );
        assert_eq!(causes(&e), ["by policy"]);
    }

    #[test]
    fn a_spec_with_problems_shows_each_with_its_field_reason_and_fix() {
        let problem = Problem::new(Code::RemovedField, FieldPath::new("machines.web.docker"), "`docker:` is gone").fix("`app: { image: <ref> }`");
        let e = CliError::InvalidSpec {
            file: "isoloom.yml".to_owned(),
            problems: vec![problem],
        };
        assert_eq!(
            render(&e),
            "✗ error[removed-field] machines.web.docker\n  `docker:` is gone\n  fix: `app: { image: <ref> }`\nerror: fix the spec first (`isoloom validate`)\n"
        );
    }

    #[test]
    fn output_its_reader_closed_says_nothing() {
        let closed = CliError::Output(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
        assert_eq!(render(&closed), "");
        let full = CliError::Output(std::io::Error::other("no space left on device"));
        assert_eq!(render(&full), "error: can't write the output\n  caused by: no space left on device\n");
    }

    #[test]
    fn a_message_of_several_lines_stays_under_its_error() {
        let e = CliError::Profile("profile `x`:\nno machine `wbe`".to_owned());
        assert_eq!(render(&e), "error: profile `x`:\n    no machine `wbe`\n");
    }
}
