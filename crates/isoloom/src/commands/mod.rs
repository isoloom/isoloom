//! One module per subcommand: its arguments (`Args`) and what it does (`run`).

use std::process::ExitCode;

use crate::Res;
use crate::cli::Command;

pub mod capture;
pub mod chart;
pub mod check;
pub mod connect;
pub mod coverage;
pub mod defaults;
pub mod doctor;
pub mod down;
pub mod exec;
pub mod gc;
pub mod generate;
pub mod graph;
pub mod import;
pub mod inspect;
pub mod message;
pub mod profiles;
pub mod provision;
pub mod report;
pub mod reset;
pub mod resources;
pub mod run;
pub mod schema;
pub mod status;
pub mod targets;
pub mod tc;
pub mod test;
pub mod validate;

/// Runs the subcommand asked for.
pub fn run(command: Command) -> Res<ExitCode> {
    crate::output::use_json(command.json_events());
    match command {
        Command::Validate(a) => validate::run(&a),
        Command::Targets(a) => targets::run(&a),
        Command::Doctor(a) => doctor::run(&a),
        Command::Resources(a) => resources::run(&a),
        Command::Generate(a) => generate::run(&a),
        Command::Check(a) => check::run(&a),
        Command::Defaults(a) => defaults::run(&a),
        Command::Run(a) => run::run(&a),
        Command::Reset(a) => reset::run(&a),
        Command::Down(a) => down::run(&a),
        Command::Test(a) => test::run(&a),
        Command::Provision(a) => provision::run(&a),
        Command::Gc(a) => gc::run(&a),
        Command::Status(a) => status::run(&a),
        Command::Connect(a) => connect::run(&a),
        Command::Exec(a) => exec::run(&a),
        Command::Capture(a) => capture::run(&a),
        Command::Graph(a) => graph::run(&a),
        Command::Report(a) => report::run(&a),
        Command::Tc(a) => tc::run(a),
        Command::Message(a) => message::run(&a),
        Command::Inspect(a) => inspect::run(a),
        Command::Profiles(a) => profiles::run(&a),
        Command::Chart(a) => chart::run(&a),
        Command::Schema(a) => schema::run(&a),
        Command::Import(i) => import::run(i),
        Command::Coverage(a) => coverage::run(&a),
    }
}
