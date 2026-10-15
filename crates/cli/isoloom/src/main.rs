//! `isoloom`: describe an environment once, and convert it to each tool's own files.
//!
//! `cli` defines the command line and `commands` holds one module per subcommand. What they
//! share: the spec as a command loads it (`spec`), the target named (`target`), the files written
//! for a target (`generated`), the console a command writes to (`console`), the error that stops
//! a command (`error`), how it reads on stderr (`diagnostic`) and the exit code it gives (`exit`).

use std::io::Write as _;
use std::process::ExitCode;

use clap::Parser;

mod cli;
mod commands;
mod console;
mod diagnostic;
mod error;
mod exit;
mod generated;
mod spec;
mod target;

/// The command line's stack: clap builds every subcommand's parser on it, and Windows gives the
/// main thread only 1 MB, which a debug build outgrows (a stack overflow before any output).
const STACK: usize = 16 * 1024 * 1024;

fn main() -> ExitCode {
    let thread = std::thread::Builder::new()
        .name("isoloom".into())
        .stack_size(STACK)
        .spawn(|| run(cli::Cli::parse()));
    match thread {
        Ok(cli) => cli.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
        Err(e) => finish(&Err(error::CliError::ToolStart {
            program: "isoloom's command-line thread".to_owned(),
            source: e,
        })),
    }
}

/// Runs the command line, prints its error if it has one, and gives its exit code.
fn run(cli: cli::Cli) -> ExitCode {
    let profile = spec::Profile::new(cli.profile, std::env::var("ISOLOOM_PROFILE").ok());
    let outcome = {
        let (mut out, mut err) = (std::io::stdout().lock(), std::io::stderr().lock());
        commands::run(cli.command, &profile, &mut console::Console::new(&mut out, &mut err))
    };
    finish(&outcome)
}

/// The outcome's exit code, its error printed on stderr first.
fn finish(outcome: &error::Result<exit::Verdict>) -> ExitCode {
    if let Err(e) = outcome {
        // When stderr can't be written either, the exit code is all that's left to say.
        std::io::stderr().write_all(diagnostic::render(e).as_bytes()).unwrap_or(());
    }
    exit::code(outcome)
}
