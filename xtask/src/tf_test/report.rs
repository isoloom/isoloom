//! The verdicts and the report. A run passes when its assertions hold. A run named
//! `known_bug_<what>` records a bug a generated module has today: its assertions say what the
//! spec wants, and it is expected to fail (an assertion, not an error); once it passes, the bug is
//! fixed and the run must lose its prefix to guard the fix, so a pass fails the report.

use super::results::{Diagnostic, ModuleResult, RunResult};
use super::suite::Suite;

/// The prefix of a run that records a known bug.
pub(crate) const KNOWN_BUG: &str = "known_bug_";

/// The lines of a planned value shown before it is cut.
const VALUE_LINES: usize = 12;

/// A run's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Its assertions hold.
    Passed,
    /// An assertion failed, or the run couldn't be evaluated.
    Failed,
    /// A known bug's run failed its assertions, as expected.
    KnownBug,
    /// A known bug's run passed: the bug is fixed, the prefix must go.
    Fixed,
}

/// The verdict on one run.
#[must_use]
pub(crate) fn verdict(run: &RunResult) -> Verdict {
    match (run.name.starts_with(KNOWN_BUG), run.status.as_str()) {
        (false, "pass") => Verdict::Passed,
        (true, "fail") => Verdict::KnownBug,
        (true, "pass") => Verdict::Fixed,
        _ => Verdict::Failed,
    }
}

/// What the report counted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tally {
    /// Runs whose assertions hold.
    pub(crate) passed: usize,
    /// Runs or suites that failed (a fixed known bug included).
    pub(crate) failed: usize,
    /// Known bugs, failing as expected.
    pub(crate) known: usize,
}

/// The report: a line per suite, then each failure and known bug with where it is and the planned
/// values it read; and the counts.
#[must_use]
pub(crate) fn report(suites: &[(Suite, String)], results: &[ModuleResult]) -> (String, Tally) {
    let (mut table, mut details, mut tally) = (Vec::new(), Vec::new(), Tally::default());
    for (suite, file) in suites {
        let Some(result) = results.iter().find(|r| r.module == suite.module) else {
            tally.failed += 1;
            table.push(format!("FAIL  {:<20} {:<22} no result", suite.example, suite.target));
            continue;
        };
        let before = tally;
        suite_details(&mut details, (suite, file), result, &mut tally);
        let status = if tally.failed > before.failed { "FAIL" } else { "ok" };
        let known = tally.known - before.known;
        let known = if known > 0 {
            format!(", {known} known bug{}", if known == 1 { "" } else { "s" })
        } else {
            String::new()
        };
        table.push(format!(
            "{status:<5} {:<20} {:<22} {} runs{known}",
            suite.example,
            suite.target,
            result.runs.len()
        ));
    }
    table.extend(details);
    (table.join("\n") + "\n", tally)
}

fn suite_details(out: &mut Vec<String>, (suite, file): (&Suite, &str), result: &ModuleResult, tally: &mut Tally) {
    let head = format!("{} {}", suite.example, suite.target);
    let unfinished: Vec<&str> = result
        .planned
        .iter()
        .filter(|(f, n)| !result.runs.iter().any(|r| &r.file == f && &r.name == n))
        .map(|(_, n)| n.as_str())
        .collect();
    if result.runs.is_empty() || !unfinished.is_empty() || crashed(result) {
        tally.failed += 1;
        out.push(format!("\nFAIL  {head}: {} (status {})", unfinished_note(result, &unfinished), result.status));
        out.extend(log_summary(&result.log).into_iter().map(|l| format!("      {l}")));
    }
    for e in &result.errors {
        tally.failed += 1;
        out.push(format!("\nFAIL  {head}: {}", e.summary));
        diagnostic(out, e, file);
    }
    for run in &result.runs {
        let label = match verdict(run) {
            Verdict::Passed => {
                tally.passed += 1;
                continue;
            }
            Verdict::Failed => {
                tally.failed += 1;
                "FAIL "
            }
            Verdict::KnownBug => {
                tally.known += 1;
                "KNOWN"
            }
            Verdict::Fixed => {
                tally.failed += 1;
                out.push(format!(
                    "\nFIXED {head} run \"{}\" passes now: drop `{KNOWN_BUG}` from its name, so it guards the fix ({file})",
                    run.name
                ));
                continue;
            }
        };
        out.push(format!("\n{label} {head} run \"{}\" ({})", run.name, run.status));
        for d in &run.diagnostics {
            diagnostic(out, d, file);
        }
    }
}

/// Whether Terraform crashed or ended otherwise than with its tests' verdict (0: every run
/// passed, 1: one didn't), even after every run finished: a panic while it reported the last.
fn crashed(result: &ModuleResult) -> bool {
    !matches!(result.status.as_str(), "0" | "1") || result.log.iter().any(|l| l.starts_with("panic:"))
}

fn unfinished_note(result: &ModuleResult, unfinished: &[&str]) -> String {
    if result.runs.is_empty() {
        "no run finished".to_owned()
    } else if unfinished.is_empty() {
        "Terraform crashed after its last run".to_owned()
    } else {
        format!("{} run(s) never finished: {}", unfinished.len(), unfinished.join(", "))
    }
}

/// What explains a module whose runs didn't all finish: Terraform's errors and panics, else the
/// end of what it said.
fn log_summary(log: &[String]) -> Vec<&str> {
    let key: Vec<&str> = log
        .iter()
        .map(String::as_str)
        .filter(|l| l.starts_with("Error:") || l.starts_with("panic:"))
        .take(10)
        .collect();
    if key.is_empty() {
        log.iter().rev().take(10).rev().map(String::as_str).collect()
    } else {
        key
    }
}

/// A diagnostic: the assertion's message, where it is, and what was planned: both sides of a
/// comparison (the suites write the planned one first), else each value the expression read.
fn diagnostic(out: &mut Vec<String>, d: &Diagnostic, file: &str) {
    let message = if d.detail.is_empty() { &d.summary } else { &d.detail };
    let at = d.line.map_or_else(|| file.to_owned(), |l| format!("{file}:{l}"));
    out.push(format!("      {} ({at})", one_line(message)));
    match d.sides.as_ref().filter(|(l, r)| telling(l, r)) {
        Some((planned, wanted)) => {
            value(out, "planned:", planned);
            value(out, "wanted: ", wanted);
            if !d.note.is_empty() {
                let collections = [planned, wanted].iter().all(|s| s.starts_with('[') || s.starts_with('{'));
                let hint = if collections {
                    " (a list against a tuple: wrap both sides in `tolist` or `sort`)"
                } else {
                    ""
                };
                out.push(format!("      note:    {}{hint}", d.note));
            }
        }
        None => {
            for (what, statement) in &d.values {
                value(out, "planned:", &format!("{what} {statement}"));
            }
        }
    }
}

/// Whether a comparison's two sides say more than the values it read: not two booleans (the
/// sides of an `&&` or `||`), nor two equal sides (the comparison wasn't `==`: `length(...) > 0`).
fn telling(left: &str, right: &str) -> bool {
    let boolean = |s: &str| matches!(s, "true" | "false");
    !(boolean(left) && boolean(right)) && left != right
}

/// A value under its label, cut after [`VALUE_LINES`] lines.
fn value(out: &mut Vec<String>, label: &str, text: &str) {
    let lines: Vec<&str> = text.lines().collect();
    out.push(format!("      {label} {}", lines.first().unwrap_or(&"")));
    out.extend(lines.iter().skip(1).take(VALUE_LINES).map(|l| format!("               {l}")));
    if lines.len() > VALUE_LINES + 1 {
        out.push(format!("               ... ({} more lines)", lines.len() - VALUE_LINES - 1));
    }
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(name: &str, status: &str) -> RunResult {
        RunResult {
            file: "tests/segmented.tftest.hcl".into(),
            name: name.into(),
            status: status.into(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn a_known_bug_is_expected_to_fail_an_assertion() {
        assert_eq!(verdict(&run("routing", "pass")), Verdict::Passed);
        assert_eq!(verdict(&run("routing", "fail")), Verdict::Failed);
        assert_eq!(verdict(&run("known_bug_open_vnet", "fail")), Verdict::KnownBug);
        assert_eq!(verdict(&run("known_bug_open_vnet", "pass")), Verdict::Fixed);
        assert_eq!(verdict(&run("known_bug_open_vnet", "error")), Verdict::Failed, "an error isn't the bug");
    }

    fn suite() -> (Suite, String) {
        let s = Suite {
            file: "x".into(),
            example: "segmented".into(),
            target: "cloud-vm/aws".into(),
            module: "segmented/.isoloom/cloud-vm/aws".into(),
        };
        (s, "tests/terraform/cloud-vm/aws/segmented.tftest.hcl".into())
    }

    #[test]
    fn a_failure_names_the_example_target_assertion_and_planned_value() {
        let mut failed = run("routing", "fail");
        failed.diagnostics.push(Diagnostic {
            summary: "Test assertion failed".into(),
            detail: "back has no default route".into(),
            line: Some(16),
            values: vec![("aws_route_table.back.route".into(), "is [\n  \"0.0.0.0/0\"\n]".into())],
            sides: None,
            note: String::new(),
        });
        let result = ModuleResult {
            module: "segmented/.isoloom/cloud-vm/aws".into(),
            status: "1".into(),
            runs: vec![run("firewall", "pass"), failed, run("known_bug_x", "fail")],
            ..ModuleResult::default()
        };
        let (text, tally) = report(&[suite()], &[result]);
        assert_eq!(
            tally,
            Tally {
                passed: 1,
                failed: 1,
                known: 1
            }
        );
        assert!(text.contains("FAIL  segmented cloud-vm/aws run \"routing\""), "{text}");
        assert!(
            text.contains("back has no default route (tests/terraform/cloud-vm/aws/segmented.tftest.hcl:16)"),
            "{text}"
        );
        assert!(
            text.contains("planned: aws_route_table.back.route is [\n                 \"0.0.0.0/0\""),
            "{text}"
        );
        assert!(text.contains("KNOWN segmented cloud-vm/aws run \"known_bug_x\""), "{text}");
    }

    #[test]
    fn only_a_comparison_whose_sides_differ_shows_them() {
        assert!(telling("\"10.0.0.0/24\"", "null"));
        assert!(!telling("true", "false"), "the sides of an &&");
        assert!(!telling("0", "0"), "`length(...) > 0`: the values read say more");
    }

    #[test]
    fn a_suite_without_results_fails() {
        let (text, tally) = report(&[suite()], &[]);
        assert_eq!(tally.failed, 1);
        assert!(text.contains("no result"), "{text}");
    }

    #[test]
    fn a_module_whose_init_failed_fails_with_what_init_said() {
        let init = ModuleResult {
            module: "segmented/.isoloom/cloud-vm/aws".into(),
            status: "init-failed".into(),
            log: vec!["Error: provider missing".into()],
            ..ModuleResult::default()
        };
        let (text, tally) = report(&[suite()], &[init]);
        assert_eq!(tally.failed, 1);
        assert!(
            text.contains("no run finished (status init-failed)") && text.contains("Error: provider missing"),
            "{text}"
        );
    }

    /// A module Terraform crashed in (status 11, a panic) after `routing` passed, its file
    /// holding the runs `planned`.
    fn crashed(planned: &[&str]) -> ModuleResult {
        ModuleResult {
            module: "segmented/.isoloom/cloud-vm/aws".into(),
            status: "11".into(),
            runs: vec![run("routing", "pass")],
            planned: planned.iter().map(|n| ("tests/segmented.tftest.hcl".to_owned(), (*n).to_owned())).collect(),
            log: vec!["panic: value has marks".into(), "goroutine 95".into()],
            ..ModuleResult::default()
        }
    }

    #[test]
    fn a_crash_fails_the_runs_that_never_finished() {
        let (text, tally) = report(&[suite()], &[crashed(&["routing", "outputs"])]);
        assert_eq!((tally.passed, tally.failed), (1, 1));
        assert!(
            text.contains("1 run(s) never finished: outputs (status 11)") && text.contains("panic: value has marks"),
            "{text}"
        );
    }

    #[test]
    fn a_crash_after_the_last_run_fails() {
        let (text, tally) = report(&[suite()], &[crashed(&["routing"])]);
        assert_eq!((tally.passed, tally.failed), (1, 1));
        assert!(text.contains("Terraform crashed after its last run (status 11)"), "{text}");
    }
}
