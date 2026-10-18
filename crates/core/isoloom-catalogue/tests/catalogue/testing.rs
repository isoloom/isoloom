//! Shared helpers: the fixture catalogue, read from disk and changed in memory, and a lab of one
//! service machine.

use std::collections::BTreeMap;
use std::path::Path;

use isoloom_catalogue::{Catalogue, Resolved};
use isoloom_spec::syntax::{self, Problem};

/// The fixture catalogue's files, by their path in it (`demo/entry.yml`).
pub(crate) fn fixture_files() -> std::io::Result<BTreeMap<String, String>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut out = BTreeMap::new();
    walk(&root, &root, &mut out)?;
    Ok(out)
}

/// Every file under `dir`, by its path from `root`.
fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let path = e?.path();
        if path.is_dir() {
            walk(root, &path, out)?;
        } else {
            let rel = path.strip_prefix(root).map_err(std::io::Error::other)?.to_string_lossy().replace('\\', "/");
            out.insert(rel, std::fs::read_to_string(&path)?);
        }
    }
    Ok(())
}

/// The catalogue of `files`.
pub(crate) fn load(files: &BTreeMap<String, String>) -> Result<Catalogue, isoloom_catalogue::Error> {
    Catalogue::from_files(files.iter().map(|(p, t)| (p.as_str(), t.as_str())))
}

/// The fixture catalogue.
pub(crate) fn demo() -> Result<Catalogue, Box<dyn std::error::Error>> {
    Ok(load(&fixture_files()?)?)
}

/// A lab whose machine `web` runs `service` on `os` with `with` (inline YAML), beside `app` on
/// the same network.
pub(crate) fn lab(os: &str, service: &str, with: &str) -> Result<syntax::Spec, Vec<Problem>> {
    syntax::parse(&format!(
        "version: 1\nname: t\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\nsecrets: {{ admin: {{ env: ADMIN }} }}\nmachines:\n  web: {{ os: {os}, service: {service}, with: {with}, networks: {{ lab: 10 }} }}\n  app: {{ os: {os}, networks: {{ lab: 20 }} }}\n"
    ))
}

/// `web` of [`lab`] resolved in `catalogue`: its first service.
pub(crate) fn resolve(catalogue: &Catalogue, os: &str, service: &str, with: &str) -> Result<Resolved, Vec<Problem>> {
    let mut found = catalogue.resolve(&lab(os, service, with)?, "web")?;
    if found.is_empty() { Err(Vec::new()) } else { Ok(found.remove(0)) }
}
