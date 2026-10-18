//! A target's generated files on their way to disk: the defaults applied, the project files
//! they copy added, then written under the project folder.

use std::path::Path;

use isoloom_spec::defaults::Defaults;
use isoloom_targets::GeneratedFile;

use crate::error::{CliError, Result};

/// The generator's files with the defaults in effect applied, plus the project's own files
/// they copy into the output.
pub(crate) fn finish(files: Vec<GeneratedFile>, defaults: &Defaults, dir: &Path) -> Result<Vec<GeneratedFile>> {
    let mut files = isoloom_spec::defaults::apply_to_files(files, defaults);
    files.extend(isoloom_targets::project_copies(&files, dir)?);
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
