//! `isoloom`: describe an environment once, run it anywhere.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use isoloom_core as core;

#[derive(Parser)]
#[command(
    name = "isoloom",
    version,
    about = "Describe an environment once (machines, networks, services), run it anywhere: Docker, local VMs, Proxmox, cloud."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check the spec for mistakes (and that every path it mentions exists).
    Validate {
        /// The project folder (holding isoloom.yml).
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Don't check that files mentioned by the spec exist.
        #[arg(long)]
        skip_files: bool,
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
    },
    /// List the targets the spec can run on, and why the others can't.
    Targets {
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Add up the machines, CPUs, memory and disk the spec needs.
    Resources {
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Write each target's files under .isoloom/ (all possible targets, or one with --target).
    Generate {
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Only this target (docker, vagrant).
        #[arg(long)]
        target: Option<String>,
    },
    /// Fail when the generated files under .isoloom/ don't match the spec (for CI).
    Check {
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Show what each output (Compose, Vagrant, Terraform, Ludus) does with every spec field.
    Coverage {
        /// The full table as Markdown, with what each output does per field.
        #[arg(long)]
        markdown: bool,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, Box<dyn std::error::Error>> {
    match cli.command {
        Command::Validate { dir, skip_files, json } => {
            let spec = core::load(&dir)?;
            let mut problems = core::validate(&spec);
            if !skip_files {
                problems.extend(core::validate_files(&spec, &dir));
            }
            if json {
                let list: Vec<_> = problems.iter().map(|p| serde_json::json!({ "at": p.at, "message": p.message })).collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({ "ok": problems.is_empty(), "problems": list }))?
                );
            } else if problems.is_empty() {
                println!("✓ {} is valid", spec.name);
            } else {
                for p in &problems {
                    println!("✗ {p}");
                }
                println!("{} problem{}", problems.len(), if problems.len() == 1 { "" } else { "s" });
            }
            Ok(if problems.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
        }
        Command::Targets { dir, json } => {
            let spec = core::load(&dir)?;
            let effective = core::effective(&spec);
            if json {
                let ids: Vec<&str> = effective.iter().map(|t| t.id()).collect();
                println!("{}", serde_json::to_string_pretty(&ids)?);
                return Ok(ExitCode::SUCCESS);
            }
            for t in core::Target::ALL {
                if effective.contains(&t) {
                    println!("✓ {}", t.id());
                } else {
                    let lacking = core::targets::missing(&spec, t.needs());
                    let why = if lacking.is_empty() {
                        "left out by `targets:`".to_string()
                    } else {
                        format!("needs `{}:` on {}", t.needs().key(), lacking.join(", "))
                    };
                    println!("✗ {} ({why})", t.id());
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Resources { dir, json } => {
            let spec = core::load(&dir)?;
            let t = core::totals(&spec);
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({ "machines": t.machines, "cpus": t.cpus, "memoryMb": t.memory_mb, "diskGb": t.disk_gb }))?
                );
            } else {
                println!(
                    "{} machines · {} CPUs · {:.1} GB memory · {} GB disk (as VMs)",
                    t.machines,
                    t.cpus,
                    f64::from(t.memory_mb) / 1024.0,
                    t.disk_gb
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Generate { dir, target } => {
            let spec = core::load(&dir)?;
            let problems = core::validate(&spec);
            if !problems.is_empty() {
                for p in &problems {
                    eprintln!("✗ {p}");
                }
                eprintln!("fix the spec first (`isoloom validate`)");
                return Ok(ExitCode::FAILURE);
            }
            let (files, skipped) = match &target {
                Some(id) => {
                    let t = core::Target::ALL
                        .into_iter()
                        .find(|t| t.id() == id)
                        .ok_or_else(|| format!("unknown target `{id}`"))?;
                    match core::generate(&spec, t) {
                        Ok(f) => (f, vec![]),
                        Err(e) => (vec![], vec![e]),
                    }
                }
                None => core::generate_all(&spec),
            };
            for f in &files {
                let path = dir.join(&f.path);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, &f.contents)?;
                println!("✓ {}", f.path);
            }
            for e in &skipped {
                println!("· {e}");
            }
            // Asking for one target that can't be generated is an error; skipping some of "all" isn't.
            Ok(if target.is_some() && files.is_empty() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Coverage { markdown } => {
            use core::coverage::{Output, markdown as md, score, table};
            if markdown {
                print!("{}", md());
                return Ok(ExitCode::SUCCESS);
            }
            let rows = table();
            let width = rows.iter().map(|r| r.path.len()).max().unwrap_or(0);
            print!("{:width$}", "");
            for o in Output::ALL {
                print!("  {:>9}", o.label());
            }
            println!();
            for r in &rows {
                print!("{:width$}", r.path);
                for o in Output::ALL {
                    print!("  {:>9}", r.status(o).symbol());
                }
                println!();
            }
            println!();
            for o in Output::ALL {
                println!("{:>9}: {}", o.label(), score(&rows, o));
            }
            println!("\n✓ done · ◐ partial · n/a: means nothing there · info: descriptive · core: read by isoloom");
            Ok(ExitCode::SUCCESS)
        }
        Command::Check { dir } => {
            let spec = core::load(&dir)?;
            let (files, _) = core::generate_all(&spec);
            let mut stale = 0;
            for f in &files {
                match std::fs::read_to_string(dir.join(&f.path)) {
                    Ok(current) if current == f.contents => println!("✓ {}", f.path),
                    Ok(_) => {
                        println!("✗ {} is out of date", f.path);
                        stale += 1;
                    }
                    Err(_) => {
                        println!("✗ {} is missing", f.path);
                        stale += 1;
                    }
                }
            }
            if stale > 0 {
                println!("run `isoloom generate` and commit the result");
                return Ok(ExitCode::FAILURE);
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}
