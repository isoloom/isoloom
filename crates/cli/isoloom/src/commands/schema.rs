//! `isoloom schema`: the JSON Schema of isoloom.yml, for editors.

use super::{Console, Result, Verdict};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {}

/// Prints the schema.
pub(crate) fn run(_: &Args, console: &mut Console) -> Result<Verdict> {
    console.json(&isoloom_spec::schema::schema())?;
    Ok(Verdict::Pass)
}
