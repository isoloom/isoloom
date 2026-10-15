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
//! `PSScriptAnalyzer`); the runner that does this is `tools/verify/runner`, handed to the image
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
//! The container may use `ISOLOOM_VERIFY_CPUS` CPUs (`docker run --cpus`, default 4), so a run
//! leaves a shared machine the rest, and its runner checks as many files at once
//! (`VERIFY_JOBS`, when unset).
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
pub(crate) struct Args {
    /// Rewrite the baseline with today's failures.
    #[arg(long)]
    update_baseline: bool,
    /// A folder of generated files (default: every example, generated now).
    tree: Option<PathBuf>,
}

/// Folder names never sent to the validators: tool state and caches.
const SKIPPED: [&str; 4] = [".terraform", ".vagrant", ".git", "__pycache__"];

/// Runs the validators; the exit status says how it went.
pub(crate) fn run(a: &Args) -> ExitCode {
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
pub(crate) fn cksum(data: &[u8]) -> u32 {
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
        repo::remove(&out.join(stale))?;
    }
    repo::remove(&work)?;
    std::fs::create_dir_all(&work)?;
    let mut input = tar::Builder::new(File::create(work.join("in.tar"))?);
    input.follow_symlinks(false);
    add_tree(&mut input, tree, Path::new("tree"))?;
    add_tree(&mut input, &root.join("tools/verify"), Path::new("verify"))?;
    input.into_inner()?;
    let id = process::output(Command::new("docker").args(["image", "inspect", "-f", "{{.Id}}", image]))?;
    let cache = std::env::var("VERIFY_CACHE").unwrap_or_else(|_| "isoloom-verify-cache".into());
    let jobs = std::env::var("VERIFY_JOBS").ok();
    let container = Container {
        image,
        id: id.trim(),
        cache: &cache,
        cpus: cpus(std::env::var("ISOLOOM_VERIFY_CPUS").ok().as_deref())?,
        jobs: jobs.as_deref(),
    };
    let mut docker = Command::new("docker");
    docker.args(run_args(&container, update));
    docker.stdin(File::open(work.join("in.tar"))?).stdout(File::create(work.join("out.tar"))?);
    let status = docker.status()?;
    // What the runner wrote comes back whole or not at all: no summary is the failure `finish`
    // reports.
    if let Ok(results) = File::open(work.join("out.tar")) {
        tar::Archive::new(results).unpack(out).ok();
    }
    Ok(status.code().unwrap_or(2))
}

/// The CPUs the container may use when `ISOLOOM_VERIFY_CPUS` doesn't say: enough for the
/// validators to run side by side, while a shared machine keeps the rest.
const DEFAULT_CPUS: u16 = 4;

/// The CPUs the container may use: `setting` (`ISOLOOM_VERIFY_CPUS`), a whole number from 1, or
/// [`DEFAULT_CPUS`] when it is unset or blank.
fn cpus(setting: Option<&str>) -> Res<u16> {
    match setting.map(str::trim) {
        None | Some("") => Ok(DEFAULT_CPUS),
        Some(s) => s
            .parse::<u16>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| format!("ISOLOOM_VERIFY_CPUS is `{s}`: give a whole number of CPUs, from 1").into()),
    }
}

/// How the validators' container runs.
struct Container<'a> {
    /// The image, by its tag.
    image: &'a str,
    /// The image's id, which the baseline records.
    id: &'a str,
    /// The named volume holding the tools' caches.
    cache: &'a str,
    /// The CPUs it may use (`docker run --cpus`).
    cpus: u16,
    /// The runner's validators at once: `VERIFY_JOBS` as set, else one per CPU.
    jobs: Option<&'a str>,
}

/// `docker run`'s arguments: the container capped at its CPUs, its runner told as many jobs
/// (unless `VERIFY_JOBS` says otherwise: the container sees every CPU of the machine), the tree
/// on its standard input and the results on its standard output.
fn run_args(c: &Container, update: bool) -> Vec<String> {
    let script = "set -u; mkdir -p /work /out; tar -xf - -C /work --no-same-owner --warning=no-unknown-keyword || exit 2; code=0; \
                  python3 /work/verify/runner --tree /work/tree --out /out --baseline /work/verify/baseline.json $1 >&2 || code=$?; \
                  tar -cf - -C /out .; exit $code";
    let jobs = c.jobs.map_or_else(|| c.cpus.to_string(), ToOwned::to_owned);
    let mut args: Vec<String> = ["run", "--rm", "-i", "--platform", "linux/amd64", "--cpus"].map(String::from).into();
    args.push(c.cpus.to_string());
    args.extend(["-v".to_owned(), format!("{}:/cache", c.cache)]);
    for env in [
        format!("VERIFY_JOBS={jobs}"),
        "VERIFY_TERRAFORM_JOBS".to_owned(),
        format!("VERIFY_IMAGE={} {}", c.image, c.id),
    ] {
        args.extend(["-e".to_owned(), env]);
    }
    args.extend([c.image, "sh", "-c", script, "verify", if update { "--update" } else { "" }].map(String::from));
    args
}

/// Prints the summary, updates the baseline when asked, and turns the runner's status into ours.
fn finish(out: &Path, root: &Path, update: bool, status: i32) -> Res<ExitCode> {
    let summary = std::fs::read_to_string(out.join("summary.txt")).map_err(|_| format!("the run failed (exit status {status}), no results"))?;
    print!("{summary}");
    if update {
        std::fs::copy(out.join("baseline.json"), root.join("tools/verify/baseline.json"))?;
        eprintln!("verify: baseline rewritten (tools/verify/baseline.json)");
    }
    repo::remove(&out.join(".work"))?;
    match status {
        0 => Ok(ExitCode::SUCCESS),
        3 => Ok(ExitCode::FAILURE),
        other => Err(format!("the runner failed (exit status {other})").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(jobs: Option<&str>) -> Vec<String> {
        let c = Container {
            image: "isoloom-verify:1",
            id: "sha256:ab",
            cache: "the-cache",
            cpus: 4,
            jobs,
        };
        run_args(&c, false)
    }

    /// Whether `args` holds `flag` followed by `value`.
    fn has(args: &[String], flag: &str, value: &str) -> bool {
        args.windows(2).any(|w| w[0] == flag && w[1] == value)
    }

    #[test]
    fn the_container_is_capped_at_its_cpus_and_runs_as_many_jobs() {
        let args = container(None);
        assert!(has(&args, "--cpus", "4"), "{args:?}");
        assert!(has(&args, "-e", "VERIFY_JOBS=4"), "{args:?}");
        assert!(has(&args, "-e", "VERIFY_TERRAFORM_JOBS"), "passed through: {args:?}");
        assert!(has(&args, "-v", "the-cache:/cache"), "{args:?}");
        assert!(has(&args, "-e", "VERIFY_IMAGE=isoloom-verify:1 sha256:ab"), "{args:?}");
        let image = args.iter().position(|a| a == "isoloom-verify:1").unwrap();
        assert!(args.iter().position(|a| a == "--cpus").unwrap() < image, "an option of docker run");
        assert_eq!(args.last().map(String::as_str), Some(""), "no --update");
    }

    #[test]
    fn verify_jobs_as_set_wins_over_the_cpus() {
        let args = container(Some("2"));
        assert!(has(&args, "-e", "VERIFY_JOBS=2") && has(&args, "--cpus", "4"), "{args:?}");
    }

    #[test]
    fn the_cpus_are_a_whole_number_from_one_four_by_default() {
        assert_eq!(cpus(None).unwrap(), 4);
        assert_eq!(cpus(Some(" ")).unwrap(), 4);
        assert_eq!(cpus(Some("8")).unwrap(), 8);
        for wrong in ["0", "1.5", "-2", "all"] {
            let e = cpus(Some(wrong)).unwrap_err().to_string();
            assert!(e.contains(&format!("`{wrong}`")), "{e}");
        }
    }

    #[test]
    fn cksum_is_posix_cksum() {
        // `printf abc | cksum` and `printf '' | cksum`.
        assert_eq!(cksum(b"abc"), 1_219_131_554);
        assert_eq!(cksum(b""), 4_294_967_295);
    }
}
