//! `cargo xtask bench`: the generation benchmarks (criterion, in
//! `crates/targets/isoloom-targets/benches/pipeline.rs`: parsing, lowering and rendering
//! representative examples, and generating every example on every target), each median
//! compared with the stored baseline, `bench-baseline.json`.
//!
//! A benchmark more than `--threshold` percent slower than its baseline fails the run; one the
//! baseline doesn't know, or one gone, is only reported. A benchmark with a budget
//! ([`BUDGETS`]: generating every example on every target) fails the run when it is over it,
//! on any host. `--update` writes today's medians as the baseline (the first run writes it
//! too). The baseline names the host it was measured on (its OS and architecture): on another
//! host the comparison is printed, never failed, since the numbers aren't comparable.
//! Criterion's reports go to `<target dir>/xtask-bench`.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::{Res, process, repo};

/// The arguments of `cargo xtask bench`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Write today's medians as the baseline instead of comparing.
    #[arg(long)]
    update: bool,
    /// How much slower than its baseline a benchmark may be, in percent.
    #[arg(long, value_name = "PERCENT", default_value_t = 25.0)]
    threshold: f64,
}

/// Each benchmark's median, in nanoseconds, by its id (`parse/hello-stack`).
type Medians = BTreeMap<String, f64>;

/// The benchmarks held to an absolute budget, in nanoseconds, on every host. Generating every
/// example on every target, parsing included, stays under 8 s: the budget set on 2026-10-15 as
/// about four times what a debug build took on the Intel Mac. The benchmarks are a release
/// build, measured as a median, so only a gross regression reaches it; the baseline comparison
/// catches the smaller ones. Raise it only with the reason in the commit.
const BUDGETS: [(&str, f64); 1] = [("generate/every-example", 8e9)];

/// One line per benchmark over its budget.
fn over_budget(now: &Medians) -> Vec<String> {
    BUDGETS
        .iter()
        .filter_map(|(id, budget)| {
            let median = now.get(*id)?;
            (median > budget).then(|| format!("{id:<28} {:>10}  over its budget of {}\n", shown(*median), shown(*budget)))
        })
        .collect()
}

/// The host the numbers come from.
fn host() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Runs the benchmarks, then compares with (or writes) the baseline.
pub(crate) fn run(a: &Args) -> Res<ExitCode> {
    let root = repo::root();
    let reports = repo::target_dir(&root).join("xtask-bench");
    repo::remove(&reports)?;
    let mut cmd = process::cargo(&root);
    cmd.args(["bench", "-p", "isoloom-targets", "--bench", "pipeline"])
        .env("CRITERION_HOME", &reports);
    if !process::passed(&mut cmd)? {
        return Ok(ExitCode::FAILURE);
    }
    let now = medians(&reports)?;
    let over = over_budget(&now);
    print!("{}", over.concat());
    if !over.is_empty() {
        println!("bench: {} benchmark(s) over budget", over.len());
        return Ok(ExitCode::FAILURE);
    }
    let file = root.join("bench-baseline.json");
    let stored = match std::fs::read_to_string(&file) {
        Ok(text) => Some(serde_json::from_str::<Value>(&text)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let Some(stored) = stored.filter(|_| !a.update) else {
        let doc = json!({ "host": host(), "unit": "ns", "medians": now });
        std::fs::write(&file, format!("{}\n", serde_json::to_string_pretty(&doc)?))?;
        println!("bench: {} medians written to {}", now.len(), file.display());
        return Ok(ExitCode::SUCCESS);
    };
    let baseline: Medians = serde_json::from_value(stored.get("medians").cloned().unwrap_or_default())?;
    let measured_on = stored.get("host").and_then(Value::as_str).unwrap_or("unknown");
    let report = compare(&baseline, &now, a.threshold);
    print!("{}", report.lines.concat());
    if measured_on != host() {
        println!("bench: the baseline comes from {measured_on}, this is {}: compared, not judged", host());
        return Ok(ExitCode::SUCCESS);
    }
    if report.slower > 0 {
        println!("bench: {} benchmark(s) more than {}% slower than the baseline", report.slower, a.threshold);
    }
    Ok(process::exit_code(report.slower == 0))
}

/// Each benchmark's median from criterion's reports (`<id dir>/new/{benchmark,estimates}.json`).
fn medians(reports: &Path) -> Res<Medians> {
    let mut out = Medians::new();
    for entry in std::fs::read_dir(reports)?.flatten() {
        let new = entry.path().join("new");
        let (Ok(benchmark), Ok(estimates)) = (
            std::fs::read_to_string(new.join("benchmark.json")),
            std::fs::read_to_string(new.join("estimates.json")),
        ) else {
            continue;
        };
        let id = serde_json::from_str::<Value>(&benchmark)?
            .get("full_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let median = serde_json::from_str::<Value>(&estimates)?
            .pointer("/median/point_estimate")
            .and_then(Value::as_f64);
        if let (Some(id), Some(median)) = (id, median) {
            out.insert(id, median.round());
        }
    }
    if out.is_empty() {
        return Err(format!("no benchmark results under {}", reports.display()).into());
    }
    Ok(out)
}

/// The comparison: a line per benchmark, and how many are slower than the threshold allows.
#[derive(Debug, Default)]
struct Report {
    lines: Vec<String>,
    slower: usize,
}

fn compare(baseline: &Medians, now: &Medians, threshold: f64) -> Report {
    let mut report = Report::default();
    for (id, median) in now {
        let Some(base) = baseline.get(id) else {
            report.lines.push(format!("{id:<28} {:>10}  (not in the baseline)\n", shown(*median)));
            continue;
        };
        let change = (median / base - 1.0) * 100.0;
        let slower = change > threshold;
        report.slower += usize::from(slower);
        let mark = if slower { "  SLOWER" } else { "" };
        report.lines.push(format!(
            "{id:<28} {:>10}  baseline {:>10}  {change:+6.1}%{mark}\n",
            shown(*median),
            shown(*base)
        ));
    }
    for id in baseline.keys().filter(|id| !now.contains_key(*id)) {
        report.lines.push(format!("{id:<28} gone (in the baseline only)\n"));
    }
    report
}

/// A duration in nanoseconds, in the unit that reads best.
fn shown(ns: f64) -> String {
    match ns {
        n if n >= 1e9 => format!("{:.2} s", n / 1e9),
        n if n >= 1e6 => format!("{:.2} ms", n / 1e6),
        n if n >= 1e3 => format!("{:.1} µs", n / 1e3),
        n => format!("{n:.0} ns"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn medians(entries: &[(&str, f64)]) -> Medians {
        entries.iter().map(|(id, ns)| ((*id).to_owned(), *ns)).collect()
    }

    #[test]
    fn only_a_benchmark_slower_than_the_threshold_counts() {
        let baseline = medians(&[("parse/a", 100.0), ("lower/a", 100.0), ("render/a", 100.0)]);
        let now = medians(&[("parse/a", 130.0), ("lower/a", 120.0), ("render/b", 5.0)]);
        let report = compare(&baseline, &now, 25.0);
        assert_eq!(report.slower, 1);
        let text = report.lines.concat();
        assert!(text.contains("parse/a") && text.contains("+30.0%  SLOWER"), "{text}");
        assert!(text.contains("render/b") && text.contains("not in the baseline"), "{text}");
        assert!(text.contains("render/a                     gone"), "{text}");
    }

    #[test]
    fn a_benchmark_over_its_budget_is_named() {
        assert_eq!(over_budget(&medians(&[("generate/every-example", 7.9e9)])), Vec::<String>::new());
        let over = over_budget(&medians(&[("generate/every-example", 8.1e9), ("parse/a", 9e9)]));
        assert_eq!(over.len(), 1);
        assert!(
            over[0].contains("generate/every-example") && over[0].contains("over its budget of 8.00 s"),
            "{over:?}"
        );
    }

    #[test]
    fn a_duration_reads_in_its_unit() {
        assert_eq!(shown(850.0), "850 ns");
        assert_eq!(shown(366_600.0), "366.6 µs");
        assert_eq!(shown(11_804_000.0), "11.80 ms");
        assert_eq!(shown(2_500_000_000.0), "2.50 s");
    }
}
