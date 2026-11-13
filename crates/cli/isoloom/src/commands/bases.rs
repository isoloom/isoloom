//! `isoloom bases fetch`: vendors the published Dockerfiles of the base images the project's
//! builds start `FROM` that the service matrix doesn't have, so their machines get a VM form
//! offline. Each image resolves to its source (docker-library's `official-images` manifest for an
//! official image, the publisher's repository for the few Isoloom knows), checked out at the
//! pinned commit with `git`, its folder copied under `vendor/bases/`, and listed in
//! `vendor/bases/index.yml`; the fetched Dockerfiles' own bases follow, until every chain ends
//! in the matrix or can't be fetched (said, never guessed).

use std::path::{Path, PathBuf};
use std::process::Command;

use isoloom_targets::dockerfile::sources::{self, INDEX, VENDOR_DIR};
use isoloom_targets::dockerfile::{Origin, Sources, Vendored};

use super::{Console, Project, Result, Verdict, spec};
use crate::error::CliError;

#[derive(Debug, clap::Subcommand)]
pub(crate) enum Bases {
    /// Fetch and vendor the base images the builds need (under vendor/bases/).
    Fetch(Args),
}

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    pub(crate) project: Project,
    #[command(flatten)]
    pub(crate) settings: spec::Settings,
}

/// The most images one fetch follows (a chain of bases is a few deep; a loop stops here).
const MAX_IMAGES: usize = 32;

pub(crate) fn run(b: Bases, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let Bases::Fetch(a) = b;
    let dir = &a.project.dir;
    let (lab, _) = spec::with_settings(profile, dir, &a.settings)?;
    let mut index = spec::bases_index(dir)?;
    let mut sources = Sources::of(&index, |p| std::fs::read_to_string(dir.join(p)).ok());
    let mut todo: Vec<String> = Vec::new();
    for p in lab.dockerfile_paths() {
        if let Ok(text) = std::fs::read_to_string(dir.join(p.as_str())) {
            todo.extend(sources::wanted(&text, &sources));
        }
    }
    let scratch = std::env::temp_dir().join(format!("isoloom-bases-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).map_err(|source| CliError::Write { path: scratch.clone(), source })?;
    let mut manifests = None;
    let mut failed = 0;
    let mut seen = 0;
    while let Some(image) = todo.pop() {
        seen += 1;
        if seen > MAX_IMAGES || sources.get(&image).is_some() {
            continue;
        }
        let origin = match sources::publisher(&image) {
            Some(o) => Some(o),
            None => official(&image, &scratch, &mut manifests),
        };
        let Some(origin) = origin else {
            console.line(format_args!("✗ {image}: no published source Isoloom knows (give its machine `os` and `app.steps`)"))?;
            failed += 1;
            continue;
        };
        match vendor(dir, &scratch, &image, origin) {
            Ok(v) => {
                let path = format!("{}/{}", v.context, v.origin.file);
                console.line(format_args!("✓ {image}: {}@{} {}", v.origin.repo, short(&v.origin.commit), v.origin.file))?;
                index.insert(sources::key(&image), v);
                sources = Sources::of(&index, |p| std::fs::read_to_string(dir.join(p)).ok());
                if let Ok(text) = std::fs::read_to_string(dir.join(&path)) {
                    todo.extend(sources::wanted(&text, &sources));
                }
            }
            Err(why) => {
                console.line(format_args!("✗ {image}: {why}"))?;
                failed += 1;
            }
        }
    }
    std::fs::remove_dir_all(&scratch).unwrap_or_default();
    let text = serde_yaml_ng::to_string(&index).map_err(|e| CliError::Write { path: dir.join(INDEX), source: std::io::Error::other(e) })?;
    write(&dir.join(INDEX), &text)?;
    Ok(if failed == 0 { Verdict::Pass } else { Verdict::Fail })
}

/// An official image's source, from docker-library's manifest of its name (fetched once).
fn official(image: &str, scratch: &Path, manifests: &mut Option<PathBuf>) -> Option<Origin> {
    let (name, tag) = isoloom_targets::dockerfile::matrix::split(image);
    if name.contains('/') {
        return None;
    }
    if manifests.is_none() {
        let to = scratch.join("official-images");
        let ok = git(scratch, &["clone", "--quiet", "--depth", "1", "https://github.com/docker-library/official-images", to.to_str()?]).is_ok();
        *manifests = ok.then_some(to);
    }
    let text = std::fs::read_to_string(manifests.as_ref()?.join("library").join(&name)).ok()?;
    sources::official(&text, &tag)
}

/// `origin` checked out at its commit, its folder copied under `vendor/bases/<image>/`.
fn vendor(dir: &Path, scratch: &Path, image: &str, mut origin: Origin) -> std::result::Result<Vendored, String> {
    let folder = sources::key(image).replace(['/', ':'], "-");
    let checkout = scratch.join(&folder);
    let path = checkout.to_str().ok_or("a path that isn't UTF-8")?;
    git(scratch, &["init", "--quiet", path])?;
    git(&checkout, &["fetch", "--quiet", "--depth", "1", &origin.repo, &origin.commit])?;
    git(&checkout, &["checkout", "--quiet", "FETCH_HEAD"])?;
    git(&checkout, &["rev-parse", "FETCH_HEAD"])?.trim().clone_into(&mut origin.commit);
    let context = format!("{VENDOR_DIR}/{folder}");
    let to = dir.join(&context);
    std::fs::remove_dir_all(&to).unwrap_or_default();
    copy_tree(&checkout.join(&origin.directory), &to).map_err(|e| format!("copying its sources: {e}"))?;
    Ok(Vendored { origin, context })
}

/// `git args` run in `cwd`: its output, or why it failed.
fn git(cwd: &Path, args: &[&str]) -> std::result::Result<String, String> {
    let out = Command::new("git").args(args).current_dir(cwd).output().map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!("git {}: {}", args.first().unwrap_or(&""), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// `from`'s files under `to`, `.git` left out.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_name() == ".git" {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn write(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| CliError::Write { path: parent.to_owned(), source })?;
    }
    std::fs::write(path, text).map_err(|source| CliError::Write { path: path.to_owned(), source })
}

/// A commit's first 12 characters.
fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}
