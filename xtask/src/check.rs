//! `cargo xtask check`: every check a change to Isoloom must pass, in one command, the same one
//! locally and in CI (`.github/workflows/ci.yml` runs its steps with `--only`). `--fast` runs the
//! short list a branch is checked with before it is merged: the lockfile, rustfmt, clippy, the
//! tests and the Windows clippy. `--list` prints the steps, `--only` and `--skip` choose among
//! them by name.
//!
//! Every step runs, a red one included, and the summary names each step's outcome: the command
//! fails when one did. A step whose tool is missing is red, and says what to install: nothing
//! is skipped quietly. The tools: cargo-nextest, cargo-deny, cargo-machete, cargo-semver-checks,
//! typos, lychee, shellcheck, gitleaks, wasm-pack, terraform, Docker (`tf-test`), the toolchain
//! of the workspace's `rust-version` (`msrv`), the `x86_64-pc-windows-gnu` target with a MinGW
//! C compiler on `PATH` (`windows-gnu`) and nightly (`fuzz`).

mod outputs;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Instant;

use crate::{Res, process, repo};

/// The arguments of `cargo xtask check`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Only the short list: lockfile, fmt, clippy, test, windows-gnu.
    #[arg(long)]
    fast: bool,
    /// Prints the steps and exits.
    #[arg(long)]
    list: bool,
    /// Only these steps (comma-separated names, as `--list` prints them).
    #[arg(long, value_delimiter = ',', value_name = "STEPS")]
    only: Vec<String>,
    /// Every step but these (comma-separated names).
    #[arg(long, value_delimiter = ',', value_name = "STEPS")]
    skip: Vec<String>,
}

/// One check: its name, whether `--fast` runs it, what it proves, how it runs (whether it
/// passed), and whether a red run is run once more.
pub(crate) struct Step {
    pub(crate) name: &'static str,
    pub(crate) fast: bool,
    pub(crate) about: &'static str,
    run: fn(&Path) -> Res<bool>,
    retry: bool,
}

const fn step(name: &'static str, fast: bool, about: &'static str, run: fn(&Path) -> Res<bool>) -> Step {
    Step {
        name,
        fast,
        about,
        run,
        retry: false,
    }
}

/// Every step, in the order they run: the quick ones first, so a red one shows early.
pub(crate) const STEPS: &[Step] = &[
    step("lockfile", true, "Cargo.lock is current (as --locked checks it)", lockfile),
    step("fmt", true, "rustfmt", |root| cargo(root, &["fmt", "--all", "--check"])),
    step("typos", false, "spelling; legitimate words go in .typos.toml", |root| tool(root, "typos", &[])),
    step("links", false, "every link between the Markdown pages reaches a file (offline)", links),
    step("deny", false, "advisories, licences, bans, sources (deny.toml)", |root| {
        cargo(root, &["deny", "check"])
    }),
    step("machete", false, "no unused dependency", |root| cargo(root, &["machete"])),
    step("hooks", false, "the Git hooks: shellcheck, then their test (gitleaks on PATH)", hooks),
    step("clippy", true, "clippy, any warning fails", |root| {
        cargo(root, &["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"])
    }),
    step("test", true, "the tests under cargo-nextest, then the doctests (`cargo xtask test`)", |root| {
        xtask(root, &["test"])
    }),
    step("doc", false, "the API docs build without a warning", |root| {
        process::passed(
            process::cargo(root)
                .args(["doc", "--workspace", "--no-deps", "--locked"])
                .env("RUSTDOCFLAGS", "-D warnings"),
        )
    }),
    step("quality", false, "the ratchet: no per-crate count goes up (`cargo xtask quality`)", |root| {
        xtask(root, &["quality"])
    }),
    step("regenerate", false, "the generated docs and schemas are current", regenerate),
    step("fuzz", false, "the fuzz crate's format and lints (`cargo xtask fuzz --check`)", |root| {
        xtask(root, &["fuzz", "--check"])
    }),
    step("examples", false, "every example validates and lists its targets", outputs::examples),
    step(
        "determinism",
        false,
        "every example generated twice by the command line gives the same files",
        outputs::determinism,
    ),
    step("msrv", false, "the published crates build with the workspace's rust-version", msrv),
    step(
        "windows-gnu",
        true,
        "clippy for Windows (x86_64-pc-windows-gnu): the cfg(windows) code",
        |root| {
            cargo(
                root,
                &[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--locked",
                    "--target",
                    "x86_64-pc-windows-gnu",
                    "--",
                    "-D",
                    "warnings",
                ],
            )
        },
    ),
    step("semver", false, "the library API against its latest release on crates.io", semver),
    step(
        "wasm",
        false,
        "the topology editor's engine builds for the browser (`cargo xtask wasm`)",
        |root| xtask(root, &["wasm"]),
    ),
    step("wasm-test", false, "the engine's exports, called as JavaScript calls them", |root| {
        tool(root, "wasm-pack", &["test", "--node", "crates/frontends/isoloom-wasm"])
    }),
    // Once more when red: the provider cache's checksum fails now and then on a fresh image.
    Step {
        retry: true,
        ..step("tf-test", false, "the modules' logic, mocked providers (`cargo xtask tf-test`)", |root| {
            xtask(root, &["tf-test"])
        })
    },
    step(
        "terraform",
        false,
        "every generated Terraform module is valid for its providers",
        outputs::terraform,
    ),
];

/// Runs the chosen steps; fails when one did.
pub(crate) fn run(a: &Args) -> Res<ExitCode> {
    let chosen = chosen(a)?;
    if a.list {
        for s in &chosen {
            println!("{:<12} {}{}", s.name, if s.fast { "(fast) " } else { "" }, s.about);
        }
        return Ok(ExitCode::SUCCESS);
    }
    let root = repo::root();
    let mut outcomes = Vec::new();
    for s in &chosen {
        outcomes.push((s.name, attempt(s, &root)));
    }
    println!("\n== check summary");
    for (name, outcome) in &outcomes {
        println!("{name:<12} {outcome}");
    }
    let green = outcomes.iter().all(|(_, o)| o.starts_with("ok"));
    println!("== check {}", if green { "green" } else { "red" });
    Ok(process::exit_code(green))
}

/// The steps `a` asks for, in order. An error names a step that doesn't exist.
pub(crate) fn chosen(a: &Args) -> Res<Vec<&'static Step>> {
    for name in a.only.iter().chain(&a.skip) {
        if !STEPS.iter().any(|s| s.name == name) {
            let names: Vec<&str> = STEPS.iter().map(|s| s.name).collect();
            return Err(format!("no step `{name}`; the steps: {}", names.join(", ")).into());
        }
    }
    Ok(STEPS
        .iter()
        .filter(|s| !a.fast || s.fast)
        .filter(|s| a.only.is_empty() || a.only.iter().any(|n| n == s.name))
        .filter(|s| !a.skip.iter().any(|n| n == s.name))
        .collect())
}

/// Runs one step (twice when it may be retried and was red): its outcome and how long it took.
fn attempt(s: &Step, root: &Path) -> String {
    let start = Instant::now();
    let mut outcome = once(s, root);
    if !outcome && s.retry {
        println!("== check {}: red, run once more", s.name);
        outcome = once(s, root);
        if outcome {
            return format!("ok on retry ({}s)", start.elapsed().as_secs());
        }
    }
    format!("{} ({}s)", if outcome { "ok" } else { "FAIL" }, start.elapsed().as_secs())
}

fn once(s: &Step, root: &Path) -> bool {
    println!("== check {}: {}", s.name, s.about);
    match (s.run)(root) {
        Ok(passed) => passed,
        Err(e) => {
            eprintln!("check {}: {e}", s.name);
            false
        }
    }
}

/// `cargo <args>` in the checkout.
fn cargo(root: &Path, args: &[&str]) -> Res<bool> {
    process::passed(process::cargo(root).args(args))
}

/// A tool on `PATH`, in the checkout.
fn tool(root: &Path, program: &str, args: &[&str]) -> Res<bool> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(root);
    process::passed(&mut cmd).map_err(|e| format!("{e} (is {program} installed?)").into())
}

/// Another task of this xtask, run by this very binary (no second build).
fn xtask(root: &Path, args: &[&str]) -> Res<bool> {
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.args(args).current_dir(root);
    process::passed(&mut cmd)
}

fn lockfile(root: &Path) -> Res<bool> {
    let mut cmd = process::cargo(root);
    cmd.args(["metadata", "--locked", "--offline", "--format-version", "1", "-q"])
        .stdout(std::process::Stdio::null());
    process::passed(&mut cmd)
}

fn links(root: &Path) -> Res<bool> {
    let pages = ["README.md", "CHANGELOG.md", "ARCHITECTURE.md", ".github/*.md", "docs/**/*.md"];
    tool(root, "lychee", &[&["--offline", "--no-progress"][..], &pages[..]].concat())
}

/// shellcheck over every hook and the set-up script, then the hooks' own test.
fn hooks(root: &Path) -> Res<bool> {
    let mut scripts: Vec<String> = std::fs::read_dir(root.join(".githooks"))?
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| format!(".githooks/{}", e.file_name().to_string_lossy()))
        .collect();
    scripts.sort();
    scripts.push("scripts/setup-hooks.sh".to_owned());
    let mut args = vec!["-x"];
    args.extend(scripts.iter().map(String::as_str));
    Ok(tool(root, "shellcheck", &args)? && tool(root, "bash", &[".githooks/test.sh"])?)
}

fn regenerate(root: &Path) -> Res<bool> {
    tool(root, "sh", &[".githooks/regenerate", "--check"])
}

/// `cargo +<rust-version> check` of the published crates (the development tasks build with the
/// pinned toolchain only). Through rustup's `cargo`, which reads `+<version>`.
fn msrv(root: &Path) -> Res<bool> {
    let version = rust_version(&std::fs::read_to_string(root.join("Cargo.toml"))?)?;
    let mut cmd = Command::new("cargo");
    process::without_run_env(&mut cmd);
    cmd.env_remove("RUSTUP_TOOLCHAIN").current_dir(root);
    cmd.arg(format!("+{version}"));
    cmd.args([
        "check",
        "--workspace",
        "--exclude",
        "xtask",
        "--exclude",
        "xtask-quality",
        "--all-targets",
        "--locked",
    ]);
    process::passed(&mut cmd)
}

/// The workspace's `rust-version`, from its manifest.
pub(crate) fn rust_version(manifest: &str) -> Res<String> {
    let doc: toml::Table = manifest.parse()?;
    doc.get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("rust-version"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "Cargo.toml sets no workspace.package.rust-version".into())
}

/// The crates whose API is a promise (docs/reference/policies.md): the others are building
/// blocks.
pub(crate) const SEMVER_CRATES: &[&str] = &["isoloom-spec", "isoloom-targets"];

/// cargo-semver-checks over the supported library API (docs/reference/policies.md), against
/// each crate's latest release on crates.io.
fn semver(root: &Path) -> Res<bool> {
    let mut fetch = Command::new("curl");
    fetch.args([
        "-fsS",
        "-A",
        "isoloom-xtask (https://github.com/isoloom/isoloom)",
        "https://crates.io/api/v1/crates/isoloom-spec",
    ]);
    let published = latest_release(&process::output(&mut fetch)?)?;
    println!("latest isoloom-spec on crates.io: {published}");
    let mut args = vec!["semver-checks"];
    args.extend(SEMVER_CRATES.iter().flat_map(|c| ["-p", c]));
    args.extend(release_type(&published));
    cargo(root, &args)
}

/// The latest stable version in crates.io's answer about a crate.
pub(crate) fn latest_release(answer: &str) -> Res<String> {
    let json: serde_json::Value = serde_json::from_str(answer)?;
    json.pointer("/crate/max_stable_version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "crates.io's answer names no max_stable_version".into())
}

/// The release type the changes are checked as. While crates.io's latest is a 1.x, the
/// unreleased changes are the major they are (they remove public API); from 2.x on, the
/// workspace version decides.
pub(crate) fn release_type(published: &str) -> Vec<&'static str> {
    if published.starts_with("1.") {
        vec!["--release-type", "major"]
    } else {
        Vec::new()
    }
}
