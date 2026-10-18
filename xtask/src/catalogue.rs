//! `cargo xtask catalogue`: the catalogue's tools (design §4.6 and §4.8).
//!
//! - `refresh`: pins what the entries fetch into their `lock.yml` (and the Galaxy collections
//!   into `collections.yml`), the only code that reaches the network for the catalogue;
//! - `check`: runs an entry's parity contract for real, in both forms, through the Isothrum CLI.

mod check;
mod fetch;
mod galaxy;
mod harness;
mod index;
mod pgp;
mod pins;
mod refresh;
mod registry;

use std::process::ExitCode;

use crate::Res;

/// The arguments of `cargo xtask catalogue`.
#[derive(clap::Args)]
pub(crate) struct Args {
    #[command(subcommand)]
    action: Action,
}

#[derive(clap::Subcommand)]
enum Action {
    /// Pin what the entries fetch (keys, images, vendor packages) into their lock.yml, and the
    /// Galaxy collections into collections.yml; `--check` exits 1 on drift.
    Refresh(refresh::Args),
    /// Run an entry's parity contract for real: each case as a lab, through the Isothrum CLI,
    /// on `docker` and on `vagrant` (`--provider`); exit 1 when a case fails.
    Check(check::Args),
}

/// Runs the action.
pub(crate) fn run(args: &Args) -> Res<ExitCode> {
    match &args.action {
        Action::Refresh(a) => refresh::run(a),
        Action::Check(a) => check::run(a),
    }
}
