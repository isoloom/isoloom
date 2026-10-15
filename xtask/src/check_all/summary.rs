//! The table printed at the end: every host's gates, their results and times, then the end of
//! each failed gate's log.

use std::fmt::Write as _;
use std::path::Path;

use super::Host;
use super::protocol::{GateResult, Status};

/// One host's part, done.
pub struct HostRun {
    /// The host.
    pub host: Host,
    /// Its gates' outcomes.
    pub results: Vec<GateResult>,
    /// How long it took.
    pub secs: u64,
}

impl HostRun {
    /// A host whose part stopped unexpectedly.
    pub fn lost(host: Host) -> HostRun {
        HostRun {
            host,
            results: vec![GateResult {
                repo: "-".into(),
                gate: "-".into(),
                status: Status::Fail,
                secs: 0,
                note: "its part stopped unexpectedly".into(),
            }],
            secs: 0,
        }
    }
}

/// `m:ss`.
pub fn duration(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// The summary table, with the logs folder last.
pub fn table(runs: &[HostRun], logs: &Path) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "\n{:<12} {:<9} {:<15} {:<12} {:>6}  NOTE", "HOST", "REPO", "GATE", "RESULT", "TIME");
    for run in runs {
        for r in &run.results {
            let line = format!(
                "{:<12} {:<9} {:<15} {:<12} {:>6}  {}",
                run.host.id(),
                r.repo,
                r.gate,
                r.status.text(),
                duration(r.secs),
                r.note
            );
            let _ = writeln!(out, "{}", line.trim_end());
        }
        let _ = writeln!(out, "{:<12} {:<9} {:<15} {:<12} {:>6}", run.host.id(), "total", "", "", duration(run.secs));
    }
    let _ = writeln!(out, "logs: {}", logs.display());
    out
}

/// Whether any gate failed.
pub fn any_failed(runs: &[HostRun]) -> bool {
    runs.iter().flat_map(|r| &r.results).any(|r| r.status == Status::Fail)
}

/// The last 20 lines of each failed gate's log.
pub fn failure_tails(runs: &[HostRun], logs: &Path) -> String {
    let mut out = String::new();
    for run in runs {
        for r in run.results.iter().filter(|r| r.status == Status::Fail) {
            let name = if r.repo == "-" {
                format!("{}.log", run.host.id())
            } else {
                format!("{}-{}-{}.log", run.host.id(), r.repo, r.gate)
            };
            let Ok(text) = std::fs::read_to_string(logs.join(&name)) else { continue };
            let lines: Vec<&str> = text.lines().collect();
            let _ = writeln!(out, "\n--- {} (last 20 lines of {})", name.trim_end_matches(".log"), logs.join(&name).display());
            for l in &lines[lines.len().saturating_sub(20)..] {
                let _ = writeln!(out, "{l}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(gate: &str, status: Status) -> GateResult {
        GateResult {
            repo: "isoloom".into(),
            gate: gate.into(),
            status,
            secs: 75,
            note: String::new(),
        }
    }

    #[test]
    fn the_table_lists_each_gate_then_the_hosts_total() {
        let runs = [HostRun {
            host: Host::Mac,
            results: vec![result("fmt", Status::Pass), result("test", Status::Fail)],
            secs: 125,
        }];
        let t = table(&runs, Path::new("/logs"));
        assert!(t.contains("mac          isoloom   fmt             pass           1:15"), "{t}");
        assert!(t.contains("mac          total                                    2:05"), "{t}");
        assert!(any_failed(&runs));
        assert!(!any_failed(&[HostRun::lost(Host::Linux)][..0]));
    }
}
