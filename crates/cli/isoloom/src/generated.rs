//! A target's generated files on their way to disk: the defaults applied, the project files
//! they copy added, then written under the project folder.

use std::path::Path;

use crate::Res;

/// The generator's files with the defaults in effect applied, plus the project's own files
/// they copy into the output.
pub fn finish(files: Vec<isoloom_targets::GeneratedFile>, defaults: &isoloom_spec::defaults::Defaults, dir: &Path) -> Res<Vec<isoloom_targets::GeneratedFile>> {
    let mut files = isoloom_spec::defaults::apply_to_files(files, defaults);
    files.extend(isoloom_targets::project_copies(&files, dir)?);
    Ok(files)
}

/// Writes one file under `dir`, creating its folders.
pub fn write_file(dir: &Path, f: &isoloom_targets::GeneratedFile) -> Res<()> {
    let path = dir.join(&f.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &f.contents)?;
    Ok(())
}
