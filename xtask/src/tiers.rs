//! `cargo xtask tiers record <entry> <host> <result>`: a real run added to the tier list
//! (`crates/targets/isoloom-targets/tiers.json`), the evidence a target's tier is worked out from.
//!
//! The entry is a target, or a target and its provider or cloud (`docker`, `vagrant/libvirt`,
//! `cloud-vm/aws`); the host is one of the list's `hosts` (`mac`, `jarvis`, `karen`, `friday`), or
//! a new one with `--os`; the result is `pass` or `fail`. The run is dated today (UTC) unless
//! `--date` says otherwise. The list is written back in the form it is kept in, and
//! `docs/matrix.md`, which shows the tiers, is generated again.

use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};

use crate::{Res, process, repo};

/// The tier list, from the repository root.
const TIERS: &str = "crates/targets/isoloom-targets/tiers.json";

/// The generated page that shows the tiers, from the repository root.
const MATRIX: &str = "docs/matrix.md";

/// The arguments of `cargo xtask tiers`.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[command(subcommand)]
    action: Action,
}

#[derive(clap::Subcommand)]
enum Action {
    /// Add a real run of an entry's generated files: `cargo xtask tiers record vagrant/hyperv
    /// karen pass`.
    Record(Record),
}

/// A real run to record.
#[derive(clap::Args)]
struct Record {
    /// The entry: a target, or a target and its provider or cloud (`vagrant/libvirt`).
    entry: String,
    /// The host that ran the tools, as the list's `hosts` names it (`mac`, `jarvis`, `karen`).
    host: String,
    /// Whether the run came up and its checks passed.
    result: RunResult,
    /// The day it ran, `YYYY-MM-DD` (default: today, UTC).
    #[arg(long)]
    date: Option<String>,
    /// The server it ran on, for a server target (`pve-lab`).
    #[arg(long)]
    server: Option<String>,
    /// What ran, in a few words.
    #[arg(long)]
    note: Option<String>,
    /// The host's OS, to add a host the list doesn't name yet (`linux`, `macos`, `windows`).
    #[arg(long)]
    os: Option<String>,
}

/// A run's result.
#[derive(Clone, Copy, clap::ValueEnum)]
enum RunResult {
    /// It came up and its checks passed.
    Pass,
    /// It didn't.
    Fail,
}

/// Runs the action asked for.
pub(crate) fn run(a: &Args) -> Res<ExitCode> {
    match &a.action {
        Action::Record(r) => record_run(r),
    }
}

/// Records the run in the list, then generates the page that shows the tiers again.
fn record_run(r: &Record) -> Res<ExitCode> {
    let root = repo::root();
    let path = root.join(TIERS);
    let mut doc: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    let date = match &r.date {
        Some(d) => d.clone(),
        None => civil_date(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()),
    };
    record(&mut doc, r, &date)?;
    std::fs::write(&path, written(&doc)?)?;
    println!("tiers: {} on {} ({date}): {}, in {TIERS}", r.entry, r.host, result_id(r.result));
    regenerate_matrix(&root)
}

/// `docs/matrix.md` generated again from the code, so it shows the new evidence.
fn regenerate_matrix(root: &Path) -> Res<ExitCode> {
    let out = process::cargo(root)
        .args(["run", "-q", "-p", "isoloom", "--", "docs", "--matrix"])
        .output()
        .map_err(|e| format!("`cargo run -p isoloom -- docs --matrix` couldn't start: {e}"))?;
    if !out.status.success() {
        return Err(format!("`isoloom docs --matrix` failed:\n{}", String::from_utf8_lossy(&out.stderr)).into());
    }
    std::fs::write(root.join(MATRIX), &out.stdout)?;
    println!("tiers: {MATRIX} generated again");
    Ok(ExitCode::SUCCESS)
}

/// The list as it is kept: pretty JSON, two spaces, a final newline.
fn written(doc: &Value) -> Res<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(doc)?))
}

/// The result as the list writes it.
fn result_id(r: RunResult) -> &'static str {
    match r {
        RunResult::Pass => "pass",
        RunResult::Fail => "fail",
    }
}

/// `r` added to `doc`'s entry, dated `date`: the host known (or added with its OS), the date a
/// day, and the host's OS one the entry exists on.
fn record(doc: &mut Value, r: &Record, date: &str) -> Res<()> {
    if !is_date(date) {
        return Err(format!("`{date}` isn't a date (YYYY-MM-DD)").into());
    }
    let os = host_os(doc, &r.host, r.os.as_deref())?;
    let entry = doc
        .get_mut("entries")
        .and_then(Value::as_array_mut)
        .and_then(|entries| entries.iter_mut().find(|e| e.get("entry").and_then(Value::as_str) == Some(r.entry.as_str())))
        .ok_or_else(|| format!("no entry `{}` in {TIERS}: a target, or target/provider (`vagrant/libvirt`)", r.entry))?;
    let exists_on = entry.get("exists_on").and_then(Value::as_array).cloned().unwrap_or_default();
    if !exists_on.iter().any(|o| o.as_str() == Some(os.as_str())) {
        return Err(format!("`{}` doesn't exist on {os} (it exists on {})", r.entry, Value::Array(exists_on)).into());
    }
    let mut run = Map::new();
    run.insert("host".to_owned(), json!(r.host));
    run.insert("date".to_owned(), json!(date));
    run.insert("result".to_owned(), json!(result_id(r.result)));
    for (key, value) in [("server", &r.server), ("note", &r.note)] {
        if let Some(v) = value {
            run.insert(key.to_owned(), json!(v));
        }
    }
    let runs = entry
        .get_mut("runs")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| format!("`{}` has no `runs` list", r.entry))?;
    runs.push(Value::Object(run));
    Ok(())
}

/// The OS of `host`: the list's, or `os` for a host it doesn't name yet (then added to it).
fn host_os(doc: &mut Value, host: &str, os: Option<&str>) -> Res<String> {
    let hosts = doc
        .get_mut("hosts")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("{TIERS} has no `hosts`"))?;
    match (hosts.get(host).and_then(Value::as_str), os) {
        (Some(known), Some(asked)) if known != asked => Err(format!("host `{host}` is {known}, not {asked}").into()),
        (Some(known), _) => Ok(known.to_owned()),
        (None, Some(asked)) if ["linux", "macos", "windows"].contains(&asked) => {
            hosts.insert(host.to_owned(), json!(asked));
            Ok(asked.to_owned())
        }
        (None, Some(asked)) => Err(format!("`{asked}` isn't a host OS: linux, macos or windows").into()),
        (None, None) => {
            let known: Vec<&str> = hosts.keys().map(String::as_str).collect();
            Err(format!("no host `{host}` (known: {}); add it with `--os linux|macos|windows`", known.join(", ")).into())
        }
    }
}

/// Whether `text` is a day written `YYYY-MM-DD`.
fn is_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let number =
        |s: &str, len: usize, max: u32| s.len() == len && s.bytes().all(|b| b.is_ascii_digit()) && s.parse::<u32>().is_ok_and(|v| (1..=max).contains(&v));
    matches!(parts.as_slice(), [y, m, d] if number(y, 4, 9999) && number(m, 2, 12) && number(d, 2, 31))
}

/// The UTC day `secs` seconds after the Unix epoch, `YYYY-MM-DD` (Howard Hinnant's
/// days-to-civil algorithm, for the proleptic Gregorian calendar).
fn civil_date(secs: u64) -> String {
    let days = secs / 86_400 + 719_468;
    let era = days / 146_097;
    let doe = days % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(entry: &str, host: &str, os: Option<&str>) -> Record {
        Record {
            entry: entry.to_owned(),
            host: host.to_owned(),
            result: RunResult::Pass,
            date: None,
            server: Some("pve-lab".to_owned()),
            note: None,
            os: os.map(str::to_owned),
        }
    }

    fn list() -> Value {
        json!({
            "hosts": { "mac": "macos", "karen": "windows" },
            "entries": [
                { "entry": "vagrant/hyperv", "goal": 1, "exists_on": ["windows"], "runs": [] },
                { "entry": "proxmox", "goal": 1, "exists_on": ["linux", "macos", "windows"], "runs": [] }
            ]
        })
    }

    #[test]
    fn a_run_is_added_to_its_entry_with_its_date_and_server() {
        let mut doc = list();
        record(&mut doc, &asked("proxmox", "mac", None), "2026-10-15").unwrap();
        assert_eq!(
            doc["entries"][1]["runs"],
            json!([{ "host": "mac", "date": "2026-10-15", "result": "pass", "server": "pve-lab" }])
        );
        assert_eq!(doc["entries"][0]["runs"], json!([]));
    }

    #[test]
    fn a_new_host_needs_its_os_and_is_added() {
        let mut doc = list();
        let unknown = record(&mut doc, &asked("proxmox", "jarvis", None), "2026-10-16").unwrap_err();
        assert!(unknown.to_string().contains("--os"), "{unknown}");
        record(&mut doc, &asked("proxmox", "jarvis", Some("linux")), "2026-10-16").unwrap();
        assert_eq!(doc["hosts"]["jarvis"], "linux");
        assert!(record(&mut doc, &asked("proxmox", "jarvis", Some("windows")), "2026-10-16").is_err());
        assert!(record(&mut doc, &asked("proxmox", "ultron", Some("beos")), "2026-10-16").is_err());
    }

    #[test]
    fn a_run_on_a_host_os_the_entry_doesnt_exist_on_or_an_unknown_entry_is_refused() {
        let mut doc = list();
        let mac = record(&mut doc, &asked("vagrant/hyperv", "mac", None), "2026-10-15").unwrap_err();
        assert!(mac.to_string().contains("doesn't exist on macos"), "{mac}");
        let unknown = record(&mut doc, &asked("vagrant/proxmox", "mac", None), "2026-10-15").unwrap_err();
        assert!(unknown.to_string().contains("no entry `vagrant/proxmox`"), "{unknown}");
        assert!(record(&mut doc, &asked("proxmox", "mac", None), "15/10/2026").is_err());
        assert_eq!(doc, list());
    }

    #[test]
    fn a_day_is_counted_from_the_unix_epoch_in_utc() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(951_782_400), "2000-02-29");
        assert_eq!(civil_date(1_791_936_000), "2026-10-14");
        assert_eq!(civil_date(1_791_936_000 + 86_399), "2026-10-14");
        assert!(is_date(&civil_date(1_791_936_000)) && !is_date("2026-1-1") && !is_date("2026-10-15x"));
    }

    #[test]
    fn the_list_is_kept_in_the_form_this_task_writes_it() {
        let text = std::fs::read_to_string(repo::root().join(TIERS)).unwrap();
        let doc: Value = serde_json::from_str(&text).unwrap();
        assert!(
            written(&doc).unwrap() == text,
            "{TIERS} isn't in its written form: format it as serde_json's pretty printer does"
        );
    }
}
