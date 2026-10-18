//! `isoloom matrix`: what each target does with each spec field, and how far each target has been
//! proven (its tier, with the evidence), worked out from the generators themselves.

use super::{Console, Result, Verdict};

// No doc comments on the clap types: clap would print them in `--help`.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// The matrix as JSON: the columns with their tiers, every tier entry with its evidence, and
    /// each row's cell per column (its state, and for a refusal the step and reason).
    #[arg(long)]
    json: bool,
}

/// Prints the matrix, as text or JSON.
pub(crate) fn run(a: &Args, console: &mut Console) -> Result<Verdict> {
    let matrix = isoloom_targets::matrix::matrix()?;
    if a.json {
        console.json(&matrix.json())?;
    } else {
        console.print(matrix.text())?;
    }
    Ok(Verdict::Pass)
}
