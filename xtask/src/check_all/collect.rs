//! Where a host's gates go: printed as the stream protocol ([`Printer`], a remote host's part),
//! or into the logs folder and the summary ([`Collector`], the machine that started the run).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::Host;
use super::protocol::{self, Event, GateResult, Status};

/// What a host's part reports.
pub(super) trait Sink {
    /// A gate starts.
    fn begin(&mut self, repo: &str, gate: &str);
    /// A line of its output.
    fn line(&mut self, line: &str);
    /// It ended.
    fn result(&mut self, result: GateResult);
}

/// Prints the stream protocol on standard output, for the machine that started the run.
pub(super) struct Printer;

impl Sink for Printer {
    fn begin(&mut self, repo: &str, gate: &str) {
        println!("{}", protocol::begin_line(repo, gate));
    }

    fn line(&mut self, line: &str) {
        println!("{line}");
    }

    fn result(&mut self, result: GateResult) {
        println!("{}", protocol::result_line(&result));
    }
}

/// One host's results, each gate's output in its own log (`<host>-<repo>-<gate>.log`), the rest
/// in the host's (`<host>.log`), and a progress line per gate on the terminal.
pub(super) struct Collector {
    host: Host,
    logs: PathBuf,
    log: Option<File>,
    /// The gates' outcomes, in order.
    pub results: Vec<GateResult>,
}

impl Collector {
    /// A collector writing into the `logs` folder.
    pub(super) fn new(host: Host, logs: &Path) -> Collector {
        Collector {
            host,
            logs: logs.to_path_buf(),
            log: None,
            results: Vec::new(),
        }
    }

    /// A line of a host's stream: a gate's start or end, or output.
    pub(super) fn feed(&mut self, raw: &str) {
        match protocol::parse(raw) {
            Event::Begin { repo, gate } => self.begin(&repo, &gate),
            Event::Result(r) => self.result(r),
            Event::Line(l) => self.line(&l),
        }
    }

    /// The host couldn't run at all: one row for it.
    pub(super) fn host_failed(&mut self, status: Status, note: &str) {
        println!("  {:<12} {}: {note}", self.host.id(), status.text());
        self.results.push(GateResult {
            repo: "-".into(),
            gate: "-".into(),
            status,
            secs: 0,
            note: note.into(),
        });
    }

    /// The host it collects for.
    pub(super) fn host(&self) -> Host {
        self.host
    }

    /// The host's own log.
    pub(super) fn host_log(&self) -> PathBuf {
        self.logs.join(format!("{}.log", self.host.id()))
    }

    fn write(&mut self, line: &str) {
        if self.log.is_none() {
            self.log = OpenOptions::new().create(true).append(true).open(self.host_log()).ok();
        }
        // A log that can't be written is dropped; the results still reach the summary.
        if let Some(f) = &mut self.log
            && writeln!(f, "{line}").is_err()
        {
            self.log = None;
        }
    }
}

impl Sink for Collector {
    fn begin(&mut self, repo: &str, gate: &str) {
        let path = self.logs.join(format!("{}-{repo}-{gate}.log", self.host.id()));
        self.log = File::create(path).ok();
    }

    fn line(&mut self, line: &str) {
        self.write(line);
    }

    fn result(&mut self, result: GateResult) {
        println!(
            "  {:<12} {:<9} {:<15} {:<5} {}",
            self.host.id(),
            result.repo,
            result.gate,
            result.status.text(),
            super::summary::duration(result.secs)
        );
        self.results.push(result);
        self.log = None;
    }
}
