//! `isoloom import terraform`: a draft from the `.tf` files of a folder.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::draft::{Dest, write_draft};
use crate::Res;

pub fn run(dir: Option<PathBuf>, dest: &Dest) -> Res<ExitCode> {
    let dir = dir.unwrap_or_else(|| PathBuf::from("."));
    let files = tf_files(&dir)?;
    if files.is_empty() {
        return Err(format!("no .tf files in {}", dir.display()).into());
    }
    let folder = dir.canonicalize()?;
    let fallback = folder.file_name().and_then(|n| n.to_str()).unwrap_or("env").to_string();
    let source = format!("{} (.tf files)", folder.file_name().and_then(|n| n.to_str()).unwrap_or("."));
    let draft = isoloom_import::terraform::draft(&files, &fallback, &source)?;
    write_draft(&draft, folder, dest)
}

/// The folder's `.tf` files, as (name, text), sorted.
fn tf_files(dir: &Path) -> Res<Vec<(String, String)>> {
    let mut files = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| format!("can't read {}: {e}", dir.display()))? {
        let p = e?.path();
        if p.extension().and_then(|x| x.to_str()) == Some("tf")
            && let Some(name) = p.file_name()
        {
            files.push((name.to_string_lossy().to_string(), std::fs::read_to_string(&p)?));
        }
    }
    files.sort();
    Ok(files)
}
