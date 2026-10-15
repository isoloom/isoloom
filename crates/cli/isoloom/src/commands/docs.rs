//! `isoloom docs`: the reference pages generated from the code, for the repository's `docs/`
//! folder. `--spec` prints `docs/spec.md`: every field of the spec from its JSON schema.

use super::{Console, Result, Verdict};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// The spec reference (docs/spec.md): every field, its type, default, allowed values and
    /// limits.
    #[arg(long, required = true)]
    spec: bool,
}

/// Prints the page asked for.
pub(crate) fn run(a: &Args, console: &mut Console) -> Result<Verdict> {
    if a.spec {
        console.print(isoloom_spec::reference::markdown())?;
    }
    Ok(Verdict::Pass)
}
