//! The pins the refresh works out: a repository key checked and hashed, each package's exact
//! version read from its repository's index (each index fetched once per run), what changed
//! against the lock on disk, and the newer upstream releases `--propose` lists.

use std::collections::HashMap;
use std::io::Read;

use isoloom_catalogue::{Definition, Family, KeyCheck, Lock, LockedKey, RepositoryFetch, Source};
use sha2::{Digest, Sha256};

use super::{fetch, index, pgp};
use crate::Res;
use crate::text::hex;

/// The repository indexes read so far, by address: each package's versions in it.
#[derive(Default)]
pub(crate) struct Indexes {
    read: HashMap<String, String>,
}

impl Indexes {
    /// The exact version of `package` that is upstream release `version` in `repositories`: the
    /// newest build of it.
    pub(crate) fn exact(&mut self, repositories: &[RepositoryFetch], package: &str, version: &str) -> Res<Option<String>> {
        let mut found = Vec::new();
        for r in repositories {
            found.extend(self.versions(r, package)?.into_iter().filter(|v| index::upstream(v, r.family) == version));
        }
        Ok(index::newest(&found).cloned())
    }

    /// Every version of `package` in repository `r`.
    pub(crate) fn versions(&mut self, r: &RepositoryFetch, package: &str) -> Res<Vec<String>> {
        let mut out = Vec::new();
        for url in addresses(r) {
            if !self.read.contains_key(&url) {
                let t = read_index(r.family, &url)?;
                self.read.insert(url.clone(), t);
            }
            let text = self.read.get(&url).map(String::as_str).unwrap_or_default();
            out.extend(match r.family {
                Family::RedHat => index::rpm(text, package),
                Family::Alpine => index::apk(text, package),
                _ => index::apt(text, package),
            });
        }
        Ok(out)
    }
}

/// The index addresses of repository `r` (one per apt component), for x86-64.
fn addresses(r: &RepositoryFetch) -> Vec<String> {
    let base = r.url.trim_end_matches('/').replace("$basearch", "x86_64");
    match r.family {
        Family::RedHat => vec![format!("{base}/repodata/repomd.xml")],
        Family::Alpine => vec![format!("{base}/x86_64/APKINDEX.tar.gz")],
        _ => r
            .components
            .iter()
            .map(|c| format!("{base}/dists/{}/{c}/binary-amd64/Packages.gz", r.suite.as_deref().unwrap_or_default()))
            .collect(),
    }
}

/// The index at `url`, as text: apt's `Packages` and the rpm metadata un-gzipped, Alpine's
/// `APKINDEX` out of its archive.
fn read_index(family: Family, url: &str) -> Res<String> {
    match family {
        Family::RedHat => {
            let repomd = fetch::text(url, &[])?;
            let primary = index::rpm_primary(&repomd).ok_or_else(|| format!("{url}: no primary metadata"))?;
            let base = url.trim_end_matches("repodata/repomd.xml");
            gunzip(&fetch::get(&format!("{base}{primary}"), &[])?)
        }
        Family::Alpine => {
            let tar = gunzip_bytes(&fetch::get(url, &[])?)?;
            // An APKINDEX archive is two: the signature's, then the index's.
            let mut archive = tar::Archive::new(tar.as_slice());
            archive.set_ignore_zeros(true);
            for file in archive.entries()? {
                let mut file = file?;
                if file.path()?.to_string_lossy() == "APKINDEX" {
                    let mut text = String::new();
                    file.read_to_string(&mut text)?;
                    return Ok(text);
                }
            }
            Err(format!("{url}: no APKINDEX in the archive").into())
        }
        _ => gunzip(&fetch::get(url, &[])?),
    }
}

/// `bytes` un-gzipped, as text.
fn gunzip(bytes: &[u8]) -> Res<String> {
    String::from_utf8(gunzip_bytes(bytes)?).map_err(|_| "an index that isn't text".into())
}

/// `bytes` un-gzipped, every gzip member of them (Alpine's index has two).
fn gunzip_bytes(bytes: &[u8]) -> Res<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::MultiGzDecoder::new(bytes).read_to_end(&mut out)?;
    Ok(out)
}

/// Repository `r`'s key, fetched and checked: its pin for the lock when it is a GPG key (its
/// fingerprint found among the file's primary keys); `None` for a key the entry pins by SHA-256
/// itself, after checking the file still has it.
pub(crate) fn key(r: &RepositoryFetch) -> Res<Option<LockedKey>> {
    let bytes = fetch::get(&r.key_url, &[])?;
    let sha256 = hex(&Sha256::digest(&bytes));
    match &r.key {
        KeyCheck::Fingerprint(f) => {
            let text = String::from_utf8(bytes).map_err(|_| format!("{}: a GPG key in ASCII armour", r.key_url))?;
            let found = pgp::fingerprints(&text).map_err(|e| format!("{}: {e}", r.key_url))?;
            if !found.contains(f) {
                return Err(format!(
                    "{}: no key with fingerprint {f} in the file (it has {}): don't pin it",
                    r.key_url,
                    found.join(", ")
                )
                .into());
            }
            Ok(Some(LockedKey {
                fingerprint: Some(f.clone()),
                sha256: sha256.parse()?,
            }))
        }
        KeyCheck::Sha256(want) if want.as_str() == sha256 => Ok(None),
        KeyCheck::Sha256(want) => Err(format!(
            "{}: its SHA-256 is {sha256}, not entry.yml's {want}: don't pin it before you know why",
            r.key_url
        )
        .into()),
    }
}

/// One difference between the lock on disk and upstream: `moved` when a pin changed under it.
pub(crate) struct Change {
    pub moved: bool,
    pub text: String,
}

/// What differs between `old` and `new`, the day aside.
pub(crate) fn changes(old: &Lock, new: &Lock) -> Vec<Change> {
    let mut out = Vec::new();
    let mut add = |moved, text: String| out.push(Change { moved, text });
    for (url, key) in &new.keys {
        match old.keys.get(url) {
            Some(k) if k.sha256 != key.sha256 => add(true, format!("the key {url} changed ({} → {})", k.sha256, key.sha256)),
            Some(_) => {}
            None => add(false, format!("the key {url} pinned")),
        }
    }
    for (version, release) in &new.releases {
        let before = old.releases.get(version);
        for (os, image) in &release.images {
            match before.and_then(|b| b.images.get(os)) {
                Some(i) if i.digest != image.digest => add(true, format!("{version} on {os}: the image moved ({} → {})", i.digest, image.digest)),
                Some(i) if i != image => add(false, format!("{version} on {os}: the image's platforms or process changed")),
                Some(_) => {}
                None => add(false, format!("{version} on {os}: the image pinned")),
            }
        }
        for (os, packages) in &release.packages {
            for (name, v) in packages {
                match before.and_then(|b| b.packages.get(os)).and_then(|p| p.get(name)) {
                    Some(o) if o != v => add(true, format!("{version} on {os}: {name} {o} → {v}")),
                    Some(_) => {}
                    None => add(false, format!("{version} on {os}: {name} {v} pinned")),
                }
            }
        }
    }
    for version in old.releases.keys().filter(|v| !new.releases.contains_key(*v)) {
        add(false, format!("{version} dropped: entry.yml no longer has it"));
    }
    out
}

/// The newer upstream releases of each vendor release's pinned packages, one line each.
pub(crate) fn newer(def: &Definition, indexes: &mut Indexes) -> Res<Vec<String>> {
    let mut out = Vec::new();
    let newest_listed = def.releases.iter().map(|r| &r.version).max();
    for release in def.releases.iter().filter(|r| r.source == Source::Vendor) {
        let Some(os) = release.os.first() else { continue };
        let fetches = def.fetches(release, *os)?;
        for package in &fetches.packages {
            let mut ups: Vec<String> = Vec::new();
            for r in &fetches.repositories {
                for v in indexes.versions(r, package.as_str())? {
                    let up = index::upstream(&v, r.family).to_owned();
                    let newer = up.parse::<isoloom_catalogue::Version>().is_ok_and(|u| newest_listed.is_none_or(|n| &u > n));
                    if newer && !ups.contains(&up) {
                        ups.push(up);
                    }
                }
            }
            if !ups.is_empty() {
                out.push(format!("{package} has newer releases than entry.yml lists on {os}: {}", ups.join(", ")));
            }
        }
    }
    Ok(out)
}
