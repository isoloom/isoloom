//! `isoloom import compose`: a draft from a Compose file.

use std::path::PathBuf;
use std::process::ExitCode;

use isoloom_core as core;

use super::draft::{Dest, source_of, write_draft};
use crate::Res;

/// The Compose file names looked for when none is given.
const COMPOSE_FILES: &[&str] = &["compose.yaml", "compose.yml", "docker-compose.yaml", "docker-compose.yml"];

pub fn run(file: Option<PathBuf>, dest: &Dest) -> Res<ExitCode> {
    let file = match file {
        Some(f) => f,
        None => COMPOSE_FILES
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
            .ok_or("no compose.yaml or docker-compose.yml here; pass the file")?,
    };
    let text = std::fs::read_to_string(&file).map_err(|e| format!("can't read {}: {e}", file.display()))?;
    let (folder, fallback, source) = source_of(&file)?;
    let draft = core::import::compose::draft(&text, &fallback, &source)?;
    write_draft(&draft, folder, dest)
}
