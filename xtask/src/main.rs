//! Isoloom's development tasks, run as `cargo xtask <task>` (the alias is in
//! `.cargo/config.toml`), the same way on macOS, Linux and Windows:
//!
//! - `check-all`: every repository's gates (Isoloom, Isothrum, the launcher) on this machine and
//!   the test hosts, in one table;
//! - `verify`: the official validators on every generated file, in the pinned image;
//! - `generate-all`: every example generated into one tree, for `verify`;
//! - `dev-link`: the consumers built against this checkout, without touching their pins;
//! - `host-setup`: a test host's tools, installed once;
//! - `quality`: the ratchet, the clean-code counts per crate against `quality-baseline.json`;
//! - `test`: the tests under cargo-nextest, then the doctests;
//! - `msrv` and `features`: every crate checked with its declared minimum Rust, and with every
//!   combination of its features (cargo-hack);
//! - `fuzz`: the fuzz targets of `fuzz/` (cargo-fuzz), each for a while;
//! - `mutants`: mutation testing of the core crates (cargo-mutants);
//! - `bench`: the generation benchmarks against `bench-baseline.json`;
//! - `tiers record`: a real run added to the tier list, the evidence the tiers are worked out
//!   from.
//!
//! Each task is a module; `process` runs the tools they drive, `repo` finds the checkouts. `deps`
//! is the test that keeps the workspace's dependencies one way.

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the tasks report on the terminal: printing is their interface, not stray debugging output"
)]

mod bench;
mod check_all;
#[cfg(test)]
mod deps;
mod dev_link;
mod fuzz;
mod generate_all;
mod hack;
mod host_setup;
mod mutants;
mod process;
mod quality;
mod repo;
mod test;
mod text;
mod tiers;
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
    /// The ratchet: every clean-code count per crate against quality-baseline.json; fails when
    /// one goes up (`--update` writes them when they only went down).
    Quality(quality::Args),
    /// The workspace's tests under cargo-nextest (`.config/nextest.toml`), then the doctests.
    Test(test::Args),
    /// Every crate checked with the toolchain its `rust-version` names (cargo-hack).
    Msrv,
    /// Every crate checked with every combination of its features (cargo-hack).
    Features,
    /// The fuzz targets of fuzz/ (cargo-fuzz, nightly), each for a while; exits 1 when one
    /// crashes.
    Fuzz(fuzz::Args),
    /// Mutation testing of the core crates (cargo-mutants): exits 1 when a mutant survives.
    Mutants(mutants::Args),
    /// The generation benchmarks (criterion) against bench-baseline.json; exits 1 when one is
    /// slower than the threshold allows.
    Bench(bench::Args),
    /// The tier list (crates/targets/isoloom-targets/tiers.json): `record <entry> <host>
    /// <result>` adds a real run, the evidence a tier is worked out from.
    Tiers(tiers::Args),
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
        Task::Quality(a) => quality::run(&a),
        Task::Test(a) => test::run(&a),
        Task::Msrv => hack::msrv(),
        Task::Features => hack::features(),
        Task::Fuzz(a) => fuzz::run(&a),
        Task::Mutants(a) => mutants::run(&a),
        Task::Bench(a) => bench::run(&a),
        Task::Tiers(a) => tiers::run(&a),
    }
}
