//! `cargo xtask msrv` and `cargo xtask features`, through cargo-hack (`cargo install --locked
//! cargo-hack`):
//!
//! - `msrv`: every crate checked with the toolchain its `rust-version` names (`cargo hack check
//!   --rust-version`), so the declared minimum is a fact, not a hope. rustup must have that
//!   toolchain (`rustup toolchain install <version> --profile minimal`).
//! - `features`: every combination of every crate's features checked (`cargo hack check
//!   --feature-powerset`), so features stay additive and no combination breaks the build.

use std::process::ExitCode;

use crate::{Res, process, repo};

/// Checks every crate with the toolchain of its `rust-version`.
pub(crate) fn msrv() -> Res<ExitCode> {
    hack(&["check", "--rust-version", "--workspace"])
}

/// Checks every crate with every combination of its features.
pub(crate) fn features() -> Res<ExitCode> {
    hack(&["check", "--feature-powerset", "--workspace"])
}

fn hack(args: &[&str]) -> Res<ExitCode> {
    let root = repo::root();
    if process::first_line(process::cargo(&root).args(["hack", "--version"])).is_none() {
        return Err("cargo-hack is missing: cargo install --locked cargo-hack".into());
    }
    let mut cmd = process::cargo(&root);
    cmd.arg("hack").args(args);
    // The toolchain `--rust-version` asks for (`cargo +<version>`) must win over the one rustup
    // picked for this xtask.
    cmd.env_remove("RUSTUP_TOOLCHAIN");
    Ok(process::exit_code(process::passed(&mut cmd)?))
}
