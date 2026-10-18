//! `isoloom docs`: the reference pages generated from the code, for the repository's `docs/`
//! folder. `--spec` prints `docs/spec.md`, every field of the spec from its JSON schema;
//! `--matrix` prints `docs/matrix.md`, what each target does with each field, and the tiers.

use super::{Console, Result, Verdict};

#[derive(Debug, clap::Args)]
#[group(required = true, multiple = false)]
pub(crate) struct Args {
    /// The spec reference (docs/spec.md): every field, its type, default, allowed values and
    /// limits.
    #[arg(long)]
    spec: bool,
    /// The capability matrix (docs/matrix.md): what each target does with each spec field, and
    /// each target's tier with its evidence.
    #[arg(long)]
    matrix: bool,
}

/// Prints the page asked for.
pub(crate) fn run(a: &Args, console: &mut Console) -> Result<Verdict> {
    if a.spec {
        console.print(isoloom_spec::reference::markdown())?;
    }
    if a.matrix {
        console.print(isoloom_targets::matrix::matrix()?.markdown())?;
    }
    Ok(Verdict::Pass)
}
