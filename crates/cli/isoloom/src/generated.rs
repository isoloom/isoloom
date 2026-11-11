//! A target's generated files on their way to disk: the project files they copy added, then
//! written under the project folder, with the list of what Isoloom wrote there.
//!
//! What Isoloom owns under `.isoloom/` is exactly the files it wrote, which `isoloom generate`
//! lists in [`LIST`]: only those can go stale. A file it wrote and no longer writes (a target the
//! lab no longer allows, a machine renamed) is removed by `generate` and reported by `check`;
//! anything else there is left alone: what a runner or the tools it starts write beside the
//! generated files (`.terraform.lock.hcl`, a Vagrant `.vagrant/` folder, a Terraform crash
//! log), and files a program embedding Isoloom wrote without the list. A folder
//! is removed only once removing Isoloom's files left it empty.
//!
//! The list is a file in the project, so it is read as untrusted: a line that isn't a plain path
//! under `.isoloom/` (`..`, an absolute path), or a file reached through a symbolic link, is an
//! error before anything is removed, so a removal never leaves `.isoloom/`. The folders a file is
//! written in are the project's too (a clone can hold `.isoloom/docker` as a link elsewhere): a
//! file whose path goes through a symbolic link, or is one, is an error before anything is
//! written, and each file is written beside its place and renamed into it, which replaces what
//! was there rather than writing through it.

use std::collections::BTreeSet;
use std::fs::Metadata;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use isoloom_targets::{GeneratedFile, Lab};

use isoloom_manifest::contract::OUTPUT_DIR;

use crate::error::{CliError, Result};

/// The list of the files `isoloom generate` wrote, relative to the project folder.
pub(crate) const LIST: &str = ".isoloom/.generated";

/// The list's first line: what it is.
const LIST_HEADER: &str = "# The files isoloom generate wrote here, which it removes once it no longer writes them. Don't edit.";

/// The generator's files plus the project's own files they copy into the output.
pub(crate) fn finish(lab: &Lab, mut files: Vec<GeneratedFile>, dir: &Path) -> Result<Vec<GeneratedFile>> {
    let copies = isoloom_targets::project_copies(lab, &files, dir)?;
    files.extend(copies);
    Ok(files)
}

/// Refuses `files` when one of them would be written through a symbolic link: its path goes
/// through one under `dir`, or is one. Nothing is written before.
pub(crate) fn writable<'a>(dir: &Path, files: impl IntoIterator<Item = &'a GeneratedFile>) -> Result<()> {
    for f in files {
        if let Reach::Linked(link) = reach(dir, &f.path)? {
            return Err(CliError::LinkedOutput { path: f.path.clone(), link });
        }
    }
    Ok(())
}

/// Writes one file under `dir`, creating its folders: into a fresh file beside it, then renamed
/// into its place, so what was there is replaced, never written through. [`writable`] first.
pub(crate) fn write_file(dir: &Path, f: &GeneratedFile) -> Result<()> {
    let path = dir.join(&f.path);
    let parent = path.parent().unwrap_or(dir);
    std::fs::create_dir_all(parent).map_err(CliError::writing(parent))?;
    let mut fresh = path.clone().into_os_string();
    fresh.push(".isoloom-new");
    let fresh = PathBuf::from(fresh);
    match std::fs::remove_file(&fresh) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(CliError::writing(&fresh)(e)),
        _ => {}
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&fresh)
        .and_then(|mut file| file.write_all(f.contents.as_bytes()))
        .map_err(CliError::writing(&fresh))?;
    std::fs::rename(&fresh, &path).map_err(CliError::writing(path))
}

/// The files the list names (none without one); a line naming anything but a file under
/// `.isoloom/` is an error.
fn listed(dir: &Path) -> Result<BTreeSet<String>> {
    let path = dir.join(LIST);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(e) => return Err(CliError::reading(path)(e)),
    };
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            if under_output(l) {
                Ok(l.to_owned())
            } else {
                Err(refused(l, format!("isn't a path under {OUTPUT_DIR}/")))
            }
        })
        .collect()
}

/// Whether `entry` is a relative path of plain names under `.isoloom/`: no `..`, no `.`, no root.
fn under_output(entry: &str) -> bool {
    let mut parts = Path::new(entry).components();
    let plain = parts.clone().all(|c| matches!(c, Component::Normal(_)));
    plain && parts.next() == Some(Component::Normal(OUTPUT_DIR.as_ref())) && parts.next().is_some()
}

/// What the way from a project folder to one of its paths found.
enum Reach {
    /// The path, or a folder on its way, isn't there.
    Missing,
    /// The path, reached through real folders only.
    Found(Metadata),
    /// The first symbolic link on the way, the path itself included.
    Linked(PathBuf),
}

/// `entry` under `dir`, each part of its path looked at without following a link: a symbolic
/// link on its way (`.isoloom` or `.isoloom/docker` linked elsewhere) could lead a write or a
/// removal out of the project.
fn reach(dir: &Path, entry: &str) -> Result<Reach> {
    let mut path = dir.to_path_buf();
    let mut found = None;
    for part in Path::new(entry).components() {
        path.push(part);
        match std::fs::symlink_metadata(&path) {
            Ok(m) if m.file_type().is_symlink() => return Ok(Reach::Linked(path)),
            Ok(m) => found = Some(m),
            Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => {
                return Ok(Reach::Missing);
            }
            Err(e) => return Err(CliError::reading(path)(e)),
        }
    }
    Ok(found.map_or(Reach::Missing, Reach::Found))
}

/// Whether `entry` is still a file under `dir`, reached through real folders only; a symbolic
/// link on its way is an error.
fn still_there(dir: &Path, entry: &str) -> Result<bool> {
    match reach(dir, entry)? {
        Reach::Linked(_) => Err(refused(entry, "is reached through a symbolic link".to_owned())),
        Reach::Missing => Ok(false),
        Reach::Found(m) => Ok(m.is_file()),
    }
}

/// The error for a list entry Isoloom won't remove, and `why`.
fn refused(entry: &str, why: String) -> CliError {
    CliError::GeneratedList { entry: entry.to_owned(), why }
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
    let mut stale = Vec::new();
    for p in listed(dir)?.into_iter().filter(|p| p.starts_with(scope) && !now.contains(p.as_str())) {
        if still_there(dir, &p)? {
            stale.push(p);
        }
    }
    Ok(stale)
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

/// The list a generation of `files` that removes `removed` leaves: what the list under `dir`
/// named but `removed`, and `files`. `generate` writes it and `check` compares it, as any
/// generated file; `None` when the generation writes and removes nothing, which leaves the list
/// as it is.
pub(crate) fn list(dir: &Path, files: &[GeneratedFile], removed: &[String]) -> Result<Option<GeneratedFile>> {
    if files.is_empty() && removed.is_empty() {
        return Ok(None);
    }
    let mut all = listed(dir)?;
    for r in removed {
        all.remove(r);
    }
    all.extend(files.iter().filter(|f| f.path.starts_with(&format!("{OUTPUT_DIR}/"))).map(|f| f.path.clone()));
    let text: String = std::iter::once(LIST_HEADER.to_owned()).chain(all).map(|l| l + "\n").collect();
    Ok(Some(GeneratedFile {
        path: LIST.to_owned(),
        contents: text,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_under_the_output_folder() {
        assert_eq!(LIST, format!("{OUTPUT_DIR}/.generated"));
    }
}
