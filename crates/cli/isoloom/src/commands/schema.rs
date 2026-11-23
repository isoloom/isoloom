//! `isoloom schema`: the JSON Schema of isoloom.yml, for editors: the one committed at
//! `schemas/spec-v1.schema.json`, each catalogue entry's `with:` rule included.

use super::{Console, Result, Verdict};
use crate::error::CliError;

#[derive(Debug, clap::Args)]
pub(crate) struct Args {}

/// Prints the schema.
pub(crate) fn run(_: &Args, console: &mut Console) -> Result<Verdict> {
    let catalogue = isoloom_catalogue::Catalogue::embedded().map_err(CliError::Catalogue)?;
    console.json(&isoloom_catalogue::schema::spec(catalogue))?;
    Ok(Verdict::Pass)
}
