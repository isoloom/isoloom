//! `cargo xtask verify`: the official validators on generated files, inside the pinned toolchain
//! image (`tools/verify/Dockerfile`), and the failures compared with the baseline
//! (`tools/verify/baseline.json`): a failure the baseline doesn't know fails the run.
//!
//! TREE is a folder of generated files, such as `cargo xtask generate-all` writes. When it holds
//! `.isoloom/` folders, only the files under them are checked (the rest is the projects' own
//! input, which the generated files refer to). Without TREE, every example is generated first,
//! into `$VERIFY_OUT/tree`.
//!
//! Each file goes to its format's validator (`docker compose config`, terraform and tflint,
//! `vagrant validate`, kubeconform and helm, cloud-init, hadolint, ansible-lint, shellcheck,
//! PSScriptAnalyzer); the runner that does this is `tools/verify/runner`, handed to the image
//! with the tree. Written to `$VERIFY_OUT` (default `target/verify`): `results.jsonl`,
//! `new.jsonl` (the failures the baseline doesn't know), `stale.jsonl` (baseline entries no
//! failure matches any more) and `summary.txt`, also printed. `--update-baseline` rewrites the
//! baseline with today's failures instead, keeping the reasons already written.
//!
//! The same on macOS, Linux and Windows: no host path reaches Docker. The tree goes into the
//! container as a tar stream on its standard input and the results come back on its standard
//! output; the only mount is a named volume (`$VERIFY_CACHE`, default `isoloom-verify-cache`),
//! the cache of Terraform providers and Kubernetes schemas, so a run is offline once the first has
//! filled it. `VERIFY_JOBS` and `VERIFY_TERRAFORM_JOBS` go to the runner; `ISOLOOM` to
//! generate-all.
//!
//! Exit status: 0 when every failure is in the baseline, 1 when some aren't, 2 when the run
//! itself failed.

use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use crate::{Res, generate_all, process, repo};

/// The arguments of `cargo xtask verify`.
#[derive(clap::Args)]
pub struct Args {
    /// Rewrite the baseline with today's failures.
    #[arg(long)]
    update_baseline: bool,
    /// A folder of generated files (default: every example, generated now).
    tree: Option<PathBuf>,
}

/// Folder names never sent to the validators: tool state and caches.
const SKIPPED: [&str; 4] = [".terraform", ".vagrant", ".git", "__pycache__"];

/// Runs the validators; the exit status says how it went.
pub fn run(a: &Args) -> ExitCode {
    verify(a).unwrap_or_else(|e| {
        eprintln!("verify: {e}");
        ExitCode::from(2)
    })
}

fn verify(a: &Args) -> Res<ExitCode> {
    let root = repo::root();
    let out = std::env::var_os("VERIFY_OUT").map_or_else(|| root.join("target/verify"), PathBuf::from);
    std::fs::create_dir_all(&out)?;
    let out = repo::physical(&out);
    let tree = if let Some(t) = &a.tree {
        let tree = repo::physical(t);
        if out.starts_with(&tree) {
            return Err(format!("VERIFY_OUT ({}) can't be inside the tree", out.display()).into());
        }
        tree
    } else {
        let tree = out.join("tree");
        generate_all::generate(&generate_all::binary(&root)?, &generate_all::examples(&root)?, &tree)?;
        tree
    };
    if process::quiet(Command::new("docker").arg("info")).is_err() {
        return Err("Docker isn't running".into());
    }
    let image = image(&root)?;
    let status = run_image(&image, &tree, &root, &out, a.update_baseline)?;
    finish(&out, &root, a.update_baseline, status)
}

/// The image, tagged with a checksum (POSIX `cksum`) of what it is built from, so a changed
/// Dockerfile builds a new one and an unchanged one is built once per host.
fn image(root: &Path) -> Res<String> {
    let dir = root.join("tools/verify");
    let mut inputs = std::fs::read(dir.join("Dockerfile"))?;
    inputs.extend(std::fs::read(dir.join("requirements.txt"))?);
    let image = format!("isoloom-verify:{}", cksum(&inputs));
    if process::quiet(Command::new("docker").args(["image", "inspect", &image])).is_ok() {
        return Ok(image);
    }
    eprintln!("verify: building {image} (tools/verify/Dockerfile)");
    // The build context goes as a tar stream too, so no host path reaches Docker.
    let mut context = tar::Builder::new(Vec::new());
    for name in ["Dockerfile", "requirements.txt"] {
        context.append_path_with_name(dir.join(name), name)?;
    }
    let bytes = context.into_inner()?;
    let mut build = Command::new("docker");
    build
        .args(["build", "--platform", "linux/amd64", "-t", &image, "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit());
    let mut child = build.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&bytes)?;
    }
    if !child.wait()?.success() {
        return Err(format!("building {image} failed").into());
    }
    Ok(image)
}

/// The POSIX `cksum` of `data`: a CRC-32 (polynomial 0x04C11DB7, most significant bit first)
/// over the data and then its length.
pub fn cksum(data: &[u8]) -> u32 {
    fn feed(crc: u32, byte: u8) -> u32 {
        (0..8).fold(crc ^ (u32::from(byte) << 24), |c, _| {
            if c & 0x8000_0000 == 0 { c << 1 } else { (c << 1) ^ 0x04C1_1DB7 }
        })
    }
    let mut crc = data.iter().fold(0, |c, b| feed(c, *b));
    let mut len = data.len();
    while len > 0 {
        crc = feed(crc, len.to_le_bytes()[0]);
        len >>= 8;
    }
    !crc
}

/// Adds `dir` to the archive as `name`, without the [`SKIPPED`] folders, symlinks as links.
fn add_tree(tar: &mut tar::Builder<File>, dir: &Path, name: &Path) -> Res<()> {
    tar.append_dir(name, dir)?;
    for entry in std::fs::read_dir(dir)?.flatten() {
        let file_name = entry.file_name();
        if SKIPPED.iter().any(|s| file_name == *s) {
            continue;
        }
        let (path, inside) = (entry.path(), name.join(&file_name));
        if entry.file_type()?.is_dir() {
            add_tree(tar, &path, &inside)?;
        } else {
            tar.append_path_with_name(&path, &inside)?;
        }
    }
    Ok(())
}

/// Runs the image on the tree: the tree and the runner go in as one archive (`/work/tree`,
/// `/work/verify`), the results come back as another. The runner's exit status.
fn run_image(image: &str, tree: &Path, root: &Path, out: &Path, update: bool) -> Res<i32> {
    let work = out.join(".work");
    for stale in ["results.jsonl", "new.jsonl", "stale.jsonl", "summary.txt", "baseline.json"] {
        let _ = std::fs::remove_file(out.join(stale));
    }
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;
    let mut input = tar::Builder::new(File::create(work.join("in.tar"))?);
    input.follow_symlinks(false);
    add_tree(&mut input, tree, Path::new("tree"))?;
    add_tree(&mut input, &root.join("tools/verify"), Path::new("verify"))?;
    input.into_inner()?;
    let id = process::output(Command::new("docker").args(["image", "inspect", "-f", "{{.Id}}", image]))?;
    let cache = std::env::var("VERIFY_CACHE").unwrap_or_else(|_| "isoloom-verify-cache".into());
    let script = "set -u; mkdir -p /work /out; tar -xf - -C /work --no-same-owner --warning=no-unknown-keyword || exit 2; code=0; \
                  python3 /work/verify/runner --tree /work/tree --out /out --baseline /work/verify/baseline.json $1 >&2 || code=$?; \
                  tar -cf - -C /out .; exit $code";
    let mut docker = Command::new("docker");
    docker.args(["run", "--rm", "-i", "--platform", "linux/amd64", "-v", &format!("{cache}:/cache")]);
    docker.args([
        "-e",
        "VERIFY_JOBS",
        "-e",
        "VERIFY_TERRAFORM_JOBS",
        "-e",
        &format!("VERIFY_IMAGE={image} {}", id.trim()),
    ]);
    docker.args([image, "sh", "-c", script, "verify", if update { "--update" } else { "" }]);
    docker.stdin(File::open(work.join("in.tar"))?).stdout(File::create(work.join("out.tar"))?);
    let status = docker.status()?;
    if let Ok(results) = File::open(work.join("out.tar")) {
        let _ = tar::Archive::new(results).unpack(out);
    }
    Ok(status.code().unwrap_or(2))
}

/// Prints the summary, updates the baseline when asked, and turns the runner's status into ours.
fn finish(out: &Path, root: &Path, update: bool, status: i32) -> Res<ExitCode> {
    let summary = std::fs::read_to_string(out.join("summary.txt")).map_err(|_| format!("the run failed (exit status {status}), no results"))?;
    print!("{summary}");
    if update {
        std::fs::copy(out.join("baseline.json"), root.join("tools/verify/baseline.json"))?;
        eprintln!("verify: baseline rewritten (tools/verify/baseline.json)");
    }
    let _ = std::fs::remove_dir_all(out.join(".work"));
    match status {
        0 => Ok(ExitCode::SUCCESS),
        3 => Ok(ExitCode::FAILURE),
        other => Err(format!("the runner failed (exit status {other})").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cksum_is_posix_cksum() {
        // `printf abc | cksum` and `printf '' | cksum`.
        assert_eq!(cksum(b"abc"), 1_219_131_554);
        assert_eq!(cksum(b""), 4_294_967_295);
    }
}
