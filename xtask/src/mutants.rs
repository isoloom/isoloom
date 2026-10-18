//! `cargo xtask mutants [CRATE...]`: mutation testing (cargo-mutants, PLAN 3d) on the core crates
//! (by default isoloom-spec, isoloom-catalogue, isoloom-plan, isoloom-render and
//! isoloom-manifest). Each mutant is a
//! small change to the code (an operator swapped, a condition negated, a function returning a
//! default) and must make a test of its crate fail. A mutant no test catches ("missed") fails the
//! run, until a test catches it or `.cargo/mutants.toml` lists it, with its reason, as one no test
//! can tell apart from the original.
//!
//! Each crate's report goes to `<target dir>/mutants/<crate>/mutants.out` (`missed.txt`,
//! `caught.txt`, `timeout.txt`, `outcomes.json`). `--jobs N` builds and tests N mutants at once,
//! each in its own copy of the tree. `cargo xtask check-all --deep` runs it. Needs `cargo install
//! --locked cargo-mutants`.

use std::process::ExitCode;

use crate::{Res, process, repo};

/// The crates mutated by default: the core.
pub(crate) const CORE: [&str; 5] = ["isoloom-spec", "isoloom-catalogue", "isoloom-plan", "isoloom-render", "isoloom-manifest"];

/// The arguments of `cargo xtask mutants`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// How many mutants are built and tested at once.
    #[arg(long, short, default_value_t = 1)]
    jobs: u8,
    /// The crates (default: isoloom-spec, isoloom-catalogue, isoloom-plan, isoloom-render,
    /// isoloom-manifest).
    crates: Vec<String>,
}

/// Mutates each crate in turn; fails when a mutant survived or timed out in any.
pub(crate) fn run(a: &Args) -> Res<ExitCode> {
    let root = repo::root();
    if process::first_line(process::cargo(&root).args(["mutants", "--version"])).is_none() {
        return Err("cargo-mutants is missing: cargo install --locked cargo-mutants".into());
    }
    let crates: Vec<String> = if a.crates.is_empty() {
        CORE.iter().map(|c| (*c).to_owned()).collect()
    } else {
        a.crates.clone()
    };
    let mut clean = true;
    for krate in &crates {
        let out = repo::target_dir(&root).join("mutants").join(krate);
        // cargo-mutants makes `mutants.out` in this folder, not the folder itself.
        std::fs::create_dir_all(&out)?;
        let mut cmd = process::cargo(&root);
        cmd.args(["mutants", "--package", krate, "--jobs", &a.jobs.to_string(), "--output"]).arg(&out);
        let caught = process::passed(&mut cmd)?;
        clean &= caught;
        let verdict = if caught { "every mutant caught" } else { "mutants missed or timed out" };
        println!("mutants: {krate}: {verdict} (report: {})", out.join("mutants.out").display());
    }
    Ok(process::exit_code(clean))
}
