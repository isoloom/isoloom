//! `cargo xtask fuzz-combos`: combination fuzzing of the spec pipeline (the Phase 6 "10k random
//! specs" item). A seeded generator ([`generator`]) builds many thousands of random but valid
//! specs in the new syntax, and for each the harness runs the full `generate_all` across every
//! target, asserting the [`invariants`]: no panic (no `unwrap`/`expect` fires), generation is
//! deterministic (render-twice is byte-identical), every refusal carries a reason, and no secret
//! value reaches a generated file.
//!
//! The big run (`--count`, default [`DEEP`]) sits behind `cargo xtask check-all --deep`. A small
//! fast run is a normal test, so the gate catches a regression without the long run.
//! A failing seed reproduces its spec exactly; the harness prints the seed and the spec, and any
//! seed found to fail is pinned in `REGRESSION_SEEDS` (and its spec may be committed under
//! `regressions/`), which the fast test always replays.

mod generator;
mod invariants;

use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::process::ExitCode;

use crate::Res;

/// How many random specs the big run checks by default: the Phase 6 "10k random specs".
const DEEP: u64 = 10_000;

/// How many the fast test (the normal gate) checks: enough to exercise the generator's variety,
/// fast enough for the under-a-minute unit suite.
#[cfg(test)]
const FAST: u64 = 150;

/// Seeds found to fail once, replayed by the fast test forever after. Empty while none has.
#[cfg(test)]
const REGRESSION_SEEDS: &[u64] = &[];

/// The arguments of `cargo xtask fuzz-combos`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// How many random specs to check.
    #[arg(long, default_value_t = DEEP)]
    count: u64,
    /// The first seed (the specs are seeds `start..start + count`).
    #[arg(long, default_value_t = 0)]
    start: u64,
    /// Stop after this many failures (so a broken pipeline doesn't flood the terminal).
    #[arg(long, default_value_t = 10)]
    max_failures: usize,
    /// Write each failing spec here as `<seed>.yml`, to inspect or commit as a regression.
    #[arg(long, value_name = "DIR")]
    dump: Option<PathBuf>,
}

/// Runs the big combination fuzz; exits 1 when any spec breaks an invariant or panics.
pub(crate) fn run(a: &Args) -> Res<ExitCode> {
    let mut failures: Vec<(u64, String)> = Vec::new();
    let mut totals = Totals::default();
    let hook = install_quiet_hook();
    for seed in a.start..a.start.saturating_add(a.count) {
        totals.checked += 1;
        match guarded(seed) {
            Ok(outcome) => {
                totals.generated += outcome.generated;
                totals.skipped += outcome.skipped;
            }
            Err(reason) => {
                failures.push((seed, reason));
                if failures.len() >= a.max_failures {
                    break;
                }
            }
        }
    }
    panic::set_hook(hook);
    report(a, &totals, &failures)
}

/// What the run counted across every spec it checked.
#[derive(Default)]
struct Totals {
    /// How many specs were checked.
    checked: u64,
    /// How many target generations produced files, summed over the specs.
    generated: usize,
    /// How many targets were left out with a reason, summed over the specs.
    skipped: usize,
}

/// Checks one seed, turning a panic (a fired `unwrap`/`expect`, say) into a failure that names
/// the seed instead of aborting the whole run.
fn guarded(seed: u64) -> Result<invariants::Outcome, String> {
    match panic::catch_unwind(AssertUnwindSafe(|| run_seed(seed))) {
        Ok(result) => result,
        Err(payload) => Err(format!("panicked: {}", panic_message(payload.as_ref()))),
    }
}

/// Generates the spec for `seed` and checks every invariant.
fn run_seed(seed: u64) -> Result<invariants::Outcome, String> {
    invariants::check(&generator::spec(seed))
}

/// The message of a caught panic, when it is a string.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".to_owned())
}

/// A panic hook that says nothing, so a caught panic in the run doesn't print its own trace; the
/// previous hook is returned to put back.
fn install_quiet_hook() -> Box<dyn Fn(&panic::PanicHookInfo<'_>) + Sync + Send + 'static> {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    previous
}

/// Prints the summary and, for a failure, the seed and its spec; writes the dump when asked.
fn report(a: &Args, totals: &Totals, failures: &[(u64, String)]) -> Res<ExitCode> {
    println!(
        "fuzz-combos: checked {} specs from seed {}, {} target generations, {} refusals (each with a reason)",
        totals.checked, a.start, totals.generated, totals.skipped
    );
    if failures.is_empty() {
        println!("fuzz-combos: every invariant held");
        return Ok(ExitCode::SUCCESS);
    }
    for (seed, reason) in failures {
        println!("\nfuzz-combos: seed {seed} FAILED: {reason}");
        println!("--- spec (seed {seed}) ---\n{}", generator::spec(*seed).yaml);
        if let Some(dir) = &a.dump {
            std::fs::create_dir_all(dir)?;
            std::fs::write(dir.join(format!("{seed}.yml")), generator::spec(*seed).yaml)?;
        }
    }
    println!("\nfuzz-combos: {} seed(s) failed; pin them in REGRESSION_SEEDS", failures.len());
    Ok(ExitCode::FAILURE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fast run: a few hundred random specs through the whole pipeline in the normal gate. A
    /// panic here fails the test (so "no panic, no unwrap/expect fires" is a gate), and an invar-
    /// iant that breaks names the seed, which reproduces the spec.
    #[test]
    fn many_random_specs_generate_cleanly_and_deterministically() {
        for seed in 0..FAST {
            if let Err(reason) = run_seed(seed) {
                panic!("seed {seed} broke an invariant: {reason}\n\nspec:\n{}", generator::spec(seed).yaml);
            }
        }
    }

    /// Every seed once found to fail is checked again, forever.
    #[test]
    fn the_regression_seeds_pass() {
        for &seed in REGRESSION_SEEDS {
            if let Err(reason) = run_seed(seed) {
                panic!("regression seed {seed} failed again: {reason}");
            }
        }
    }

    /// The generator is deterministic: a seed always yields the same spec, so a failing seed
    /// reproduces.
    #[test]
    fn a_seed_always_builds_the_same_spec() {
        for seed in [0, 1, 42, 9999] {
            assert_eq!(generator::spec(seed).yaml, generator::spec(seed).yaml);
        }
    }

    /// The generator does vary: different seeds give different specs (so the run isn't checking
    /// one spec many times).
    #[test]
    fn different_seeds_build_different_specs() {
        let a = generator::spec(1).yaml;
        let b = generator::spec(2).yaml;
        assert_ne!(a, b);
    }
}
