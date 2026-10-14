//! `isoloom gc`: Docker leftovers of environments, listed, and removed with `--yes`.

use std::collections::BTreeSet;
use std::process::{Command, ExitCode, Stdio};

use isoloom_core as core;

use crate::Res;

#[derive(clap::Args)]
pub struct Args {
    /// Remove what is listed.
    #[arg(long)]
    yes: bool,
    /// Also built images that no container uses (they are rebuilt or pulled on the next run).
    #[arg(long)]
    images: bool,
}

/// What can go: (name or id, what to show) for each kind.
struct Leftovers {
    networks: Vec<(String, String)>,
    volumes: Vec<(String, String)>,
    images: Vec<(String, String)>,
}

impl Leftovers {
    fn count(&self) -> usize {
        self.networks.len() + self.volumes.len() + self.images.len()
    }
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let managed = format!("label={}=true", core::generate::MANAGED_LABEL);
    let alive = live_projects();
    let left = Leftovers {
        networks: orphans("network", &managed, &alive),
        volumes: orphans("volume", &managed, &alive),
        images: if a.images { unused_images(&managed) } else { Vec::new() },
    };
    if left.count() == 0 {
        println!("nothing to collect");
        return Ok(ExitCode::SUCCESS);
    }
    for (kind, list) in [("network", &left.networks), ("volume", &left.volumes)] {
        for (name, env) in list {
            println!("{kind} {name} ({env})");
        }
    }
    for (_, what) in &left.images {
        println!("image {what}");
    }
    if !a.yes {
        println!(
            "\n{} left over; `isoloom gc --yes{}` removes them",
            left.count(),
            if a.images { " --images" } else { "" }
        );
        return Ok(ExitCode::SUCCESS);
    }
    remove(&left)
}

/// `docker <args>`, its stdout lines (empty when Docker isn't there or fails).
fn docker_lines(args: &[&str]) -> Vec<String> {
    Command::new("docker")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Compose projects that still have a container, in any state: never collected (a parked
/// environment keeps its volumes, with the user's progress in them).
fn live_projects() -> BTreeSet<String> {
    docker_lines(&["ps", "-a", "--format", "{{.Label \"com.docker.compose.project\"}}"])
        .into_iter()
        .collect()
}

/// Isoloom's networks or volumes whose Compose project has no container left, with the
/// environment each belongs to.
fn orphans(kind: &str, managed: &str, alive: &BTreeSet<String>) -> Vec<(String, String)> {
    let env_label = core::generate::ENVIRONMENT_LABEL;
    docker_lines(&[
        kind,
        "ls",
        "--filter",
        managed,
        "--format",
        &format!("{{{{.Name}}}}\t{{{{.Label \"com.docker.compose.project\"}}}}\t{{{{.Label \"{env_label}\"}}}}"),
    ])
    .into_iter()
    .filter_map(|l| {
        let mut p = l.split('\t');
        let (name, project, env) = (p.next()?.to_string(), p.next().unwrap_or("").to_string(), p.next().unwrap_or("").to_string());
        (!alive.contains(&project)).then_some((name, if env.is_empty() { project } else { env }))
    })
    .collect()
}

/// Isoloom's built images no container uses, as (id, name).
fn unused_images(managed: &str) -> Vec<(String, String)> {
    let used: BTreeSet<String> = {
        let ids = docker_lines(&["ps", "-aq"]);
        let mut args = vec!["inspect", "-f", "{{.Image}}"];
        args.extend(ids.iter().map(String::as_str));
        if ids.is_empty() {
            Default::default()
        } else {
            docker_lines(&args).into_iter().collect()
        }
    };
    // `docker images` has no `.Label` field: the name says which environment built it.
    docker_lines(&["images", "--no-trunc", "--filter", managed, "--format", "{{.ID}}\t{{.Repository}}:{{.Tag}}"])
        .into_iter()
        .filter_map(|l| {
            let (id, name) = l.split_once('\t')?;
            (!used.contains(id)).then(|| (id.to_string(), name.to_string()))
        })
        .collect()
}

fn remove(left: &Leftovers) -> Res<ExitCode> {
    let mut failed = 0;
    let mut rm = |args: &[&str]| {
        if !Command::new("docker").args(args).stdout(Stdio::null()).status().is_ok_and(|s| s.success()) {
            failed += 1;
        }
    };
    for (n, _) in &left.networks {
        rm(&["network", "rm", n]);
    }
    for (n, _) in &left.volumes {
        rm(&["volume", "rm", n]);
    }
    for (id, _) in &left.images {
        rm(&["image", "rm", id]);
    }
    if failed > 0 {
        eprintln!("✗ {failed} could not be removed (in use?)");
        return Ok(ExitCode::from(1));
    }
    println!("removed");
    Ok(ExitCode::SUCCESS)
}
