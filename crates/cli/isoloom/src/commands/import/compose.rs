//! `isoloom import compose`: a draft from a Compose file.

use std::path::PathBuf;

use super::draft::{self, Dest, Source};
use crate::console::Console;
use crate::error::{CliError, Result};
use crate::exit::Verdict;

/// The Compose file names looked for when none is given.
const COMPOSE_FILES: [&str; 4] = ["compose.yaml", "compose.yml", "docker-compose.yaml", "docker-compose.yml"];

/// Drafts the spec from `file`, else from the Compose file of the current folder.
pub(crate) fn run(file: Option<PathBuf>, dest: &Dest, console: &mut Console) -> Result<Verdict> {
    let file = match file {
        Some(f) => f,
        None => COMPOSE_FILES
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
            .ok_or_else(|| CliError::NothingToImport("no compose.yaml or docker-compose.yml here; pass the file".to_owned()))?,
    };
    let text = std::fs::read_to_string(&file).map_err(CliError::reading(&file))?;
    let source = Source::file(&file)?;
    let drafted = isoloom_import::compose::draft(&text, &source.fallback, &source.named)?;
    draft::write(&drafted, &source.folder, dest, console)
}
