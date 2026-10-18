//! Embeds the repository's `catalogue/` folder in the crate: every file of an entry (and the
//! catalogue's own `daemons.yml` and `collections.yml`) becomes one `(path, text)` pair of a
//! generated table, read with `include_str!`, so generating never reads the catalogue from disk
//! or the network. Only text is allowed: a file that isn't UTF-8 fails the build, naming it.
//! What the catalogue's tools write beside the entries (the generated schemas, the conformance
//! record) isn't embedded. Cargo runs this again whenever the folder changes.

use std::error::Error;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The files at the catalogue's top that aren't catalogue data: written by its tools.
const NOT_DATA: [&str; 3] = ["entry.schema.json", "contract.schema.json", "conformance.json"];

fn main() -> Result<(), Box<dyn Error>> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    // In the repository the catalogue lives at the workspace root; in a published package it is
    // vendored beside the crate (the `catalogue` symlink is copied in as real files when cargo
    // packages the crate). Prefer the repository copy, fall back to the vendored one.
    let repo_root = manifest.join("../../../catalogue");
    let root = if repo_root.is_dir() { repo_root } else { manifest.join("catalogue") };
    // Cargo's protocol: its instructions go on standard output.
    writeln!(std::io::stdout(), "cargo:rerun-if-changed={}", root.display())?;
    let mut files = Vec::new();
    if root.is_dir() {
        walk(&root, &root, &mut files)?;
    }
    files.sort();
    let mut out =
        String::from("/// Every file of `catalogue/`, by its path in the folder (`nginx/entry.yml`).\npub(crate) static FILES: &[(&str, &str)] = &[\n");
    for (rel, abs) in &files {
        writeln!(out, "    ({rel:?}, include_str!({abs:?})),")?;
    }
    out.push_str("];\n");
    std::fs::write(PathBuf::from(std::env::var("OUT_DIR")?).join("files.rs"), out)?;
    Ok(())
}

/// Every data file under `dir`: its path from `root` (with `/`) and its absolute path.
fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> Result<(), Box<dyn Error>> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let rel = path
            .strip_prefix(root)?
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if rel.split('/').any(|part| part.starts_with('.')) || NOT_DATA.contains(&rel.as_str()) {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out)?;
            continue;
        }
        if String::from_utf8(std::fs::read(&path)?).is_err() {
            return Err(format!("catalogue/{rel} isn't text (UTF-8): the catalogue embeds text files only").into());
        }
        out.push((rel, path.canonicalize()?.display().to_string()));
    }
    Ok(())
}
