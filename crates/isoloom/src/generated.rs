//! A target's generated files on their way to disk: the defaults applied, the project files
//! they copy added, then written under the project folder.

use std::path::Path;

use isoloom_core as core;

use crate::Res;

/// The generator's files with the defaults in effect applied, plus the project's own files
/// they copy into the output.
pub fn finish(files: Vec<core::GeneratedFile>, defaults: &core::defaults::Defaults, dir: &Path) -> Res<Vec<core::GeneratedFile>> {
    let mut files = core::defaults::apply_to_files(files, defaults);
    files.extend(core::project_copies(&files, dir)?);
    Ok(files)
}

/// Writes one file under `dir`, creating its folders.
pub fn write_file(dir: &Path, f: &core::GeneratedFile) -> Res<()> {
    let path = dir.join(&f.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &f.contents)?;
    Ok(())
}

/// Writes every file under `dir`.
pub fn write_all(dir: &Path, files: &[core::GeneratedFile]) -> Res<()> {
    files.iter().try_for_each(|f| write_file(dir, f))
}
