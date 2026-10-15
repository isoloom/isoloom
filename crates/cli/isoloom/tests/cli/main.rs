//! The `isoloom` binary itself: what each command prints and the exit code it gives, how an
//! error reads (a spec's problems with their fixes, an error's causes), imports that pass
//! validate, and secrets that never reach a generated file. Each test gets its own folders and
//! `ISOLOOM_HOME`.

mod commands;
mod errors;
mod imports;
mod secrets;
mod testing;
