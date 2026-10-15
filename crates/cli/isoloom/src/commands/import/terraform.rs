//! `isoloom import terraform`: a draft from the `.tf` files of a folder.

use std::path::{Path, PathBuf};

use super::draft::{self, Dest, Source};
use crate::console::Console;
use crate::error::{CliError, Result};
use crate::exit::Verdict;

/// Drafts the spec from the `.tf` files of `dir`, else of the current folder.
pub(crate) fn run(dir: Option<PathBuf>, dest: &Dest, console: &mut Console) -> Result<Verdict> {
    let dir = dir.unwrap_or_else(|| PathBuf::from("."));
    let files = tf_files(&dir)?;
    if files.is_empty() {
        return Err(CliError::NothingToImport(format!("no .tf files in {}", dir.display())));
    }
    let source = Source::folder(&dir, ".tf files")?;
    let drafted = isoloom_import::terraform::draft(&files, &source.fallback, &source.named)?;
    draft::write(&drafted, &source.folder, dest, console)
}

/// The folder's `.tf` files, as (name, text), sorted.
fn tf_files(dir: &Path) -> Result<Vec<(String, String)>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(CliError::reading(dir))? {
        let path = entry.map_err(CliError::reading(dir))?.path();
        if path.extension().and_then(|x| x.to_str()) == Some("tf")
            && let Some(name) = path.file_name()
        {
            let text = std::fs::read_to_string(&path).map_err(CliError::reading(&path))?;
            files.push((name.to_string_lossy().into_owned(), text));
        }
    }
    files.sort();
    Ok(files)
}
