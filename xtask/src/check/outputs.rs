//! The checks of what the command line writes: every example validates, generation is
//! deterministic, and the generated Terraform is valid for its providers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use crate::{Res, generate_all, process, repo};

/// How many folders `terraform validate` checks at once, once the provider cache is filled.
const VALIDATORS: usize = 4;

/// How many times `terraform init` is tried: the registry resets a connection now and then.
const INIT_TRIES: u32 = 3;

/// Every example validates (its files skipped) and lists its targets.
pub(super) fn examples(root: &Path) -> Res<bool> {
    let isoloom = generate_all::binary(root)?;
    let mut ok = true;
    for e in generate_all::examples(root)? {
        ok &= process::passed(Command::new(&isoloom).arg("validate").arg(&e).arg("--skip-files"))?;
        ok &= process::passed(Command::new(&isoloom).arg("targets").arg(&e).stdout(Stdio::null()))?;
    }
    Ok(ok)
}

/// Every example generated twice in its own folder, as a user runs it, gives the same files.
pub(super) fn determinism(root: &Path) -> Res<bool> {
    let isoloom = generate_all::binary(root)?;
    let mut same = true;
    for e in generate_all::examples(root)? {
        let first = generated(&isoloom, &e)?;
        let second = generated(&isoloom, &e)?;
        for path in differences(&first, &second) {
            eprintln!("{}: {} differs between two runs", e.display(), path.display());
            same = false;
        }
    }
    Ok(same)
}

/// The files `isoloom generate` writes into `example/.isoloom`, from a clean folder: each path
/// (relative to it) with its bytes.
fn generated(isoloom: &Path, example: &Path) -> Res<BTreeMap<PathBuf, Vec<u8>>> {
    let out = example.join(".isoloom");
    repo::remove(&out)?;
    if !process::passed(Command::new(isoloom).arg("generate").arg(example).stdout(Stdio::null()))? {
        return Err(format!("{} failed to generate", example.display()).into());
    }
    let mut files = BTreeMap::new();
    read_tree(&out, &out, &mut files)?;
    Ok(files)
}

fn read_tree(base: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) -> Res<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            read_tree(base, &path, files)?;
        } else {
            let relative = path.strip_prefix(base)?.to_path_buf();
            files.insert(relative, std::fs::read(&path)?);
        }
    }
    Ok(())
}

/// The paths only one of two trees has, or that the two hold different bytes at.
pub(super) fn differences<'a>(a: &'a BTreeMap<PathBuf, Vec<u8>>, b: &'a BTreeMap<PathBuf, Vec<u8>>) -> Vec<&'a Path> {
    let mut out: Vec<&Path> = a.iter().filter(|(p, bytes)| b.get(*p) != Some(bytes)).map(|(p, _)| p.as_path()).collect();
    out.extend(b.keys().filter(|p| !a.contains_key(*p)).map(PathBuf::as_path));
    out
}

/// Every generated Terraform module (`proxmox`, `esxi`, `cloud-docker/*`, `cloud-vm/*`) passes
/// `terraform validate`, its providers from the plugin cache (`TF_PLUGIN_CACHE_DIR`, by default
/// `terraform-plugin-cache` in the build folder).
pub(super) fn terraform(root: &Path) -> Res<bool> {
    let isoloom = generate_all::binary(root)?;
    let mut modules = Vec::new();
    for e in generate_all::examples(root)? {
        if !process::passed(Command::new(&isoloom).arg("generate").arg(&e).stdout(Stdio::null()))? {
            return Err(format!("{} failed to generate", e.display()).into());
        }
        modules.extend(modules_of(&e.join(".isoloom"))?);
    }
    let cache = std::env::var_os("TF_PLUGIN_CACHE_DIR").map_or_else(|| repo::target_dir(root).join("terraform-plugin-cache"), PathBuf::from);
    std::fs::create_dir_all(&cache)?;
    let (first, rest) = phases(&modules);
    // The cache isn't safe for concurrent writes: one module of each target fills it in turn,
    // then the others read it, a few at a time.
    let mut valid = true;
    for m in first {
        valid &= validate(m, &cache);
    }
    let queue = Mutex::new(rest.into_iter());
    let results = Mutex::new(true);
    std::thread::scope(|s| {
        for _ in 0..VALIDATORS {
            s.spawn(|| {
                while let Some(m) = queue.lock().ok().and_then(|mut q| q.next()) {
                    let ok = validate(m, &cache);
                    if let Ok(mut all) = results.lock() {
                        *all &= ok;
                    }
                }
            });
        }
    });
    Ok(valid && results.into_inner().unwrap_or(false))
}

/// The Terraform modules of a generated tree, sorted.
fn modules_of(out: &Path) -> Res<Vec<PathBuf>> {
    let mut found = Vec::new();
    for single in ["proxmox", "esxi"] {
        if out.join(single).is_dir() {
            found.push(out.join(single));
        }
    }
    for family in ["cloud-docker", "cloud-vm"] {
        let dir = out.join(family);
        if dir.is_dir() {
            found.extend(std::fs::read_dir(&dir)?.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
        }
    }
    found.sort();
    Ok(found)
}

/// The modules split in two: the first of each target (its path from `.isoloom/` on), then
/// every other one.
pub(super) fn phases(modules: &[PathBuf]) -> (Vec<&Path>, Vec<&Path>) {
    let mut seen = Vec::new();
    let (mut first, mut rest) = (Vec::new(), Vec::new());
    for m in modules {
        let kind = target_of(m);
        if seen.contains(&kind) {
            rest.push(m.as_path());
        } else {
            seen.push(kind);
            first.push(m.as_path());
        }
    }
    (first, rest)
}

/// A module's target: its path after `.isoloom/` (`cloud-vm/aws`).
pub(super) fn target_of(module: &Path) -> String {
    let text = repo::slashed(module);
    text.rsplit_once(".isoloom/").map_or(text.clone(), |(_, kind)| kind.to_owned())
}

/// `terraform init`, retried, then `terraform validate` in one module; whether it's valid. What
/// terraform said is printed when it isn't.
fn validate(module: &Path, cache: &Path) -> bool {
    let run = |args: &[&str]| {
        Command::new("terraform")
            .arg(format!("-chdir={}", module.display()))
            .args(args)
            .env("TF_PLUGIN_CACHE_DIR", cache)
            .output()
    };
    let mut said = String::new();
    for attempt in 1..=INIT_TRIES {
        match run(&["init", "-backend=false", "-input=false", "-no-color"]) {
            Ok(out) if out.status.success() => {
                said.clear();
                break;
            }
            Ok(out) => said = String::from_utf8_lossy(&out.stderr).into_owned(),
            Err(e) => said = format!("terraform couldn't start: {e} (is terraform installed?)"),
        }
        if attempt < INIT_TRIES {
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    }
    if !said.is_empty() {
        println!("== {}: init failed\n{said}", module.display());
        return false;
    }
    match run(&["validate", "-no-color"]) {
        Ok(out) if out.status.success() => {
            println!("== {}: valid", module.display());
            true
        }
        Ok(out) => {
            println!(
                "== {}: INVALID\n{}{}",
                module.display(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            false
        }
        Err(e) => {
            println!("== {}: terraform couldn't start: {e}", module.display());
            false
        }
    }
}
