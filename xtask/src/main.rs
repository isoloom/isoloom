//! Isoloom's development tasks, run as `cargo xtask <task>` (the alias is in
//! `.cargo/config.toml`), the same way on macOS, Linux and Windows:
//!
//! - `check-all`: every repository's gates (Isoloom, Isothrum, the launcher) on this machine and
//!   the test hosts, in one table;
//! - `verify`: the official validators on every generated file, in the pinned image;
//! - `generate-all`: every example generated into one tree, for `verify`;
//! - `dev-link`: the consumers built against this checkout, without touching their pins;
//! - `host-setup`: a test host's tools, installed once.
//!
//! Each task is a module; `process` runs the tools they drive, `repo` finds the checkouts. `deps`
//! is the test that keeps the workspace's dependencies one way.

// The tasks report on the terminal: printing is their interface, not stray debugging output.
#![allow(clippy::print_stdout)]

mod check_all;
#[cfg(test)]
mod deps;
mod dev_link;
mod generate_all;
mod host_setup;
mod process;
mod repo;
mod text;
mod verify;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// What a task returns: its exit code, or the error printed as `xtask: ...` (exit code 2).
type Res<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Parser)]
#[command(name = "xtask", bin_name = "cargo xtask", about = "Isoloom's development tasks.")]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Every repository's gates on one or more hosts, then one summary table; exits 1 when a
    /// gate fails.
    CheckAll(check_all::Args),
    /// The official validators on generated files, in the pinned image (tools/verify), against
    /// the baseline (tools/verify/baseline.json); exits 1 on a failure the baseline doesn't know.
    Verify(verify::Args),
    /// Every example (or the spec folders given) generated for every target it can be, into one
    /// tree that `verify` checks.
    GenerateAll(generate_all::Args),
    /// Point the consumers' Isoloom dependencies at this checkout (a git-ignored Cargo
    /// `[patch]`), or back to their pins.
    DevLink(dev_link::Args),
    /// Install what a Linux or Windows test host needs (`--check` only reports).
    HostSetup(host_setup::Args),
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Res<ExitCode> {
    match cli.task {
        Task::CheckAll(a) => check_all::run(&a),
        Task::Verify(a) => Ok(verify::run(&a)),
        Task::GenerateAll(a) => generate_all::run(&a),
        Task::DevLink(a) => Ok(dev_link::run(&a)),
        Task::HostSetup(a) => host_setup::run(&a),
    }
}
