//! Each checkout's own build folders on the other hosts, as pure functions over paths and
//! listings (the Docker and SSH calls are in [`super::docker`] and [`super::remote`]).
//!
//! A run builds and tests only the checkouts it started from. Cargo decides whether a build is
//! fresh from file times, not contents, and each package's artifacts are named from its path
//! relative to its workspace, so two worktrees sharing one build folder can run each other's
//! test binaries. Each checkout's build folders are therefore named by a [`Key`] of its path and
//! carry that path, so the ones a deleted checkout left behind can be found and removed.
//! Downloads (Cargo's registry and git database, corepack's package managers, the pnpm store)
//! are content-addressed and stay shared.

use std::fmt;

use sha2::{Digest, Sha256};

use super::Repo;

/// The start of every Docker volume name check-all uses.
const PREFIX: &str = "isoloom-check-all";

/// The label on every Docker volume check-all creates; its value is what the volume holds.
pub(super) const LABEL: &str = "isoloom.check-all";

/// The label naming the checkout a build volume belongs to.
pub(super) const CHECKOUT_LABEL: &str = "isoloom.check-all.checkout";

/// What `docker volume ls --format` prints for [`parse_volumes`]: the name and both labels.
pub(super) const VOLUME_FORMAT: &str = "{{.Name}}\t{{.Label \"isoloom.check-all\"}}\t{{.Label \"isoloom.check-all.checkout\"}}";

/// Where the container keeps each repository's build folder (`<root>/<repo>`).
pub(super) const TARGET_ROOT: &str = "/check-all/target";

/// Where the container keeps the shared downloads.
pub(super) const CACHE_ROOT: &str = "/check-all/cache";

/// The volumes of the versions before keys, which every checkout shared.
const LEGACY_VOLUMES: [&str; 2] = ["isoloom-check-all-linux-docker", "isoloom-check-all-linux-docker-node-modules"];

/// The file in an SSH host's checkout folder that lists the checkouts it holds copies of.
pub(super) const MARKER: &str = "checkouts";

/// The line the folder listing of an SSH host starts each folder with.
pub(super) const FOLDER_LINE: &str = "@@folder ";

/// Hexadecimal digits in a key.
const KEY_DIGITS: usize = 12;

/// Names the build folders of one checkout (or one set of checkouts): the first 12 hexadecimal
/// digits of the SHA-256 of their paths, one per line.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct Key(String);

impl Key {
    /// The key of `paths`, in that order.
    #[must_use]
    pub(super) fn of<S: AsRef<str>>(paths: &[S]) -> Self {
        let mut hash = Sha256::new();
        for path in paths {
            hash.update(path.as_ref().as_bytes());
            hash.update(b"\n");
        }
        Key(crate::text::hex(hash.finalize().get(..KEY_DIGITS / 2).unwrap_or_default()))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether `name` has a key's shape, so it can be removed without quoting worries.
fn is_key(name: &str) -> bool {
    name.len() == KEY_DIGITS && name.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// What a check-all volume holds, as its [`LABEL`] says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Holds {
    /// The downloads every checkout shares.
    Cache,
    /// A repository checkout's Cargo build folder.
    Target,
    /// The launcher checkout's `node_modules` (the host's has the host's binaries).
    NodeModules,
}

impl Holds {
    /// Its label value.
    #[must_use]
    pub(super) fn label(self) -> &'static str {
        match self {
            Holds::Cache => "cache",
            Holds::Target => "target",
            Holds::NodeModules => "node-modules",
        }
    }
}

/// A Docker volume the container mounts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Volume {
    /// Its name.
    pub name: String,
    /// Where the container mounts it.
    pub at: String,
    /// What it holds.
    pub holds: Holds,
    /// The checkout it belongs to; none for the shared downloads.
    pub checkout: Option<String>,
}

/// The volumes the container mounts for `checkouts` (each repository and its checkout's path as
/// it is on disk): the shared downloads, then each checkout's build folder, and the launcher's
/// `node_modules`, each named `isoloom-check-all-<repo>-<key of its path>-<what>`.
#[must_use]
pub(super) fn docker_volumes(checkouts: &[(Repo, String)]) -> Vec<Volume> {
    let mut volumes = vec![Volume {
        name: format!("{PREFIX}-cache"),
        at: CACHE_ROOT.to_owned(),
        holds: Holds::Cache,
        checkout: None,
    }];
    for (repo, path) in checkouts {
        let own = |holds: Holds, at: String| Volume {
            name: format!("{PREFIX}-{}-{}-{}", repo.id(), Key::of(&[path]), holds.label()),
            at,
            holds,
            checkout: Some(path.clone()),
        };
        volumes.push(own(Holds::Target, format!("{TARGET_ROOT}/{}", repo.id())));
        if *repo == Repo::Launcher {
            volumes.push(own(Holds::NodeModules, format!("{path}/node_modules")));
        }
    }
    volumes
}

/// A volume as `docker volume ls` prints it with [`VOLUME_FORMAT`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Listed {
    /// Its name.
    pub name: String,
    /// Its [`LABEL`], empty when it has none.
    pub holds: String,
    /// Its [`CHECKOUT_LABEL`], empty when it has none.
    pub checkout: String,
}

/// The volumes of a `docker volume ls` listing printed with [`VOLUME_FORMAT`].
#[must_use]
pub(super) fn parse_volumes(text: &str) -> Vec<Listed> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let mut fields = line.trim_end_matches('\r').splitn(3, '\t').map(str::to_owned);
            Listed {
                name: fields.next().unwrap_or_default(),
                holds: fields.next().unwrap_or_default(),
                checkout: fields.next().unwrap_or_default(),
            }
        })
        .collect()
}

/// Whether check-all made `volume`: it carries the label, or has a name from before keys.
fn ours(volume: &Listed) -> bool {
    !volume.holds.is_empty() || LEGACY_VOLUMES.contains(&volume.name.as_str())
}

/// The volumes `--clean` removes: every one check-all made, for every checkout.
#[must_use]
pub(super) fn all_volumes(listed: &[Listed]) -> Vec<&str> {
    listed.iter().filter(|v| ours(v)).map(|v| v.name.as_str()).collect()
}

/// The volumes `--prune-volumes` removes: the build volumes of the checkouts `exists` says are
/// gone, and the ones from before keys.
#[must_use]
pub(super) fn stale_volumes<'a>(listed: &'a [Listed], exists: &dyn Fn(&str) -> bool) -> Vec<&'a str> {
    let stale = |v: &Listed| LEGACY_VOLUMES.contains(&v.name.as_str()) || (ours(v) && !v.checkout.is_empty() && !exists(&v.checkout));
    listed.iter().filter(|v| stale(v)).map(|v| v.name.as_str()).collect()
}

/// An SSH host's folder for a run from `checkouts` (the Isoloom checkout first): `<root>/<key>`,
/// the key over them all, so another set of checkouts never shares its copies or builds.
#[must_use]
pub(super) fn remote_folder<S: AsRef<str>>(root: &str, checkouts: &[S]) -> String {
    format!("{}/{}", root.trim_end_matches('/'), Key::of(checkouts))
}

/// The [`MARKER`]'s text: the checkouts, one per line.
#[must_use]
pub(super) fn marker<S: AsRef<str>>(checkouts: &[S]) -> String {
    checkouts.iter().flat_map(|c| [c.as_ref(), "\n"]).collect()
}

/// A folder under an SSH host's root, as its listing names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Folder {
    /// Its name.
    pub name: String,
    /// The checkouts its [`MARKER`] lists (none without one).
    pub checkouts: Vec<String>,
}

/// The folders of an SSH host's listing: a [`FOLDER_LINE`] per folder, then its marker's lines.
#[must_use]
pub(super) fn parse_folders(text: &str) -> Vec<Folder> {
    let mut folders: Vec<Folder> = Vec::new();
    for line in text.lines().map(|l| l.trim_end_matches('\r')).filter(|l| !l.is_empty()) {
        match (line.strip_prefix(FOLDER_LINE), folders.last_mut()) {
            (Some(name), _) => folders.push(Folder {
                name: name.to_owned(),
                checkouts: Vec::new(),
            }),
            (None, Some(folder)) => folder.checkouts.push(line.to_owned()),
            (None, None) => {}
        }
    }
    folders
}

/// The folders `--prune-volumes` removes from an SSH host: each keyed one whose Isoloom checkout
/// (its marker's first line) `exists` says is gone. Any other folder stays, since the root is a
/// setting and may hold more than check-all's folders (`--clean` removes the whole root,
/// including the shared `src` and `target` of the versions before keys).
#[must_use]
pub(super) fn stale_folders<'a>(folders: &'a [Folder], exists: &dyn Fn(&str) -> bool) -> Vec<&'a str> {
    let stale = |f: &Folder| is_key(&f.name) && f.checkouts.first().is_some_and(|isoloom| !exists(isoloom));
    folders.iter().filter(|f| stale(f)).map(|f| f.name.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn target_volume(repo: Repo, path: &str) -> String {
        docker_volumes(&[(repo, path.to_owned())])
            .into_iter()
            .find(|v| v.holds == Holds::Target)
            .map(|v| v.name)
            .unwrap_or_default()
    }

    #[test]
    fn two_worktrees_get_two_build_volumes() {
        let main = target_volume(Repo::Isoloom, "/Users/me/code/isoloom");
        let worktree = target_volume(Repo::Isoloom, "/Users/me/code/isoloom-1c-terraform");
        assert_ne!(main, worktree);
    }

    #[test]
    fn the_same_checkout_gets_the_same_build_volume_every_run() {
        let first = target_volume(Repo::Isoloom, "/Users/me/code/isoloom");
        assert_eq!(first, target_volume(Repo::Isoloom, "/Users/me/code/isoloom"));
        assert_eq!(first, format!("isoloom-check-all-isoloom-{}-target", Key::of(&["/Users/me/code/isoloom"])));
    }

    #[test]
    fn every_checkout_has_its_own_volumes_and_only_the_downloads_are_shared() {
        let checkouts = [
            (Repo::Isoloom, "/c/isoloom-wt".to_owned()),
            (Repo::Isothrum, "/c/isothrum".to_owned()),
            (Repo::Launcher, "/c/launcher".to_owned()),
        ];
        let volumes = docker_volumes(&checkouts);
        let shared: Vec<&str> = volumes.iter().filter(|v| v.checkout.is_none()).map(|v| v.name.as_str()).collect();
        assert_eq!(shared, ["isoloom-check-all-cache"]);
        let mounts: Vec<(&str, Holds)> = volumes.iter().map(|v| (v.at.as_str(), v.holds)).collect();
        assert_eq!(
            mounts,
            [
                ("/check-all/cache", Holds::Cache),
                ("/check-all/target/isoloom", Holds::Target),
                ("/check-all/target/isothrum", Holds::Target),
                ("/check-all/target/launcher", Holds::Target),
                ("/c/launcher/node_modules", Holds::NodeModules),
            ]
        );
        let launcher_key = Key::of(&["/c/launcher"]).to_string();
        assert!(
            volumes
                .iter()
                .filter(|v| v.checkout.as_deref() == Some("/c/launcher"))
                .all(|v| v.name.contains(&launcher_key))
        );
    }

    fn listed(name: &str, holds: &str, checkout: &str) -> Listed {
        Listed {
            name: name.to_owned(),
            holds: holds.to_owned(),
            checkout: checkout.to_owned(),
        }
    }

    #[test]
    fn a_volume_listing_reads_back_with_and_without_labels() {
        let text = "isoloom-check-all-cache\tcache\t\nother\t\t\nisoloom-check-all-isoloom-0123456789ab-target\ttarget\t/c/wt a\n\n";
        assert_eq!(
            parse_volumes(text),
            [
                listed("isoloom-check-all-cache", "cache", ""),
                listed("other", "", ""),
                listed("isoloom-check-all-isoloom-0123456789ab-target", "target", "/c/wt a"),
            ]
        );
    }

    #[test]
    fn pruning_removes_the_volumes_of_deleted_checkouts_and_the_shared_ones_from_before() {
        let volumes = [
            listed("isoloom-check-all-cache", "cache", ""),
            listed("isoloom-check-all-isoloom-aaaaaaaaaaaa-target", "target", "/c/isoloom"),
            listed("isoloom-check-all-isoloom-bbbbbbbbbbbb-target", "target", "/c/deleted-wt"),
            listed("isoloom-check-all-linux-docker", "", ""),
            listed("someone-elses", "", "/c/deleted-wt"),
        ];
        let exists = |path: &str| path == "/c/isoloom";
        assert_eq!(
            stale_volumes(&volumes, &exists),
            ["isoloom-check-all-isoloom-bbbbbbbbbbbb-target", "isoloom-check-all-linux-docker"]
        );
        assert_eq!(all_volumes(&volumes).len(), 4);
    }

    #[test]
    fn a_remote_folder_is_keyed_by_every_checkout_of_the_run() {
        let one = remote_folder("/home/me/isoloom-check-all/", &["/c/isoloom", "/c/isothrum"]);
        assert_eq!(one, format!("/home/me/isoloom-check-all/{}", Key::of(&["/c/isoloom", "/c/isothrum"])));
        assert_ne!(one, remote_folder("/home/me/isoloom-check-all", &["/c/isoloom-wt", "/c/isothrum"]));
        assert_ne!(one, remote_folder("/home/me/isoloom-check-all", &["/c/isoloom", "/c/isothrum-wt"]));
    }

    #[test]
    fn pruning_a_remote_root_removes_only_the_folders_of_deleted_checkouts() {
        let live = Key::of(&["/c/isoloom"]).to_string();
        let gone = Key::of(&["/c/deleted-wt"]).to_string();
        let text = format!(
            "@@folder src\r\n@@folder target\n@@folder {live}\n{}@@folder {gone}\n{}@@folder notes\n",
            marker(&["/c/isoloom", "/c/isothrum"]),
            marker(&["/c/deleted-wt", "/c/isothrum"])
        );
        let folders = parse_folders(&text);
        assert_eq!(folders.len(), 5);
        let exists = |path: &str| path != "/c/deleted-wt";
        assert_eq!(stale_folders(&folders, &exists), [gone.as_str()]);
    }

    #[test]
    fn a_marked_folder_without_a_key_name_is_never_removed() {
        let folders = [Folder {
            name: "x'; rm -rf ~".to_owned(),
            checkouts: vec!["/c/gone".to_owned()],
        }];
        assert_eq!(stale_folders(&folders, &|_| false), Vec::<&str>::new());
    }

    proptest! {
        #[test]
        fn keys_are_twelve_hex_digits_and_tell_paths_apart(a in "/[a-z0-9/ ._-]{1,40}", b in "/[a-z0-9/ ._-]{1,40}") {
            let (ka, kb) = (Key::of(&[&a]), Key::of(&[&b]));
            prop_assert!(is_key(&ka.to_string()));
            prop_assert_eq!(&ka, &Key::of(&[&a]));
            prop_assert_eq!(a == b, ka == kb);
        }

        #[test]
        fn any_listed_volume_reads_back(name in "[a-z0-9_.-]{1,30}", holds in "[a-z-]{0,12}", checkout in "[^\t\r\n]{0,40}") {
            let parsed = parse_volumes(&format!("{name}\t{holds}\t{checkout}\n"));
            prop_assert_eq!(parsed, vec![listed(&name, &holds, &checkout)]);
        }
    }
}
