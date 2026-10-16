//! `cargo xtask catalogue refresh [<entry>…] [--check] [--update] [--propose]` (design §4.8):
//! the only code that fetches anything for the catalogue. For each entry it pins every
//! repository key (its GPG fingerprint checked, its file's SHA-256 recorded), every official
//! image (its tag's multi-platform digest, platforms, entrypoint and command) and each vendor
//! package's exact version per OS, into `lock.yml`; and it pins the Galaxy collections into
//! `collections.yml`. A pin that upstream moved (a digest, a key, a package build) fails loudly
//! unless `--update` accepts it; `--check` writes nothing and exits 1 on any drift.

use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use isoloom_catalogue::{Collection, Collections, Definition, Fetches, Lock, LockedImage, LockedRelease, Release, Source};
use isoloom_spec::syntax::OsRef;

use super::pins;
use crate::{Res, repo};

/// The arguments of `cargo xtask catalogue refresh`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// The entries to refresh (default: every entry).
    entries: Vec<String>,
    /// Write nothing: exit 1 when a pin differs from upstream (the drift job).
    #[arg(long)]
    check: bool,
    /// Accept pins upstream moved (a rebuilt image, a re-signed key, a new package build).
    #[arg(long, conflicts_with = "check")]
    update: bool,
    /// Also list newer upstream releases, without adding them.
    #[arg(long)]
    propose: bool,
}

/// Runs the refresh.
pub(crate) fn run(args: &Args) -> Res<ExitCode> {
    let dir = repo::root().join("catalogue");
    let names = if args.entries.is_empty() { entries(&dir)? } else { args.entries.clone() };
    let mut indexes = pins::Indexes::default();
    let mut drift = Vec::new();
    for name in &names {
        let def: Definition =
            serde_yaml_ng::from_str(&std::fs::read_to_string(dir.join(name).join("entry.yml"))?).map_err(|e| format!("catalogue/{name}/entry.yml: {e}"))?;
        let path = dir.join(name).join("lock.yml");
        let old: Lock = match std::fs::read_to_string(&path) {
            Ok(text) => serde_yaml_ng::from_str(&text).map_err(|e| format!("catalogue/{name}/lock.yml: {e}"))?,
            Err(_) => Lock::default(),
        };
        let new = lock(&def, &old, &mut indexes)?;
        let changes = pins::changes(&old, &new);
        accept(name, &changes, args)?;
        if args.propose {
            for line in pins::newer(&def, &mut indexes)? {
                println!("catalogue refresh: {name}: {line}");
            }
        }
        if changes.is_empty() {
            println!("catalogue refresh: {name}: current");
        } else if args.check {
            drift.extend(changes.iter().map(|c| format!("{name}: {}", c.text)));
        } else {
            write_lock(&path, name, new)?;
            println!("catalogue refresh: {name}: {} pin(s) written to catalogue/{name}/lock.yml", changes.len());
        }
    }
    drift.extend(collections(&dir, args)?);
    for line in &drift {
        println!("catalogue refresh: drift: {line}");
    }
    Ok(if drift.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// Every entry of the catalogue: each folder with an `entry.yml`.
fn entries(dir: &Path) -> Res<Vec<String>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let path = e?.path();
        if path.join("entry.yml").is_file()
            && let Some(name) = path.file_name()
        {
            out.push(name.to_string_lossy().into_owned());
        }
    }
    out.sort();
    Ok(out)
}

/// Fails loudly on a pin upstream moved, unless `--update` accepts it (`--check` reports it).
fn accept(name: &str, changes: &[pins::Change], args: &Args) -> Res<()> {
    let moved: Vec<&str> = changes.iter().filter(|c| c.moved).map(|c| c.text.as_str()).collect();
    if moved.is_empty() || args.update || args.check {
        return Ok(());
    }
    Err(format!(
        "{name}: upstream moved under its pins:\n  {}\nlook at why (a rebuilt image is normal, a re-signed key is not), then run `cargo xtask catalogue refresh {name} --update`",
        moved.join("\n  ")
    )
    .into())
}

/// The entry's pins, from upstream (the day kept from `old`, set when written).
fn lock(def: &Definition, old: &Lock, indexes: &mut pins::Indexes) -> Res<Lock> {
    let mut lock = Lock {
        refreshed: old.refreshed.clone(),
        ..Lock::default()
    };
    for release in &def.releases {
        let mut locked = LockedRelease::default();
        for os in &release.os {
            let fetches = def
                .fetches(release, *os)
                .map_err(|e| format!("{} {} on {os}: {e}", def.name, release.version))?;
            for r in &fetches.repositories {
                if let Some(key) = pins::key(r)? {
                    lock.keys.insert(r.key_url.parse()?, key);
                }
            }
            if let Some(tag) = &fetches.image {
                locked.images.insert(*os, image(tag)?);
            }
            if release.source == Source::Vendor && !fetches.packages.is_empty() {
                locked.packages.insert(*os, packages(release, *os, &fetches, indexes)?);
            }
        }
        lock.releases.insert(release.version.clone(), locked);
    }
    Ok(lock)
}

/// Image `tag`, pinned.
fn image(tag: &str) -> Res<LockedImage> {
    let pinned = super::registry::pin(tag)?;
    let platforms = pinned.platforms.iter().map(|p| serde_yaml_ng::from_str(p)).collect::<Result<_, _>>()?;
    Ok(LockedImage {
        digest: pinned.digest.parse()?,
        platforms,
        entrypoint: pinned.entrypoint,
        command: pinned.command,
    })
}

/// The exact version of each package `fetches` pins, for `release` on `os`.
fn packages(
    release: &Release,
    os: OsRef,
    fetches: &Fetches,
    indexes: &mut pins::Indexes,
) -> Res<indexmap::IndexMap<isoloom_spec::syntax::PackageName, isoloom_spec::syntax::PackageVersion>> {
    let mut out = indexmap::IndexMap::new();
    for package in &fetches.packages {
        let version = indexes
            .exact(&fetches.repositories, package.as_str(), release.version.as_str())?
            .ok_or_else(|| {
                format!(
                    "no `{package}` {} on {os} in {}: is the release published there?",
                    release.version,
                    fetches.repositories.iter().map(|r| r.url.as_str()).collect::<Vec<_>>().join(", ")
                )
            })?;
        out.insert(package.clone(), version.parse()?);
    }
    Ok(out)
}

/// `lock` written to `path`, dated today.
fn write_lock(path: &Path, name: &str, mut lock: Lock) -> Res<()> {
    lock.refreshed = today()?;
    let header = format!(
        "# Written by `cargo xtask catalogue refresh {name}` (plan/phase4/design.md §4.8), never by hand: the\n# pins of entry.yml's repository keys, official images and vendor packages. `--check` exits 1\n# when upstream has moved under a pin.\n"
    );
    std::fs::write(path, header + &serde_yaml_ng::to_string(&lock)?)?;
    Ok(())
}

/// Today, UTC (`2026-10-16`).
fn today() -> Res<String> {
    Ok(crate::tiers::civil_date(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()))
}

/// The Galaxy collections pinned: each kept at its version and checked, or pinned at Galaxy's
/// highest when new. The drift found, under `--check`.
fn collections(dir: &Path, args: &Args) -> Res<Vec<String>> {
    let path = dir.join("collections.yml");
    let old: Collections = match std::fs::read_to_string(&path) {
        Ok(text) => serde_yaml_ng::from_str(&text)?,
        Err(_) => Collections::default(),
    };
    let mut new = Collections::default();
    let mut changes = Vec::new();
    for (name, licence) in super::galaxy::PINNED {
        let (pinned, change) = collection(old.get(name), name, licence, args.propose)?;
        changes.extend(change);
        new.collections.push(pinned);
    }
    let moved: Vec<&str> = changes.iter().filter(|(m, _)| *m).map(|(_, t)| t.as_str()).collect();
    if !moved.is_empty() && !args.update && !args.check {
        return Err(format!(
            "collections: a pinned archive changed on Galaxy:\n  {}\nrun with `--update` once understood",
            moved.join("\n  ")
        )
        .into());
    }
    if args.check {
        return Ok(changes.into_iter().map(|(_, t)| format!("collections: {t}")).collect());
    }
    if changes.is_empty() {
        println!("catalogue refresh: collections: current");
    } else {
        let header = "# Written by `cargo xtask catalogue refresh` (plan/phase4/design.md §4.7), never by hand: the official\n# Galaxy collections the Ansible renderer may name, pinned. GPL-3.0: referenced, never embedded.\n";
        std::fs::write(&path, header.to_owned() + &serde_yaml_ng::to_string(&new)?)?;
        println!(
            "catalogue refresh: collections: {} change(s) written to catalogue/collections.yml",
            changes.len()
        );
    }
    Ok(Vec::new())
}

/// Collection `name` pinned: at `kept`'s version when it has one, else Galaxy's highest; and
/// what changed (`true` when its archive moved under the pin).
fn collection(kept: Option<&Collection>, name: &str, licence: &str, propose: bool) -> Res<(Collection, Option<(bool, String)>)> {
    let version = match kept {
        Some(c) => c.version.clone(),
        None => super::galaxy::highest(name)?,
    };
    if propose {
        let highest = super::galaxy::highest(name)?;
        if highest != version {
            println!("catalogue refresh: collections: {name} {highest} is out (pinned: {version})");
        }
    }
    let release = super::galaxy::release(name, &version)?;
    let pinned = Collection {
        name: name.to_owned(),
        version,
        url: release.url.parse()?,
        sha256: release.sha256.parse()?,
        licence: licence.to_owned(),
    };
    let change = match kept {
        Some(c) if c.sha256 != pinned.sha256 => Some((true, format!("{name} {}: its archive changed ({} → {})", c.version, c.sha256, pinned.sha256))),
        Some(c) if c == &pinned => None,
        _ => Some((false, format!("{name} {} pinned", pinned.version))),
    };
    Ok((pinned, change))
}
