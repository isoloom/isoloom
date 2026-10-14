//! `isoloom schema`: the JSON Schema of isoloom.yml, for editors.

use std::process::ExitCode;

use isoloom_core as core;

use crate::{Res, output};

pub fn run() -> Res<ExitCode> {
    output::print_json(&core::schema::schema())?;
    Ok(ExitCode::SUCCESS)
}
