//! A target's generated files on their way to disk: the project files they copy added, then
//! written under the project folder, with the list of what Isoloom wrote there.
//!
//! What Isoloom owns under `.isoloom/` is exactly the files it wrote, which `isoloom generate`
//! lists in [`LIST`]: only those can go stale. A file it wrote and no longer writes (a target the
//! lab no longer allows, a machine renamed) is removed by `generate` and reported by `check`;
//! anything else there is left alone: what a runner or the tools it starts write beside the
//! generated files (`.terraform.lock.hcl`, `.isoloom-reach/`, a Vagrant `.vagrant/` folder, a
//! Terraform crash log), and files a program embedding Isoloom wrote without the list. A folder
//! is removed only once removing Isoloom's files left it empty.

use std::collections::BTreeSet;
use std::path::Path;

use isoloom_targets::{GeneratedFile, OUTPUT_DIR};

use crate::error::{CliError, Result};

/// The list of the files `isoloom generate` wrote, relative to the project folder.
pub(crate) const LIST: &str = ".isoloom/.generated";

/// The list's first line: what it is.
const LIST_HEADER: &str = "# The files isoloom generate wrote here, which it removes once it no longer writes them. Don't edit.";

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

/// The files the list names (none without one).
fn listed(dir: &Path) -> Result<BTreeSet<String>> {
    let path = dir.join(LIST);
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(text.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_owned).collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(e) => Err(CliError::reading(path)(e)),
    }
}

/// The folder a generation covers: every file under `.isoloom/` when it generated every
/// target, else the one target's own folder (`.isoloom/<target>/`; a folder several targets
/// write, `docker/` for `docker-vm`, belongs to the target it is named after).
pub(crate) fn scope(target: Option<&str>) -> String {
    target.map_or_else(|| format!("{OUTPUT_DIR}/"), |t| format!("{OUTPUT_DIR}/{t}/"))
}

/// The files Isoloom wrote under `scope` (listed) that `files` no longer has, and that are still
/// there.
pub(crate) fn stale(dir: &Path, files: &[GeneratedFile], scope: &str) -> Result<Vec<String>> {
    let now: BTreeSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    Ok(listed(dir)?
        .into_iter()
        .filter(|p| p.starts_with(scope) && !now.contains(p.as_str()) && dir.join(p).is_file())
        .collect())
}

/// Removes the `stale` files, then each folder of theirs that is left empty, up to `.isoloom/`.
pub(crate) fn remove(dir: &Path, stale: &[String]) -> Result<()> {
    let root = dir.join(OUTPUT_DIR);
    for p in stale {
        let path = dir.join(p);
        std::fs::remove_file(&path).map_err(CliError::writing(&path))?;
        let mut folder = path.parent();
        while let Some(f) = folder.filter(|f| f.starts_with(&root) && *f != root) {
            // Not empty (a runner's file, another generated one): kept, and its parents too.
            if std::fs::remove_dir(f).is_err() {
                break;
            }
            folder = f.parent();
        }
    }
    Ok(())
}

/// The list after a generation: what it listed but `removed`, and `files`.
pub(crate) fn write_list(dir: &Path, files: &[GeneratedFile], removed: &[String]) -> Result<()> {
    let mut all = listed(dir)?;
    for r in removed {
        all.remove(r);
    }
    all.extend(files.iter().filter(|f| f.path.starts_with(&format!("{OUTPUT_DIR}/"))).map(|f| f.path.clone()));
    let text: String = std::iter::once(LIST_HEADER.to_owned()).chain(all).map(|l| l + "\n").collect();
    write_file(
        dir,
        &GeneratedFile {
            path: LIST.to_owned(),
            contents: text,
        },
    )
}
