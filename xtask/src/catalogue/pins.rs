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
        // An archive suite (apt-archive.postgresql.org's) may publish its index uncompressed
        // only, beside a bzip2 one: read the plain file then.
        _ => match fetch::get(url, &[]) {
            Ok(bytes) => gunzip(&bytes),
            Err(gz) => {
                let plain = url.strip_suffix(".gz").unwrap_or(url);
                String::from_utf8(fetch::get(plain, &[]).map_err(|_| gz)?).map_err(|_| format!("{plain} isn't text").into())
            }
        },
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

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::io::Write;

    use isoloom_catalogue::Lock;
    #[cfg(unix)]
    use isoloom_catalogue::{Family, KeyCheck, RepositoryFetch};

    #[cfg(unix)]
    use super::super::fetch::testing::Web;
    use super::*;

    /// A repository of `family` at `url`, its key pinned by SHA-256 `sha`: nginx's first one
    /// (the catalogue makes them), every field then set.
    #[cfg(unix)]
    fn repository(family: Family, url: &str, sha: &str) -> RepositoryFetch {
        let text = std::fs::read_to_string(crate::repo::root().join("catalogue/nginx/entry.yml")).unwrap();
        let def: Definition = serde_yaml_ng::from_str(&text).unwrap();
        let mut r = def
            .releases
            .iter()
            .flat_map(|release| release.os.iter().map(move |os| (release, *os)))
            .find_map(|(release, os)| def.fetches(release, os).ok()?.repositories.into_iter().next())
            .expect("nginx adds a repository");
        r.url = url.to_owned();
        r.family = family;
        r.suite = Some("bookworm".to_owned());
        r.components = vec!["nginx".to_owned()];
        r.key_url = "https://example.org/key.asc".to_owned();
        r.key = KeyCheck::Sha256(sha.parse().unwrap());
        r
    }

    #[cfg(unix)]
    fn gz(bytes: &[u8]) -> Vec<u8> {
        let mut out = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        out.write_all(bytes).unwrap();
        out.finish().unwrap()
    }

    #[cfg(unix)]
    const PACKAGES: &str =
        "Package: nginx\nVersion: 1.30.6-1~bookworm\n\nPackage: nginx\nVersion: 1.30.6-2~bookworm\n\nPackage: nginx\nVersion: 1.31.0-1~bookworm\n";

    #[cfg(unix)]
    #[test]
    fn an_apt_index_is_read_once_and_the_newest_build_of_the_release_wins() {
        let web = Web::new();
        let url = "https://nginx.example/packages/debian/dists/bookworm/nginx/binary-amd64/Packages.gz";
        web.serves(url, &[], &gz(PACKAGES.as_bytes()));
        let r = repository(Family::Debian, "https://nginx.example/packages/debian/", &"0".repeat(64));
        let mut indexes = Indexes::default();
        let found = web.browse(|| {
            let exact = indexes.exact(std::slice::from_ref(&r), "nginx", "1.30.6").unwrap();
            let none = indexes.exact(std::slice::from_ref(&r), "nginx", "1.29.0").unwrap();
            (exact, none)
        });
        assert_eq!(found, (Some("1.30.6-2~bookworm".to_owned()), None));
        assert_eq!(web.calls().len(), 1, "the index is fetched once: {:?}", web.calls());
    }

    #[cfg(unix)]
    #[test]
    fn an_apt_suite_publishing_its_index_uncompressed_only_is_read_plain() {
        let web = Web::new();
        let plain = "https://archive.example/dists/bookworm/nginx/binary-amd64/Packages";
        web.serves(plain, &[], PACKAGES.as_bytes());
        let r = repository(Family::Debian, "https://archive.example", &"0".repeat(64));
        let versions = web.browse(|| Indexes::default().versions(&r, "nginx")).unwrap();
        assert_eq!(versions, ["1.30.6-1~bookworm", "1.30.6-2~bookworm", "1.31.0-1~bookworm"]);
        // Neither there: the gzip's failure is the one said.
        let gone = repository(Family::Debian, "https://gone.example", &"0".repeat(64));
        let e = web.browse(|| Indexes::default().versions(&gone, "nginx")).unwrap_err().to_string();
        assert!(
            e.starts_with("GET https://gone.example/dists/bookworm/nginx/binary-amd64/Packages.gz failed: "),
            "{e}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_rpm_index_is_found_through_repomd_for_x86_64() {
        let web = Web::new();
        let base = "https://nginx.example/packages/rhel/9/x86_64";
        let repomd = r#"<repomd><data type="primary"><location href="repodata/abc-primary.xml.gz"/></data></repomd>"#;
        let primary = r#"<metadata><package type="rpm"><name>nginx</name><arch>x86_64</arch><version ver="1.30.6" rel="1.el9.ngx"/></package></metadata>"#;
        web.serves(&format!("{base}/repodata/repomd.xml"), &[], repomd.as_bytes()).serves(
            &format!("{base}/repodata/abc-primary.xml.gz"),
            &[],
            &gz(primary.as_bytes()),
        );
        let r = repository(Family::RedHat, "https://nginx.example/packages/rhel/9/$basearch/", &"0".repeat(64));
        let versions = web.browse(|| Indexes::default().versions(&r, "nginx")).unwrap();
        assert_eq!(versions, ["1.30.6-1.el9.ngx"]);
        web.serves(&format!("{base}/repodata/repomd.xml"), &[], b"<repomd></repomd>");
        let e = web.browse(|| Indexes::default().versions(&r, "nginx")).unwrap_err().to_string();
        assert_eq!(e, format!("{base}/repodata/repomd.xml: no primary metadata"));
    }

    #[cfg(unix)]
    #[test]
    fn an_alpine_index_is_the_second_archive_of_two() {
        let web = Web::new();
        let url = "https://nginx.example/packages/alpine/v3.24/main/x86_64/APKINDEX.tar.gz";
        let signed = [
            gz(&crate::testing::tar_of(".SIGN.RSA.nginx.rsa.pub", b"sig")),
            gz(&crate::testing::tar_of("APKINDEX", b"P:nginx\nV:1.30.6-r1\n")),
        ]
        .concat();
        web.serves(url, &[], &signed);
        let r = repository(Family::Alpine, "https://nginx.example/packages/alpine/v3.24/main", &"0".repeat(64));
        assert_eq!(web.browse(|| Indexes::default().versions(&r, "nginx")).unwrap(), ["1.30.6-r1"]);
        web.serves(url, &[], &gz(&crate::testing::tar_of("other", b"x")));
        let e = web.browse(|| Indexes::default().versions(&r, "nginx")).unwrap_err().to_string();
        assert_eq!(e, format!("{url}: no APKINDEX in the archive"));
    }

    #[cfg(unix)]
    #[test]
    fn a_key_pinned_by_its_sha256_is_checked_against_the_file() {
        let web = Web::new();
        web.serves("https://example.org/key.asc", &[], b"key");
        let sha = hex(&Sha256::digest(b"key"));
        assert_eq!(web.browse(|| key(&repository(Family::Debian, "https://x", &sha))).ok(), Some(None));
        let other = "0".repeat(64);
        let e = web.browse(|| key(&repository(Family::Debian, "https://x", &other))).unwrap_err().to_string();
        assert_eq!(
            e,
            format!("https://example.org/key.asc: its SHA-256 is {sha}, not entry.yml's {other}: don't pin it before you know why")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_gpg_key_must_be_armoured_text_holding_the_fingerprint() {
        let web = Web::new();
        web.serves("https://example.org/key.asc", &[], &[0xff, 0x00]);
        let mut r = repository(Family::Debian, "https://x", &"0".repeat(64));
        r.key = KeyCheck::Fingerprint("A".repeat(40));
        let binary = web.browse(|| key(&r)).unwrap_err().to_string();
        assert_eq!(binary, "https://example.org/key.asc: a GPG key in ASCII armour");
        web.serves("https://example.org/key.asc", &[], b"no key here");
        let no_block = web.browse(|| key(&r)).unwrap_err().to_string();
        assert_eq!(no_block, "https://example.org/key.asc: no public key block in the file");
    }

    /// A lock read from its YAML.
    fn lock(text: &str) -> Lock {
        serde_yaml_ng::from_str(text).unwrap()
    }

    const OLD: &str = "refreshed: 2026-10-16
keys:
  https://example.org/key.asc:
    sha256: abababababababababababababababababababababababababababababababab
releases:
  1.30.6:
    images:
      debian@13:
        digest: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
        platforms: [amd64]
    packages:
      debian@12:
        nginx: 1.30.6-1~bookworm
  1.29.0: {}
";

    #[test]
    fn a_moved_pin_is_named_as_moved_and_a_new_or_dropped_one_is_not() {
        let new = OLD
            .replace(&"ab".repeat(32), &"cd".repeat(32))
            .replace(&"a".repeat(64), &"b".repeat(64))
            .replace("1.30.6-1~bookworm", "1.30.6-2~bookworm")
            .replace("  1.29.0: {}\n", "  1.31.0:\n    images:\n      alpine@3.24:\n        digest: sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\n        platforms: [amd64]\n");
        let found: Vec<(bool, String)> = changes(&lock(OLD), &lock(&new)).into_iter().map(|c| (c.moved, c.text)).collect();
        assert_eq!(
            found,
            [
                (
                    true,
                    format!("the key https://example.org/key.asc changed ({} → {})", "ab".repeat(32), "cd".repeat(32))
                ),
                (
                    true,
                    format!("1.30.6 on debian@13: the image moved (sha256:{} → sha256:{})", "a".repeat(64), "b".repeat(64))
                ),
                (true, "1.30.6 on debian@12: nginx 1.30.6-1~bookworm → 1.30.6-2~bookworm".to_owned()),
                (false, "1.31.0 on alpine@3.24: the image pinned".to_owned()),
                (false, "1.29.0 dropped: entry.yml no longer has it".to_owned()),
            ]
        );
    }

    #[test]
    fn the_same_pins_are_no_change_and_new_ones_are_pinned_not_moved() {
        assert_eq!(changes(&lock(OLD), &lock(OLD)).len(), 0);
        let found: Vec<(bool, String)> = changes(&Lock::default(), &lock(OLD)).into_iter().map(|c| (c.moved, c.text)).collect();
        assert_eq!(
            found,
            [
                (false, "the key https://example.org/key.asc pinned".to_owned()),
                (false, "1.30.6 on debian@13: the image pinned".to_owned()),
                (false, "1.30.6 on debian@12: nginx 1.30.6-1~bookworm pinned".to_owned()),
            ]
        );
        let platforms = OLD.replace("platforms: [amd64]", "platforms: [amd64, arm64]");
        let texts: Vec<String> = changes(&lock(OLD), &lock(&platforms)).into_iter().map(|c| c.text).collect();
        assert_eq!(texts, ["1.30.6 on debian@13: the image's platforms or process changed"]);
    }
}
