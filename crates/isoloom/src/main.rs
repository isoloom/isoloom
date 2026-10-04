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
    /// Draft an isoloom.yml from files you already have.
    #[command(subcommand)]
    Import(Import),
    /// Show which features of each output format (Compose, Vagrant) a spec can produce.
    Coverage {
        /// Every feature, with how Isoloom produces it or why not.
        #[arg(long)]
        all: bool,
        /// The full coverage page as Markdown (both directions).
        #[arg(long)]
        markdown: bool,
    },
}

#[derive(Subcommand)]
enum Import {
    /// From a Compose file: what it says that the format can express goes into the draft;
    /// the rest is listed, with why.
    Compose {
        /// The Compose file (default: compose.yaml, compose.yml, docker-compose.yaml or
        /// docker-compose.yml in the current folder).
        file: Option<PathBuf>,
        /// Where to write isoloom.yml (default: next to the Compose file).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Replace an existing isoloom.yml.
        #[arg(long)]
        force: bool,
        /// Print the draft instead of writing it.
        #[arg(long)]
        stdout: bool,
    },
}

const COMPOSE_FILES: &[&str] = &["compose.yaml", "compose.yml", "docker-compose.yaml", "docker-compose.yml"];

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
        Command::Import(Import::Compose { file, out, force, stdout }) => {
            let file = match file {
                Some(f) => f,
                None => COMPOSE_FILES
                    .iter()
                    .map(PathBuf::from)
                    .find(|p| p.is_file())
                    .ok_or("no compose.yaml or docker-compose.yml here; pass the file")?,
            };
            let text = std::fs::read_to_string(&file).map_err(|e| format!("can't read {}: {e}", file.display()))?;
            let folder = file.canonicalize()?.parent().map(PathBuf::from).unwrap_or_default();
            let fallback = folder.file_name().and_then(|n| n.to_str()).unwrap_or("env").to_string();
            let source = file.file_name().and_then(|n| n.to_str()).unwrap_or("compose.yaml");
            let draft = core::import::compose::draft(&text, &fallback, source)?;

            if stdout {
                print!("{}", draft.yaml);
            } else {
                let dir = out.unwrap_or(folder);
                let target = dir.join("isoloom.yml");
                if (target.exists() || dir.join("isoloom.yaml").exists()) && !force {
                    return Err(format!("{} already has a spec; use --force to replace it, or --stdout", dir.display()).into());
                }
                std::fs::write(&target, &draft.yaml)?;
                println!("✓ wrote {}", target.display());
            }

            // The notes, by kind, most important first; stderr when the draft goes to stdout.
            let mut report = String::new();
            for kind in core::import::NoteKind::ALL {
                let notes: Vec<_> = draft.notes.iter().filter(|n| n.kind == kind).collect();
                if notes.is_empty() {
                    continue;
                }
                report.push_str(&format!("\n{} ({})\n", kind.title(), notes.len()));
                for n in notes {
                    report.push_str(&format!("  {}: {}\n", n.at, n.text));
                }
            }
            let problems = core::parse(&draft.yaml).map(|s| core::validate(&s)).map_err(|e| e.to_string())?;
            if problems.is_empty() {
                report.push_str("\n✓ the draft is valid\n");
            } else {
                report.push_str("\nThe draft needs fixing before it validates:\n");
                for p in &problems {
                    report.push_str(&format!("  {p}\n"));
                }
            }
            if stdout {
                eprint!("{report}")
            } else {
                print!("{report}")
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Coverage { markdown, all } => {
            use core::coverage::{formats, markdown as md};
            if markdown {
                print!("{}", md());
                return Ok(ExitCode::SUCCESS);
            }
            for f in formats() {
                println!("{}: {}", f.name, f.summary());
                if all {
                    let width = f.rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
                    for (key, s) in &f.rows {
                        println!("  {key:width$}  {:<6}  {}", s.implemented(), s.note());
                    }
                    println!();
                }
            }
            if !all {
                println!("\nEvery feature, with how or why not: isoloom coverage --all · as Markdown: --markdown");
            }
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
