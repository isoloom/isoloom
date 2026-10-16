//! `cargo xtask conformance`: the configuration renderers' resource conformance (design
//! `plan/phase4/design.md` §6.3). Each step kind, rendered by each renderer, is applied twice in
//! each Linux family's pinned official image (Debian 12, Ubuntu 24.04, Rocky 9, Alpine 3.24):
//!
//! - **shell:** the VM's set-up steps that run the recipe (the shell library and its calls),
//!   applied in a fresh container, then again;
//! - **dockerfile:** the image the container form builds (its Dockerfile `RUN`s the library's
//!   calls), then the VM's script applied in it.
//! - **cloud-init:** real cloud-init (installed in the family's image, seeded through `NoCloud`)
//!   writes the recipe's script from a `write_files` entry (gzip then base64) and runs it from
//!   `runcmd` (the first apply), then the VM's shell script is applied on top (the second apply)
//!   and must change nothing. That proves the cloud-init carrier leaves what the shell leaves.
//!
//! After the first apply a probe checks each kind's state (the repository read by the package
//! manager, the package, the group, the user and its hashed password, each file state, the
//! copies, the filled template, both guards); the second apply must change nothing (no
//! `isoloom: changed` line), and the state must hold. A `firewall` step has no state in a
//! container (no firewall runs): its second apply's silence is its proof. A `service` needs an
//! init system: the VM runs prove it (`catalogue check`).
//!
//! Containers run with `docker run --cpus` (`ISOLOOM_VERIFY_CPUS`, default 4), builds with
//! `docker build --resource cpu-quota`. The project and the scripts go in with `docker cp`, so
//! no host path is mounted and it runs the same on macOS, Linux and Windows. It needs the network
//! (packages, nginx.org's repositories): `check-all --deep` runs it. Logs go to
//! `target/conformance/<family>-<renderer>.log`. Exit status: 0 when every kind holds, 1 when one
//! doesn't, 2 when the harness itself failed.

mod container;
mod lab;

use std::path::Path;
use std::process::ExitCode;

use crate::{Res, process, repo, verify};

/// The arguments of `cargo xtask conformance`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// The families to run (`debian-12`, `ubuntu-24.04`, `rocky-9`, `alpine-3.24`; default:
    /// every one).
    #[arg(long = "family", value_name = "FAMILY")]
    families: Vec<String>,
    /// The renderers to run (`shell`, `dockerfile`; default: both).
    #[arg(long = "renderer", value_name = "RENDERER")]
    renderers: Vec<String>,
}

/// What renders the recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Renderer {
    /// The shell library, as a VM's set-up runs it.
    Shell,
    /// The Dockerfile the container form builds.
    Dockerfile,
    /// The cloud-init carrier: real cloud-init writes and runs the recipe, then the shell library
    /// runs on top and must change nothing (parity with the shell renderer).
    CloudInit,
}

impl Renderer {
    /// Every renderer, in the order the table and `--renderer` default lists them.
    const ALL: [Renderer; 3] = [Renderer::Shell, Renderer::Dockerfile, Renderer::CloudInit];

    /// Its name, as `--renderer` and the table write it.
    fn id(self) -> &'static str {
        match self {
            Renderer::Shell => "shell",
            Renderer::Dockerfile => "dockerfile",
            Renderer::CloudInit => "cloud-init",
        }
    }
}

/// What one run found: each kind's state, and whether the second apply changed anything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Outcome {
    /// Each kind's failure, when it failed (`None` holds).
    kinds: Vec<(&'static str, Option<String>)>,
    /// What the second apply changed, or why an apply failed.
    again: Vec<String>,
}

impl Outcome {
    /// Whether everything held.
    fn passed(&self) -> bool {
        self.again.is_empty() && self.kinds.iter().all(|(_, f)| f.is_none())
    }
}

/// Runs the conformance tests; the exit status says how they went.
pub(crate) fn run(a: &Args) -> ExitCode {
    conformance(a).unwrap_or_else(|e| {
        eprintln!("conformance: {e}");
        ExitCode::from(2)
    })
}

fn conformance(a: &Args) -> Res<ExitCode> {
    let root = repo::root();
    let families: Vec<lab::Family> = lab::FAMILIES
        .into_iter()
        .filter(|f| a.families.is_empty() || a.families.iter().any(|w| w == f.id))
        .collect();
    let renderers: Vec<Renderer> = Renderer::ALL
        .into_iter()
        .filter(|r| a.renderers.is_empty() || a.renderers.iter().any(|w| w == r.id()))
        .collect();
    if families.is_empty() || renderers.is_empty() {
        return Err("no family or renderer matches".into());
    }
    if process::quiet(std::process::Command::new("docker").arg("info")).is_err() {
        return Err("Docker isn't running".into());
    }
    let cpus = verify::cpus(std::env::var("ISOLOOM_VERIFY_CPUS").ok().as_deref())?;
    let out = repo::target_dir(&root).join("conformance");
    let mut rows = Vec::new();
    for family in families {
        for renderer in &renderers {
            eprintln!("conformance: {} with the {} renderer", family.id, renderer.id());
            let outcome = one(family, *renderer, &out, cpus)?;
            rows.push((family.id, *renderer, outcome));
        }
    }
    let report = table(&rows);
    print!("{report}");
    std::fs::write(out.join("report.txt"), &report)?;
    Ok(process::exit_code(rows.iter().all(|(_, _, o)| o.passed())))
}

/// One family with one renderer: its folder prepared, the container run, its lines read.
fn one(family: lab::Family, renderer: Renderer, out: &Path, cpus: u16) -> Res<Outcome> {
    let dir = out.join(format!("{}-{}", family.id, renderer.id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    let project = dir.join("project");
    lab::project(&project)?;
    let test = dir.join("test");
    lab::write(&test, "apply.sh", &lab::vm_script(family)?)?;
    lab::write(&test, "probe.sh", container::PROBE)?;
    let lines = match renderer {
        Renderer::Shell => {
            lab::write(&test, "run.sh", container::RUN)?;
            container::fresh(family, &dir, cpus)?
        }
        Renderer::Dockerfile => {
            lab::write(&test, "run.sh", container::RUN)?;
            for f in lab::container_files(family)? {
                lab::write(&project, &f.path, &f.contents)?;
            }
            container::built(family, &dir, cpus)?
        }
        // Real cloud-init runs `run.sh`: it installs cloud-init, writes and runs the recipe from
        // `user-data`, then the shell `apply.sh` on top must change nothing.
        Renderer::CloudInit => {
            lab::write(&test, "run.sh", container::RUN_CLOUD_INIT)?;
            lab::write(&test, "user-data", &lab::cloud_config(family)?)?;
            container::fresh(family, &dir, cpus)?
        }
    };
    std::fs::write(out.join(format!("{}-{}.log", family.id, renderer.id())), &lines)?;
    Ok(read(&lines))
}

/// The outcome the container's lines say.
fn read(lines: &str) -> Outcome {
    let state = |prefix: &str, kind: &str| -> Option<String> {
        let ok = format!("{prefix} ok {kind}");
        let failed = format!("{prefix} FAIL {kind}:");
        if lines.lines().any(|l| l == ok) {
            return None;
        }
        Some(
            lines
                .lines()
                .find_map(|l| l.strip_prefix(&failed))
                .map_or_else(|| "not checked".to_owned(), |why| why.trim().to_owned()),
        )
    };
    let mut again: Vec<String> = lines
        .lines()
        .filter_map(|l| l.strip_prefix("SECOND ").map(|c| format!("the second apply: {c}")))
        .collect();
    again.extend(lines.lines().filter(|l| l.starts_with("APPLY-FAILED")).map(ToOwned::to_owned));
    if !lines.lines().any(|l| l == "END") && again.is_empty() {
        again.push("the run didn't finish".to_owned());
    }
    let kinds = lab::KINDS
        .iter()
        .map(|kind| {
            // A firewall has no state in a container: its proof is the applies' silence.
            let failed = if *kind == "firewall" {
                None
            } else {
                state("FIRST", kind).or_else(|| state("AGAIN", kind))
            };
            (*kind, failed)
        })
        .collect();
    Outcome { kinds, again }
}

/// The results as a table (a row per family and renderer, a column per kind), then each failure.
fn table(rows: &[(&str, Renderer, Outcome)]) -> String {
    let kinds: Vec<String> = lab::KINDS.iter().map(|k| format!(" {k:<10}")).collect();
    let mut lines = vec![format!("{:<14} {:<11}{} again", "family", "renderer", kinds.concat())];
    let mut failures = Vec::new();
    for (family, renderer, o) in rows {
        let cells: Vec<String> = o
            .kinds
            .iter()
            .map(|(_, failed)| format!(" {:<10}", if failed.is_some() { "FAIL" } else { "ok" }))
            .collect();
        let again = if o.again.is_empty() { "ok" } else { "FAIL" };
        lines.push(format!("{family:<14} {:<11}{} {again}", renderer.id(), cells.concat()));
        for (kind, failed) in &o.kinds {
            if let Some(why) = failed {
                failures.push(format!("  {family} {} {kind}: {why}", renderer.id()));
            }
        }
        failures.extend(o.again.iter().map(|why| format!("  {family} {}: {why}", renderer.id())));
    }
    lines.extend(failures);
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_passes_when_every_kind_holds_and_the_second_apply_is_silent() {
        let lines: String = lab::KINDS
            .iter()
            .flat_map(|k| [format!("FIRST ok {k}\n"), format!("AGAIN ok {k}\n")])
            .chain(["END\n".to_owned()])
            .collect();
        assert!(read(&lines).passed());
        let noisy = lines.replace("END\n", "SECOND isoloom: changed /srv/conformance/one.conf\nEND\n");
        assert_eq!(read(&noisy).again, ["the second apply: isoloom: changed /srv/conformance/one.conf"]);
        let failed = lines.replace("FIRST ok user\n", "FIRST FAIL user: no such user\n");
        let o = read(&failed);
        assert!(!o.passed());
        assert!(o.kinds.contains(&("user", Some("no such user".to_owned()))), "{o:?}");
        assert!(table(&[("debian-12", Renderer::Shell, o)]).contains("debian-12 shell user: no such user"));
    }

    #[test]
    fn a_run_that_stopped_early_fails() {
        assert_eq!(read("FIRST ok packages\n").again, ["the run didn't finish"]);
    }
}
