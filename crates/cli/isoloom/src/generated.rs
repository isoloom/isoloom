//! A target's generated files on their way to disk: the project files they copy added, then
//! written under the project folder.

use std::path::Path;

use isoloom_targets::GeneratedFile;

use crate::error::{CliError, Result};

/// The generator's files plus the project's own files they copy into the output.
pub(crate) fn finish(mut files: Vec<GeneratedFile>, dir: &Path) -> Result<Vec<GeneratedFile>> {
    let copies = isoloom_targets::project_copies(&files, dir)?;
    files.extend(copies);
    Ok(files)
}

/// Writes one file under `dir`, creating its folders.
pub(crate) fn write_file(dir: &Path, f: &GeneratedFile) -> Result<()> {
    let path = dir.join(&f.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(CliError::writing(parent))?;
    }
    std::fs::write(&path, &f.contents).map_err(CliError::writing(path))
}
