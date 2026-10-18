//! `cargo xtask tf-test`: the cloud and server modules' logic proven without clouds. The
//! examples the suites name (`tests/terraform/`, see [`suite`]) are generated into
//! `target/tf-test/tree`, and `terraform test` runs against each suite's module, inside the
//! pinned validator image (`tools/verify/Dockerfile`, Terraform 1.7 or later for mocked
//! providers): every provider is mocked (`mock_provider`) and every run plans only, so there are
//! no credentials, no cloud and no provisioner. The assertions read the planned values: each
//! network's routes, the gateways, the firewalls, the published ports and their outputs, the
//! private names, the ready file.
//!
//! The same on macOS, Linux and Windows, as `verify`: the tree goes into the container as a tar
//! stream and the results come back on its standard output; the only mount is `verify`'s cache
//! volume (`$VERIFY_CACHE`, default `isoloom-verify-cache`), whose providers serve `init`
//! without the network once a `verify` (or a first `tf-test`) has filled it. The container may
//! use `ISOLOOM_VERIFY_CPUS` CPUs (default 4) and tests as many modules at once.
//!
//! The report names each failure's example, target, run, assertion and the planned values it
//! read; it is also written to `report.txt` in `$TF_TEST_OUT` (default `target/tf-test`), beside
//! the tree, the container's input and its raw output. A run named `known_bug_<what>` records
//! a bug of a generated module: it must fail (see [`report`]). `--fast` runs the subset a plain
//! `check-all` runs. Exit status: 0 when every run holds, 1 when one fails, 2 when the harness
//! itself failed.

mod report;
mod results;
mod suite;

use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Instant;

use crate::{Res, generate_all, process, repo, verify};

/// The arguments of `cargo xtask tf-test`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Only the fast subset (one rich example on every target the suites cover), as a plain
    /// `check-all` runs it.
    #[arg(long)]
    fast: bool,
    /// Only the suites whose path under tests/terraform contains this text (`cloud-vm/aws`).
    #[arg(long)]
    filter: Option<String>,
}

/// The script the container runs.
const SCRIPT: &str = include_str!("tf_test/run.sh");

/// Runs the suites; the exit status says how it went.
pub(crate) fn run(a: &Args) -> ExitCode {
    tf_test(a).unwrap_or_else(|e| {
        eprintln!("tf-test: {e}");
        ExitCode::from(2)
    })
}

fn tf_test(a: &Args) -> Res<ExitCode> {
    let started = Instant::now();
    let root = repo::root();
    let suites = suite::discover(&root, a.fast, a.filter.as_deref())?;
    if suites.is_empty() {
        return Err(format!("no suite under {} matches", suite::DIR).into());
    }
    let out = std::env::var_os("TF_TEST_OUT").map_or_else(|| repo::target_dir(&root).join("tf-test"), PathBuf::from);
    std::fs::create_dir_all(&out)?;
    let tree = out.join("tree");
    let examples: BTreeSet<&str> = suites.iter().map(|s| s.example.as_str()).collect();
    let specs: Vec<PathBuf> = examples.iter().map(|e| root.join("examples").join(e)).collect();
    generate_all::generate(&generate_all::binary(&root)?, &specs, &tree)?;
    if process::quiet(Command::new("docker").arg("info")).is_err() {
        return Err("Docker isn't running".into());
    }
    let image = verify::image(&root)?;
    let input = pack(&root, &tree, &suites, &out)?;
    let output = container(&image, &input, verify::cpus(std::env::var("ISOLOOM_VERIFY_CPUS").ok().as_deref())?)?;
    std::fs::write(out.join("output.jsonl"), &output)?;
    let named: Vec<(suite::Suite, String)> = suites
        .into_iter()
        .map(|s| {
            let shown = s.file.strip_prefix(&root).map_or_else(|_| s.file.display().to_string(), repo::slashed);
            (s, shown)
        })
        .collect();
    let (text, tally) = report::report(&named, &results::parse(&output));
    std::fs::write(out.join("report.txt"), &text)?;
    print!("{text}");
    println!(
        "tf-test: {} suites, {} runs held, {} failed, {} known bugs ({}s)",
        named.len(),
        tally.passed,
        tally.failed,
        tally.known,
        started.elapsed().as_secs()
    );
    Ok(process::exit_code(tally.failed == 0))
}

/// The container's input, `<out>/in.tar`: the tree (`tree/`), each suite's test file and the
/// shared mocks in its module's `tests/`, the modules' list (`modules`) and the script
/// (`run.sh`).
fn pack(root: &Path, tree: &Path, suites: &[suite::Suite], out: &Path) -> Res<PathBuf> {
    let path = out.join("in.tar");
    let mut tar = tar::Builder::new(File::create(&path)?);
    tar.follow_symlinks(false);
    verify::add_tree(&mut tar, tree, Path::new("tree"))?;
    let mocks = root.join(suite::DIR).join(suite::MOCKS);
    let mut modules = String::new();
    for s in suites {
        let tests = Path::new("tree").join(&s.module).join("tests");
        tar.append_path_with_name(&s.file, tests.join(format!("{}.tftest.hcl", s.example)))?;
        if mocks.is_dir() {
            verify::add_tree(&mut tar, &mocks, &tests.join("mocks"))?;
        }
        modules.push_str(&s.module);
        modules.push('\n');
    }
    for (name, bytes) in [("modules", modules.as_bytes()), ("run.sh", SCRIPT.as_bytes())] {
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::try_from(bytes.len())?);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, name, bytes)?;
    }
    tar.into_inner()?;
    Ok(path)
}

/// Runs the script in the image on `input`, capped at `cpus` CPUs and testing as many modules at
/// once; what it printed. Its progress goes to the terminal.
fn container(image: &str, input: &Path, cpus: u16) -> Res<String> {
    let cache = std::env::var("VERIFY_CACHE").unwrap_or_else(|_| "isoloom-verify-cache".into());
    let script = "mkdir -p /work && tar -xf - -C /work --no-same-owner --warning=no-unknown-keyword || exit 2; exec bash /work/run.sh";
    let mut docker = Command::new("docker");
    docker.args(["run", "--rm", "-i", "--platform", "linux/amd64", "--cpus", &cpus.to_string()]);
    docker.args(["-v", &format!("{cache}:/cache"), "-e", &format!("TF_TEST_JOBS={cpus}")]);
    docker.args([image, "bash", "-c", script]);
    docker.stdin(File::open(input)?).stdout(Stdio::piped()).stderr(Stdio::inherit());
    let done = docker.output()?;
    if !done.status.success() {
        return Err(format!("the container failed ({})", done.status).into());
    }
    Ok(String::from_utf8_lossy(&done.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_reports_every_module_in_the_list_s_order() {
        assert!(SCRIPT.contains("@@module") && SCRIPT.contains("@@status") && SCRIPT.contains("@@log"));
        assert!(SCRIPT.contains("terraform test -json"), "the report reads Terraform's JSON");
        assert!(SCRIPT.contains("-backend=false"), "no state backend is ever configured");
        assert!(SCRIPT.contains("timeout while waiting for plugin to start"), "a slow provider start is retried");
        assert!(
            SCRIPT.contains("for pass in init_one test_one"),
            "no test runs while an init writes in the tree"
        );
    }
}
