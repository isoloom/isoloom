//! A repository's copy for another host: a tar of its folder as git sees it (tracked and new
//! files, not the ignored ones), plus the `dev-link` files of a linked consumer, with their paths
//! moved to the copy's.

use std::fs::File;
use std::path::Path;
use std::process::Command;

use crate::{Res, dev_link, process, repo};

/// What to copy and where it lands on the other host.
pub struct Copy<'a> {
    /// The copy's folder name there.
    pub name: &'a str,
    /// The checkout here.
    pub dir: &'a Path,
    /// The folder holding the copies there (`<root>/isoloom`, `<root>/<name>`).
    pub root: &'a str,
    /// The Isoloom checkout here, which a linked consumer's `[patch]` points at.
    pub isoloom: &'a Path,
}

/// Writes the copy's tar to `out`; the notes say what was adjusted on the way.
pub fn pack(c: &Copy, out: &Path) -> Res<Vec<String>> {
    let listed = process::output(Command::new("git").arg("-C").arg(c.dir).args(["ls-files", "-z", "-co", "--exclude-standard"]))?;
    let mut tar = tar::Builder::new(File::create(out)?);
    tar.follow_symlinks(false);
    for path in listed.split('\0').filter(|p| !p.is_empty()) {
        let full = c.dir.join(path);
        if full.symlink_metadata().is_ok() {
            tar.append_path_with_name(&full, path)?;
        }
    }
    let mut notes = Vec::new();
    for config in dev_link::linked_configs(c.dir) {
        notes.extend(add_linked_config(&mut tar, c, &config)?);
    }
    tar.into_inner()?;
    Ok(notes)
}

/// A dev-link config with its paths moved to the copy's, and its lockfile.
fn add_linked_config(tar: &mut tar::Builder<File>, c: &Copy, config: &str) -> Res<Option<String>> {
    let text = std::fs::read_to_string(c.dir.join(config))?;
    let linked = dev_link::linked_isoloom(&text).unwrap_or_default();
    let here = repo::slashed(c.isoloom);
    let note = (linked != here).then(|| format!("note: {} is linked to {linked}; the copy uses {here}", c.dir.display()));
    let moved = text
        .replace(&format!("{linked}/"), &format!("{}/isoloom/", c.root))
        .replace(&format!("{}/", repo::slashed(c.dir)), &format!("{}/{}/", c.root, c.name));
    let mut header = tar::Header::new_gnu();
    header.set_size(moved.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, config, moved.as_bytes())?;
    let lock = Path::new(config).parent().unwrap_or(Path::new("")).join(dev_link::ALT_DIR).join("Cargo.lock");
    tar.append_path_with_name(c.dir.join(&lock), &lock)?;
    Ok(note)
}
