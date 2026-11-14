//! The `isoloom` binary itself: what each command prints and the exit code it gives, how an
//! error reads (a spec's problems with their fixes, an error's causes), imports that pass
//! validate, and secrets that never reach a generated file. Each test gets its own folders and
//! `ISOLOOM_HOME`.

#![expect(
    clippy::disallowed_methods,
    reason = "the tests run the binary and the tools that check its output: test code, not an adapter"
)]

mod commands;
mod docs;
mod errors;
mod imports;
mod secrets;
mod testing;
mod workflows;
