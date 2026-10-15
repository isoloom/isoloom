//! One module per subcommand: its arguments (`Args`) and what it does (`run`), writing to the
//! console and giving its verdict, or the error that stopped it.

pub(crate) mod check;
pub(crate) mod defaults;
pub(crate) mod docs;
pub(crate) mod generate;
pub(crate) mod graph;
pub(crate) mod import;
pub(crate) mod inspect;
pub(crate) mod message;
pub(crate) mod profiles;
pub(crate) mod report;
pub(crate) mod resources;
pub(crate) mod schema;
pub(crate) mod targets;
pub(crate) mod validate;

use std::path::PathBuf;

use crate::cli::Command;
use crate::console::Console;
use crate::error::{CliError, Result};
use crate::exit::Verdict;
use crate::spec::{self, Profile};

// The project folder a command works in (holding isoloom.yml), the current one unless named;
// flattened into each command's arguments (no doc comment: clap would print it in `--help`).
#[derive(Debug, clap::Args)]
pub(crate) struct Project {
    #[arg(default_value = ".")]
    pub(crate) dir: PathBuf,
}

// The arguments of the commands that read a project and answer as text or `--json`
// (`profiles`, `resources`, `targets`).
#[derive(Debug, clap::Args)]
pub(crate) struct Listing {
    #[command(flatten)]
    pub(crate) project: Project,
    #[arg(long)]
    pub(crate) json: bool,
}

/// Runs the subcommand asked for, with the profile `main` worked out.
pub(crate) fn run(command: Command, profile: &Profile, console: &mut Console) -> Result<Verdict> {
    match command {
        Command::Validate(a) => validate::run(&a, profile, console),
        Command::Targets(a) => targets::run(&a, profile, console),
        Command::Resources(a) => resources::run(&a, profile, console),
        Command::Generate(a) => generate::run(&a, profile, console),
        Command::Check(a) => check::run(&a, profile, console),
        Command::Defaults(a) => defaults::run(&a, profile, console),
        Command::Graph(a) => graph::run(&a, profile, console),
        Command::Report(a) => report::run(&a, profile, console),
        Command::Message(a) => message::run(&a, profile, console),
        Command::Inspect(a) => inspect::run(a, profile, console),
        Command::Profiles(a) => profiles::run(&a, console),
        Command::Schema(a) => schema::run(&a, console),
        Command::Import(i) => import::run(i, console),
        Command::Docs(a) => docs::run(&a, console),
    }
}
