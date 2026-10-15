//! How a host's part reports its gates, one line each: `@@begin<TAB>repo<TAB>gate` before a
//! gate's output, `@@result<TAB>repo<TAB>gate<TAB>status<TAB>seconds<TAB>note` after it. Every
//! other line is output. The machine that started the run reads the stream back with [`parse`].

/// How a gate ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// It passed.
    Pass,
    /// It failed, or the host couldn't run it.
    Fail,
    /// It didn't run, for a reason that isn't a failure (a missing checkout, say).
    Skip,
    /// The host couldn't be reached.
    Unreachable,
}

impl Status {
    /// As the table prints it: failures in capitals.
    pub fn text(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "FAIL",
            Status::Skip => "skip",
            Status::Unreachable => "unreachable",
        }
    }

    fn parse(s: &str) -> Option<Status> {
        [Status::Pass, Status::Fail, Status::Skip, Status::Unreachable]
            .into_iter()
            .find(|st| st.text() == s)
    }
}

/// One gate's outcome on a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateResult {
    /// The repository (`-` for the host as a whole).
    pub repo: String,
    /// The gate (`-` for the host as a whole).
    pub gate: String,
    /// How it ended.
    pub status: Status,
    /// How long it took.
    pub secs: u64,
    /// Why it failed or was skipped.
    pub note: String,
}

/// One line of a host's stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A gate starts.
    Begin {
        /// Its repository.
        repo: String,
        /// The gate.
        gate: String,
    },
    /// A gate ended.
    Result(GateResult),
    /// Output.
    Line(String),
}

/// The line that starts a gate.
pub fn begin_line(repo: &str, gate: &str) -> String {
    format!("@@begin\t{repo}\t{gate}")
}

/// The line that ends a gate.
pub fn result_line(r: &GateResult) -> String {
    format!("@@result\t{}\t{}\t{}\t{}\t{}", r.repo, r.gate, r.status.text(), r.secs, r.note)
}

/// What a line of the stream says (a carriage return at its end dropped, for Windows).
pub fn parse(line: &str) -> Event {
    let line = line.trim_end_matches('\r');
    let mut fields = line.split('\t');
    match fields.next() {
        Some("@@begin") => {
            let repo = fields.next().unwrap_or("-").to_string();
            let gate = fields.next().unwrap_or("-").to_string();
            Event::Begin { repo, gate }
        }
        Some("@@result") => {
            let parts: Vec<&str> = fields.collect();
            match parts.as_slice() {
                [repo, gate, status, secs, note @ ..] => Event::Result(GateResult {
                    repo: (*repo).to_string(),
                    gate: (*gate).to_string(),
                    status: Status::parse(status).unwrap_or(Status::Fail),
                    secs: secs.parse().unwrap_or(0),
                    note: note.join("\t"),
                }),
                _ => Event::Line(line.to_string()),
            }
        }
        _ => Event::Line(line.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_result_reads_back_as_written() {
        let r = GateResult {
            repo: "isoloom".into(),
            gate: "test".into(),
            status: Status::Fail,
            secs: 90,
            note: "exit 101".into(),
        };
        assert_eq!(parse(&result_line(&r)), Event::Result(r));
        assert_eq!(
            parse(&format!("{}\r", begin_line("launcher", "check"))),
            Event::Begin {
                repo: "launcher".into(),
                gate: "check".into()
            }
        );
    }

    #[test]
    fn anything_else_is_output() {
        assert_eq!(parse("test result: ok"), Event::Line("test result: ok".into()));
        assert_eq!(parse("@@result\tshort"), Event::Line("@@result\tshort".into()));
    }
}
