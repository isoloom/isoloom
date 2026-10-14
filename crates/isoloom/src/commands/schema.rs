//! `isoloom schema`: the JSON Schema of isoloom.yml, for editors; with `--events`, the schema
//! of the lifecycle commands' `--json` lines (events and errors).

use std::process::ExitCode;

use isoloom_core as core;

use crate::{Res, output};

#[derive(clap::Args)]
pub struct Args {
    /// The schema of the `--json` events and errors instead (also docs/events.schema.json).
    #[arg(long)]
    events: bool,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    if a.events {
        output::print_json(&core::events::schema())?;
    } else {
        output::print_json(&core::schema::schema())?;
    }
    Ok(ExitCode::SUCCESS)
}
