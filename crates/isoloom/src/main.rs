//! `isoloom`: describe an environment once, run it anywhere.
//!
//! `cli` holds the command line's definition and `commands` one file per subcommand. The other
//! modules are what several commands share: the spec with its profile (`spec`), the target to
//! use (`target`), the files written for it (`generated`, `prepare`), the secrets handed to it (`secrets`), the tools each target runs
//! with (`tools`), and a running environment's machines (`machine`, `external`).

// The command line prints its results: that is its interface, not stray debugging output.
#![allow(clippy::print_stdout)]

use std::process::ExitCode;

use clap::Parser;

mod cli;
mod cloud_services;
mod commands;
mod external;
mod failure;
mod generated;
mod machine;
mod output;
mod prepare;
mod secrets;
mod spec;
mod target;
mod tools;

/// What a command returns: its exit code, or the error printed as `error: ...` (exit code 2;
/// with `--json`, the error document `failure` describes).
type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn main() -> ExitCode {
    // The command line runs on a thread of its own with a larger stack: clap builds every
    // subcommand's parser on the stack, and Windows gives the main thread only 1 MB, which a
    // debug build outgrows (a stack overflow before any output).
    let thread = std::thread::Builder::new()
        .name("isoloom".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| match run(cli::Cli::parse()) {
            Ok(code) => code,
            Err(e) => {
                // With `--json`, the error document on stdout; otherwise the message on stderr.
                if !output::print_error(e.as_ref()) {
                    eprintln!("error: {e}");
                }
                ExitCode::from(2)
            }
        });
    match thread {
        Ok(cli) => cli.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
        Err(e) => {
            eprintln!("error: can't start the command line's thread: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: cli::Cli) -> Res<ExitCode> {
    spec::set_profile(cli.profile);
    commands::run(cli.command)
}
