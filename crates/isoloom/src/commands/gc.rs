//! `isoloom gc`: Docker leftovers of environments, listed, and removed with `--yes`.

use std::collections::BTreeSet;
use std::process::{ExitCode, Stdio};

use crate::{Res, output};

#[derive(clap::Args)]
pub struct Args {
    /// Remove what is listed.
    #[arg(long)]
    yes: bool,
    /// Also built images that no container uses (they are rebuilt or pulled on the next run).
    #[arg(long)]
    images: bool,
    /// Machine-readable output: one JSON document, or the error as JSON (see docs/json.md).
    #[arg(long)]
    pub json: bool,
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
    let managed = format!("label={}=true", isoloom_targets::MANAGED_LABEL);
    let alive = live_projects();
    let left = Leftovers {
        networks: orphans("network", &managed, &alive),
        volumes: orphans("volume", &managed, &alive),
        images: if a.images { unused_images(&managed) } else { Vec::new() },
    };
    if a.json {
        return json(&left, a.yes);
    }
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
    Ok(remove(&left))
}

/// `gc --json`: what is left over, as one document, and whether removing it (`--yes`) worked.
fn json(left: &Leftovers, yes: bool) -> Res<ExitCode> {
    let named = |list: &[(String, String)]| -> Vec<serde_json::Value> {
        list.iter().map(|(name, env)| serde_json::json!({ "name": name, "environment": env })).collect()
    };
    let failed = if yes && left.count() > 0 { removal_failures(left) } else { 0 };
    let images: Vec<serde_json::Value> = left.images.iter().map(|(id, name)| serde_json::json!({ "id": id, "name": name })).collect();
    output::print_json(&serde_json::json!({
        "networks": named(&left.networks),
        "volumes": named(&left.volumes),
        "images": images,
        "removed": yes && failed == 0,
        "failed": failed,
    }))?;
    Ok(output::exit_code(failed == 0))
}

/// `docker <args>`, its stdout lines (empty when Docker isn't there or fails).
fn docker_lines(args: &[&str]) -> Vec<String> {
    crate::tools::process::command("docker")
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
    let env_label = isoloom_targets::ENVIRONMENT_LABEL;
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
            std::collections::BTreeSet::default()
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

fn remove(left: &Leftovers) -> ExitCode {
    let failed = removal_failures(left);
    if failed > 0 {
        eprintln!("✗ {failed} could not be removed (in use?)");
        return ExitCode::from(1);
    }
    println!("removed");
    ExitCode::SUCCESS
}

/// Removes every leftover; how many could not be removed.
fn removal_failures(left: &Leftovers) -> usize {
    let mut failed = 0;
    let mut rm = |args: &[&str]| {
        if !crate::tools::process::command("docker")
            .args(args)
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        {
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
    failed
}
