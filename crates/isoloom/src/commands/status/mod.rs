//! `isoloom status`: every environment `run` brought up on this host (the registry), with
//! its live state as the target's own tool reports it (`probe`); `--cleanup` tears one down.

mod probe;

use std::process::{Command, ExitCode};

use isoloom_core as core;
use isoloom_core::registry::{self, Entry};

use crate::commands::run::bring_up;
use crate::{Res, output};

#[derive(clap::Args)]
pub struct Args {
    /// Machine-readable output.
    #[arg(long)]
    json: bool,
    /// Tear this environment down (by name) and forget it, from anywhere.
    #[arg(long, value_name = "NAME")]
    cleanup: Option<String>,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    let reg = registry::load()?;
    if let Some(name) = &a.cleanup {
        return cleanup(&reg, name);
    }
    let rows: Vec<(Entry, String)> = reg.environments.iter().map(|e| (e.clone(), probe::probe(e))).collect();
    if a.json {
        let list: Vec<serde_json::Value> = rows
            .iter()
            .map(|(e, state)| {
                serde_json::json!({
                    "name": e.name, "dir": e.dir, "target": e.target.id(), "instance": e.instance, "cloud": e.cloud, "profile": e.profile, "started": e.started, "state": state,
                })
            })
            .collect();
        output::print_json(&serde_json::json!({ "registry": registry::path(), "environments": list }))?;
        return Ok(ExitCode::SUCCESS);
    }
    if rows.is_empty() {
        println!(
            "no environment is up on this host (none recorded in {}); `isoloom run <target>` starts one",
            registry::path().display()
        );
        return Ok(ExitCode::SUCCESS);
    }
    print_table(&rows);
    Ok(ExitCode::SUCCESS)
}

fn print_table(rows: &[(Entry, String)]) {
    let w = |f: fn(&(Entry, String)) -> usize| rows.iter().map(f).max().unwrap_or(0);
    let (wn, wt, ws) = (w(|r| r.0.name.len()).max(4), w(|r| target_label(&r.0).len()).max(6), w(|r| r.1.len()).max(5));
    let head = ["NAME", "TARGET", "STATE", "STARTED", "FOLDER"];
    println!("{:<wn$}  {:<wt$}  {:<ws$}  {:<20}  {}", head[0], head[1], head[2], head[3], head[4]);
    for (e, state) in rows {
        println!(
            "{:<wn$}  {:<wt$}  {:<ws$}  {:<20}  {}",
            e.name,
            target_label(e),
            state,
            e.started,
            e.dir.display()
        );
    }
}

fn target_label(e: &Entry) -> String {
    let mut label = match &e.cloud {
        Some(c) => format!("{} ({c})", e.target.id()),
        None => e.target.id().to_string(),
    };
    if let Some(n) = e.instance {
        label.push_str(&format!(" #{n}"));
    }
    if let Some(p) = &e.profile {
        label.push_str(&format!(" [{p}]"));
    }
    label
}

/// `--cleanup <name>`: the environments by that name torn down, then forgotten.
fn cleanup(reg: &registry::Registry, name: &str) -> Res<ExitCode> {
    let targets: Vec<Entry> = reg.environments.iter().filter(|e| e.name == name || e.dir.ends_with(name)).cloned().collect();
    if targets.is_empty() {
        return Err(format!("no environment named `{name}` in {}", registry::path().display()).into());
    }
    for e in &targets {
        if e.dir.join(core::instance::output_dir(e.instance)).is_dir() {
            tear_down(e)?;
        } else {
            eprintln!("{}: folder gone, removing the entry", e.name);
        }
    }
    // Removed only now, in one locked update: the teardowns take a while, and other runs
    // may have changed the registry meanwhile.
    registry::update(|r| {
        for e in &targets {
            r.remove(&e.dir, e.target, e.instance);
        }
    })?;
    Ok(ExitCode::SUCCESS)
}

fn tear_down(e: &Entry) -> Res<()> {
    let tool = bring_up(&e.dir, e.target, e.cloud.as_deref(), e.instance, true)?;
    eprintln!("Tearing down {} on {} ({})", e.name, e.target.id(), tool.wd.display());
    let ok = Command::new(&tool.program)
        .args(&tool.args)
        .current_dir(&tool.wd)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("✗ {} failed; the entry is removed anyway (check for leftovers by hand)", tool.program);
    }
    Ok(())
}
