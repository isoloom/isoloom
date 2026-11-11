//! `cargo xtask test`: the workspace's tests under cargo-nextest (each test in its own process,
//! nothing retried, slow tests reported: `.config/nextest.toml`), then the doctests, which
//! nextest doesn't run, with `cargo test --doc`. Both run, so a failure in one doesn't hide the
//! other.
//!
//! Both run with the temporary folder (`TMPDIR`, and `TMP` and `TEMP` on Windows) pointed at a
//! fresh folder of this run's, and the task fails when a test left anything in it: a test's
//! scratch folder is a `tempfile::TempDir`, removed when the test ends, so nothing piles up in
//! the system's temporary folder run after run. The folder is removed after the check;
//! `ISOLOOM_KEEP_TEMP=1` keeps it, to look at what a test wrote, and prints where it is.

use std::path::Path;
use std::process::{Command, ExitCode};

use crate::{Res, process, repo};

/// The variable that keeps the run's temporary folder.
const KEEP: &str = "ISOLOOM_KEEP_TEMP";

/// The most leftover names printed; the count says how many more there are.
const SHOWN: usize = 20;

/// The arguments of `cargo xtask test`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// More arguments for `cargo nextest run` (a filter, `--no-capture`); with them, the doctests
    /// don't run.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    nextest: Vec<String>,
}

/// Runs the tests, then the doctests; fails when either does or when they left a file or a
/// folder in the run's temporary folder.
pub(crate) fn run(a: &Args) -> Res<ExitCode> {
    let root = repo::root();
    if process::first_line(process::cargo(&root).args(["nextest", "--version"])).is_none() {
        return Err("cargo-nextest is missing: cargo install --locked cargo-nextest".into());
    }
    let temp = std::env::temp_dir().join(format!("isoloom-xtask-test-{}", std::process::id()));
    std::fs::create_dir(&temp).map_err(|e| format!("{}: {e}", temp.display()))?;
    let passed = tests(&root, a, &temp);
    let clean = clean(&temp);
    finish(&temp);
    Ok(process::exit_code(passed? && clean?))
}

/// The tests, then the doctests unless a filter was given, with `temp` as their temporary
/// folder: whether all passed.
fn tests(root: &Path, a: &Args, temp: &Path) -> Res<bool> {
    let tests = process::passed(in_temp(process::cargo(root).args(["nextest", "run", "--workspace"]).args(&a.nextest), temp))?;
    if !a.nextest.is_empty() {
        return Ok(tests);
    }
    let doctests = process::passed(in_temp(process::cargo(root).args(["test", "--workspace", "--doc"]), temp))?;
    Ok(tests && doctests)
}

/// `cmd` with `temp` as its temporary folder, on every OS.
fn in_temp<'c>(cmd: &'c mut Command, temp: &Path) -> &'c mut Command {
    cmd.env("TMPDIR", temp).env("TMP", temp).env("TEMP", temp)
}

/// Whether the tests left `temp` empty; the leftovers are printed when they didn't.
fn clean(temp: &Path) -> Res<bool> {
    let left = leftovers(temp).map_err(|e| format!("{}: {e}", temp.display()))?;
    if !left.is_empty() {
        eprintln!("{}", report(&left));
    }
    Ok(left.is_empty())
}

/// What is in `dir`, by name, sorted.
fn leftovers(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut names = std::fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}

/// The failure for the `left` names: each of the first [`SHOWN`], then how many more.
fn report(left: &[String]) -> String {
    let shown = left.iter().take(SHOWN).map(|name| format!("\n  {name}"));
    let more = (left.len() > SHOWN).then(|| format!("\n  and {} more", left.len() - SHOWN));
    format!(
        "the tests left {} file(s) or folder(s) in their temporary folder: a test's scratch folder must be a tempfile::TempDir (rerun with {KEEP}=1 to keep them){}",
        left.len(),
        shown.chain(more).collect::<String>()
    )
}

/// Removes the run's temporary folder, or keeps it and says where when [`KEEP`] is set.
fn finish(temp: &Path) {
    if std::env::var_os(KEEP).is_some_and(|v| !v.is_empty() && v != "0") {
        eprintln!("{KEEP}: the tests' temporary folder is kept at {}", temp.display());
    } else if let Err(e) = repo::remove(temp) {
        eprintln!("note: {} is left behind: {e}", temp.display());
    }
}

#[cfg(test)]
mod tests {
    use super::{SHOWN, leftovers, report};

    #[test]
    fn an_empty_folder_has_no_leftovers_and_each_file_or_folder_is_one_by_name() {
        let dir = tempfile::Builder::new().prefix("isoloom-xtask-leftovers-").tempdir().unwrap();
        assert_eq!(leftovers(dir.path()).unwrap(), Vec::<String>::new());
        std::fs::create_dir(dir.path().join("isoloom-cli-1-x")).unwrap();
        std::fs::write(dir.path().join("a-file"), "").unwrap();
        assert_eq!(leftovers(dir.path()).unwrap(), ["a-file", "isoloom-cli-1-x"]);
    }

    #[test]
    fn a_missing_folder_is_an_error_not_a_clean_run() {
        let dir = tempfile::Builder::new().prefix("isoloom-xtask-leftovers-").tempdir().unwrap();
        assert!(leftovers(&dir.path().join("gone")).is_err());
    }

    #[test]
    fn the_report_names_the_first_leftovers_and_counts_the_rest() {
        let left: Vec<String> = (0..SHOWN + 3).map(|i| format!("isoloom-spec-{i:02}")).collect();
        let text = report(&left);
        assert!(text.starts_with(&format!("the tests left {} file(s)", SHOWN + 3)), "{text}");
        assert!(
            text.contains("\n  isoloom-spec-00\n") && text.contains(&format!("\n  isoloom-spec-{:02}", SHOWN - 1)),
            "{text}"
        );
        assert!(!text.contains(&format!("isoloom-spec-{SHOWN:02}")), "{text}");
        assert!(text.ends_with("\n  and 3 more"), "{text}");
    }
}
