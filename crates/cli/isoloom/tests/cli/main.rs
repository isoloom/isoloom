//! The `isoloom` binary itself: what each command prints and the exit code it gives, how an
//! error reads (a spec's problems with their fixes, an error's causes), imports that pass
//! validate, secrets that never reach a generated file, and every example on every target, its
//! snapshot. Each test gets its own folders and `ISOLOOM_HOME`.
//!
//! The crate is named `isoloom`, with no `isoloom-` prefix to strip: its suite is `cli`.

#![expect(
    clippy::disallowed_methods,
    reason = "the tests run the binary and the tools that check its output: test code, not an adapter"
)]

#[cfg(unix)]
mod bases;
mod commands;
mod docs;
mod errors;
mod examples;
mod host_reach;
mod imports;
mod reports;
mod secrets;
mod testing;
mod workflows;
