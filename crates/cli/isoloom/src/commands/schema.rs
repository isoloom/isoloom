//! `isoloom schema`: the JSON Schema of isoloom.yml, for editors.

use std::process::ExitCode;

use crate::{Res, output};

#[derive(clap::Args)]
pub struct Args {}

pub fn run(_: &Args) -> Res<ExitCode> {
    output::print_json(&isoloom_spec::schema::schema())?;
    Ok(ExitCode::SUCCESS)
}
