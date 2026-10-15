//! `isoloom docs`: the reference pages generated from the code, for the repository's `docs/`
//! folder. `--spec` prints `docs/spec.md`: every field of the spec from its JSON schema.

use std::process::ExitCode;

#[derive(clap::Args)]
pub struct Args {
    /// The spec reference (docs/spec.md): every field, its type, default, allowed values and
    /// limits.
    #[arg(long, required = true)]
    spec: bool,
}

pub fn run(a: &Args) -> ExitCode {
    if a.spec {
        print!("{}", isoloom_spec::reference::markdown());
    }
    ExitCode::SUCCESS
}
