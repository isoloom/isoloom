//! One module per subcommand: its arguments (`Args`) and what it does (`run`).

use std::process::ExitCode;

use crate::Res;
use crate::cli::Command;
use crate::spec::Profile;

pub mod check;
pub mod defaults;
pub mod docs;
pub mod generate;
pub mod graph;
pub mod import;
pub mod inspect;
pub mod message;
pub mod profiles;
pub mod report;
pub mod resources;
pub mod schema;
pub mod targets;
pub mod validate;

/// Runs the subcommand asked for, with the profile `main` worked out.
pub fn run(command: Command, profile: &Profile) -> Res<ExitCode> {
    match command {
        Command::Validate(a) => validate::run(&a, profile),
        Command::Targets(a) => targets::run(&a, profile),
        Command::Resources(a) => resources::run(&a, profile),
        Command::Generate(a) => generate::run(&a, profile),
        Command::Check(a) => check::run(&a, profile),
        Command::Defaults(a) => defaults::run(&a, profile),
        Command::Graph(a) => graph::run(&a, profile),
        Command::Report(a) => report::run(&a, profile),
        Command::Message(a) => message::run(&a, profile),
        Command::Inspect(a) => inspect::run(a, profile),
        Command::Profiles(a) => profiles::run(&a),
        Command::Schema(a) => schema::run(&a),
        Command::Import(i) => import::run(i),
        Command::Docs(a) => Ok(docs::run(&a)),
    }
}
