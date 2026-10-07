//! `isoloom`: describe an environment once, run it anywhere.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use isoloom_core as core;

mod lifecycle;

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
        /// Also say whether this machine can run each target (tools installed, credentials
        /// present), and what is missing.
        #[arg(long)]
        host: bool,
    },
    /// What this machine can run: for every target, the tools and credentials found and the ones
    /// missing. No spec needed.
    Doctor {
        /// Machine-readable output.
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
        /// Your own image table (YAML): OS images, and the access machine when the spec leaves
        /// it to the runner. See https://www.isoloom.com/en/docs/images
        #[arg(long, value_name = "FILE")]
        images: Option<PathBuf>,
        /// Override a value for this command: `-s machines.web.vm.os=ubuntu-24.04` on the spec,
        /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
        #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
        sets: Vec<String>,
    },
    /// Fail when the generated files under .isoloom/ don't match the spec (for CI).
    Check {
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The image table the files were generated with (see `generate --images`).
        #[arg(long, value_name = "FILE")]
        images: Option<PathBuf>,
        /// Override a value for this command: `-s machines.web.vm.os=ubuntu-24.04` on the spec,
        /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
        #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
        sets: Vec<String>,
    },
    /// The defaults in effect here (built in, your file, the project's, the environment) and
    /// where each comes from. See https://www.isoloom.com/en/docs/defaults
    Defaults {
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Only the built-in layer.
        #[arg(long)]
        system: bool,
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Generate a target's files, then bring the environment up with the right tool
    /// (docker compose, vagrant, kubectl, or terraform).
    Run {
        /// The target to run (docker, vagrant, docker-vm, hybrid, kubernetes, proxmox,
        /// cloud-docker, cloud-vm). Omitted, the single derived target, or an error listing them.
        target: Option<String>,
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// For `cloud-docker`, which cloud's module to apply (aws, azure, gcp, digitalocean,
        /// linode, oci).
        #[arg(long)]
        cloud: Option<String>,
        /// Your own image table (YAML), as for `generate`.
        #[arg(long, value_name = "FILE")]
        images: Option<PathBuf>,
        /// Run as instance N (1-99) of the spec: its own names, Docker blocks and published
        /// ports, files under .isoloom-N/. For several copies on one host.
        #[arg(long, value_name = "N")]
        instance: Option<u8>,
        /// Override a value for this command: `-s machines.web.vm.os=ubuntu-24.04` on the spec,
        /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
        #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
        sets: Vec<String>,
    },
    /// Tear down what `run` started for a target (the inverse tool: compose down, vagrant
    /// destroy, kubectl delete, or terraform destroy).
    Down {
        target: Option<String>,
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long)]
        cloud: Option<String>,
        /// Run as instance N (1-99) of the spec: its own names, Docker blocks and published
        /// ports, files under .isoloom-N/. For several copies on one host.
        #[arg(long, value_name = "N")]
        instance: Option<u8>,
        /// Override a value for this command: `-s machines.web.vm.os=ubuntu-24.04` on the spec,
        /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
        #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
        sets: Vec<String>,
    },
    /// Run the environment's checks against a running target: the spec's (scripts and declared
    /// probes) and the ones Isoloom derives from `services` and `reach`. One line per check;
    /// exits 1 when any fails.
    Test {
        /// The target the environment is running on (as for `run`).
        target: Option<String>,
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// For the cloud targets, which cloud's module (as for `run`).
        #[arg(long)]
        cloud: Option<String>,
        /// Your own image table (YAML), as for `generate`.
        #[arg(long)]
        images: Option<PathBuf>,
        /// Only the spec's own checks; skip the derived ones.
        #[arg(long)]
        no_derived: bool,
        /// Machine-readable results.
        #[arg(long)]
        json: bool,
        /// The SSH private key for the cloud targets (default: your SSH agent and config).
        #[arg(long)]
        ssh_key: Option<PathBuf>,
        /// Run as instance N (1-99) of the spec: its own names, Docker blocks and published
        /// ports, files under .isoloom-N/. For several copies on one host.
        #[arg(long, value_name = "N")]
        instance: Option<u8>,
        /// Override a value for this command: `-s machines.web.vm.os=ubuntu-24.04` on the spec,
        /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
        #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
        sets: Vec<String>,
    },
    /// Run the environment's provisioning again on a running environment, in place: after a
    /// step failed or was interrupted (a host that stopped answering), without a rebuild. With
    /// `provision:` steps, the controller runs them again (booted if halted, halted after),
    /// limited to the named machines; without, the named machines' own `vm.provision` steps run
    /// again. Steps are meant to be idempotent. Local VMs (vagrant, docker-vm, hybrid) for now.
    Provision {
        /// Only these machines (all when none).
        machines: Vec<String>,
        /// The target the environment is running on (as for `run`).
        #[arg(long)]
        target: Option<String>,
        /// The project folder (holding isoloom.yml).
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        /// As instance N of the spec (as for `run`).
        #[arg(long, value_name = "N")]
        instance: Option<u8>,
        /// Override a value for this command (as for `run`). Repeatable.
        #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
        sets: Vec<String>,
    },
    /// The environments `run` brought up on this host, with their live state.
    Status {
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
        /// Tear this environment down (by name) and forget it, from anywhere.
        #[arg(long, value_name = "NAME")]
        cleanup: Option<String>,
    },
    /// Open a shell on a machine of a running environment (docker compose exec, vagrant ssh,
    /// kubectl exec, or ssh for the cloud targets).
    Connect {
        /// The machine.
        machine: String,
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The target it runs on (default: what `run` recorded for this folder).
        #[arg(long)]
        target: Option<String>,
        /// The SSH private key for the cloud targets.
        #[arg(long)]
        ssh_key: Option<PathBuf>,
        /// Run as instance N (1-99) of the spec: its own names, Docker blocks and published
        /// ports, files under .isoloom-N/. For several copies on one host.
        #[arg(long, value_name = "N")]
        instance: Option<u8>,
    },
    /// Run a command on a machine, or on every machine (`all`), of a running environment.
    Exec {
        /// The machine, or `all`.
        machine: String,
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The target it runs on (default: what `run` recorded for this folder).
        #[arg(long)]
        target: Option<String>,
        /// The SSH private key for the cloud targets.
        #[arg(long)]
        ssh_key: Option<PathBuf>,
        /// Run as instance N (1-99) of the spec: its own names, Docker blocks and published
        /// ports, files under .isoloom-N/. For several copies on one host.
        #[arg(long, value_name = "N")]
        instance: Option<u8>,
        /// The command, after `--`.
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// Capture packets on a machine's interface on one of its networks (tcpdump, from inside
    /// the machine's network namespace).
    Capture {
        /// The machine.
        machine: String,
        /// The network (one of the machine's).
        network: String,
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The target it runs on (default: what `run` recorded for this folder).
        #[arg(long)]
        target: Option<String>,
        /// The SSH private key for the cloud targets.
        #[arg(long)]
        ssh_key: Option<PathBuf>,
        /// Run as instance N (1-99) of the spec: its own names, Docker blocks and published
        /// ports, files under .isoloom-N/. For several copies on one host.
        #[arg(long, value_name = "N")]
        instance: Option<u8>,
        /// tcpdump arguments, after `--` (default: `-l -v`).
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Draw the environment: networks, machines with their addresses and services, reach rules,
    /// gateways and the access machine. D2 by default; `--format dot` for Graphviz. With
    /// `-o x.svg` or `-o x.png` the diagram is rendered when `d2` (or `dot`) is installed.
    Graph {
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// `d2` (default) or `dot`.
        #[arg(long, default_value = "d2")]
        format: String,
        /// Write here instead of stdout; `.svg` and `.png` are rendered.
        #[arg(short = 'o', long)]
        out: Option<PathBuf>,
        /// Draw instance N (its blocks and name).
        #[arg(long, value_name = "N")]
        instance: Option<u8>,
        /// Overrides, as for `generate`.
        #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
        sets: Vec<String>,
    },
    /// Print a table about the environment: `addressing`, `services`, `wiring` or `resources`
    /// (text, or `--md` for Markdown). `isoloom report list` names them.
    Report {
        /// The report.
        name: String,
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Markdown instead of aligned text.
        #[arg(long)]
        md: bool,
        /// Report on instance N.
        #[arg(long, value_name = "N")]
        instance: Option<u8>,
        /// Overrides, as for `generate`.
        #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
        sets: Vec<String>,
    },
    /// Link impairment on a running environment's router: `show`, `set` (with `--delay`, `--jitter`,
    /// `--loss`, `--rate`) or `disable` on a network, or `reset` to the spec's `tc`.
    Tc {
        /// `show`, `set`, `disable` or `reset`.
        action: String,
        /// The network (one Isoloom's router is on).
        network: String,
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The target it runs on (default: what `run` recorded for this folder).
        #[arg(long)]
        target: Option<String>,
        /// The instance.
        #[arg(long, value_name = "N")]
        instance: Option<u8>,
        /// For `set`: one-way delay, e.g. 50ms.
        #[arg(long)]
        delay: Option<String>,
        /// For `set`: delay variation, e.g. 5ms.
        #[arg(long)]
        jitter: Option<String>,
        /// For `set`: loss in percent.
        #[arg(long)]
        loss: Option<f64>,
        /// For `set`: rate cap, e.g. 10mbit.
        #[arg(long)]
        rate: Option<String>,
    },
    /// Print the spec's `message` (how to use the environment), its placeholders filled.
    Message {
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The message of instance N.
        #[arg(long, value_name = "N")]
        instance: Option<u8>,
        /// Overrides, as for `generate`.
        #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
        sets: Vec<String>,
    },
    /// Show the resolved snapshot (what `.isoloom/resolved.json` holds): every address, routes,
    /// targets and checks, worked out from the spec. A dotted path narrows it:
    /// `isoloom inspect machines.web.addresses`, `isoloom inspect targets`.
    Inspect {
        /// A dotted path into the snapshot (default: all of it). A folder here means the project.
        what: Option<String>,
        /// The project folder (holding isoloom.yml).
        dir: Option<PathBuf>,
        /// YAML instead of JSON.
        #[arg(long)]
        yaml: bool,
        /// Run as instance N (1-99) of the spec: its own names, Docker blocks and published
        /// ports, files under .isoloom-N/. For several copies on one host.
        #[arg(long, value_name = "N")]
        instance: Option<u8>,
        /// Override a value for this command: `-s machines.web.vm.os=ubuntu-24.04` on the spec,
        /// `-s defaults.cloud.aws.region=eu-west-1` on the defaults. Repeatable.
        #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
        sets: Vec<String>,
    },
    /// Print the JSON Schema of isoloom.yml (for editors: completion, hover docs, errors).
    Schema,
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
        /// Every format, its score and every feature, as JSON (for the website).
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum Import {
    /// From a Compose file: what it says that the format can express goes into the draft;
    /// the rest is listed, with why.
    /// From Terraform: the `.tf` files of a folder, read statically (variables, locals,
    /// for_each and count evaluated); VMs and subnets become the draft, the rest is listed.
    Terraform {
        /// The folder with the .tf files (default: the current folder).
        dir: Option<PathBuf>,
        /// Where to write isoloom.yml (default: that folder).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Replace an existing isoloom.yml.
        #[arg(long)]
        force: bool,
        /// Print the draft instead of writing it.
        #[arg(long)]
        stdout: bool,
    },
    /// From a Vagrantfile: it runs against a stand-in `Vagrant` module (nothing is created), and
    /// the settings it makes become the draft; the rest is listed, with why.
    Vagrant {
        /// The Vagrantfile (default: ./Vagrantfile).
        file: Option<PathBuf>,
        /// Where to write isoloom.yml (default: next to the Vagrantfile).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Replace an existing isoloom.yml.
        #[arg(long)]
        force: bool,
        /// Print the draft instead of writing it.
        #[arg(long)]
        stdout: bool,
    },
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

/// The folder a source file is in, a name for the environment (the folder's), and how the
/// draft's header names the file.
fn source_of(file: &std::path::Path) -> Result<(PathBuf, String, String), Box<dyn std::error::Error>> {
    let folder = file.canonicalize()?.parent().map(PathBuf::from).unwrap_or_default();
    let fallback = folder.file_name().and_then(|n| n.to_str()).unwrap_or("env").to_string();
    let source = file.file_name().and_then(|n| n.to_str()).unwrap_or("the source").to_string();
    Ok((folder, fallback, source))
}

/// Writes a draft (or prints it), then its notes by kind and whether it validates.
fn write_draft(draft: &core::import::Draft, folder: PathBuf, out: Option<PathBuf>, force: bool, stdout: bool) -> Result<ExitCode, Box<dyn std::error::Error>> {
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

/// Runs a Vagrantfile against a stand-in `Vagrant` module (in Vagrant's own Ruby when it's
/// installed) and returns every setting it made, as JSON. Nothing is created or started.
fn record_vagrantfile(file: &std::path::Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    const RECORDER: &str = include_str!("vagrant_record.rb");
    // A private per-run directory, created exclusively (fails if it already exists), so a
    // local attacker can't pre-place a symlink at a guessable path and have us clobber a
    // victim file or run a swapped script.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("isoloom-vagrant-{}-{nonce:x}", std::process::id()));
    std::fs::create_dir(&dir)?;
    let script = dir.join("record.rb");
    std::fs::write(&script, RECORDER)?;
    let cleanup = || {
        let _ = std::fs::remove_dir_all(&dir);
    };
    let rubies = [
        "/opt/vagrant/embedded/bin/ruby",
        "C:\\HashiCorp\\Vagrant\\embedded\\mingw64\\bin\\ruby.exe",
        "ruby",
    ];
    let mut last = String::from("no Ruby found (Vagrant's own, or `ruby` on the PATH)");
    for ruby in rubies {
        match std::process::Command::new(ruby).arg(&script).arg(file).output() {
            Ok(out) if out.status.success() => {
                cleanup();
                return Ok(serde_json::from_slice(&out.stdout)?);
            }
            Ok(out) => {
                last = format!(
                    "the Vagrantfile failed to run: {}",
                    String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("")
                )
            }
            Err(_) => continue,
        }
    }
    cleanup();
    Err(last.into())
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
        Command::Doctor { json } => {
            let all = core::host::all();
            if json {
                let list: Vec<serde_json::Value> = all
                    .iter()
                    .map(|r| serde_json::json!({ "target": r.target.id(), "cloud": r.cloud, "ready": r.ready, "notes": r.notes }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&list)?);
                return Ok(ExitCode::SUCCESS);
            }
            for r in &all {
                let name = match &r.cloud {
                    Some(c) => format!("{} ({c})", r.target.id()),
                    None => r.target.id().to_string(),
                };
                println!("{} {name:<22} {}", if r.ready { "✓" } else { "✗" }, r.summary());
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Targets { dir, json, host } => {
            let spec = core::load(&dir)?;
            let effective = core::effective(&spec);
            // A target can be possible by its machines' editions and still be refused by its
            // generator (a Windows machine on Proxmox, say). A ✓ here means `generate` really
            // produces it; a refusal shows as ✗ with the generator's reason.
            let refused = |t: core::Target| core::refusal(&spec, t);
            // With --host, whether this machine has what each target needs (one line per cloud).
            let on_host = |t: core::Target| -> Vec<core::host::Readiness> {
                if !host {
                    return vec![];
                }
                match t {
                    core::Target::CloudDocker | core::Target::CloudVm => core::host::CLOUDS.iter().map(|c| core::host::check(t, Some(c))).collect(),
                    _ => vec![core::host::check(t, None)],
                }
            };
            if json {
                let ready: Vec<core::Target> = effective.iter().copied().filter(|t| refused(*t).is_none()).collect();
                if host {
                    let list: Vec<serde_json::Value> = ready
                        .iter()
                        .map(|t| {
                            let checks: Vec<serde_json::Value> = on_host(*t)
                                .iter()
                                .map(|r| serde_json::json!({ "cloud": r.cloud, "ready": r.ready, "notes": r.notes }))
                                .collect();
                            serde_json::json!({ "target": t.id(), "host": checks })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&list)?);
                } else {
                    let ids: Vec<&str> = ready.iter().map(|t| t.id()).collect();
                    println!("{}", serde_json::to_string_pretty(&ids)?);
                }
                return Ok(ExitCode::SUCCESS);
            }
            for t in core::Target::ALL {
                if effective.contains(&t) {
                    match refused(t) {
                        None => {
                            println!("✓ {}", t.id());
                            for r in on_host(t) {
                                let label = r.cloud.as_deref().map(|c| format!(" {c}")).unwrap_or_default();
                                println!("    host{label}: {}", r.summary());
                            }
                        }
                        Some(why) => println!("✗ {} ({why})", t.id()),
                    }
                } else {
                    let lacking = core::targets::missing(&spec, t.needs());
                    let why = if !lacking.is_empty() {
                        format!("needs `{}:` on {}", t.needs().key(), lacking.join(", "))
                    } else if t == core::Target::Hybrid && !core::targets::mixed(&spec) {
                        "only when some machines are containers and others are VMs".to_string()
                    } else {
                        "left out by `targets:`".to_string()
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
        Command::Generate { dir, target, images, sets } => {
            let (spec, settings) = load_settings(&dir, images.as_deref(), &sets)?;
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
            let files = core::defaults::apply_to_files(files, &settings.defaults);
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
        Command::Run {
            target,
            dir,
            cloud,
            images,
            instance,
            sets,
        } => run_cmd(&dir, target.as_deref(), cloud.as_deref(), images.as_deref(), instance, &sets, false),
        Command::Down {
            target,
            dir,
            cloud,
            instance,
            sets,
        } => run_cmd(&dir, target.as_deref(), cloud.as_deref(), None, instance, &sets, true),
        Command::Test {
            target,
            dir,
            cloud,
            images,
            no_derived,
            json,
            ssh_key,
            instance,
            sets,
        } => test_cmd(
            &dir,
            target.as_deref(),
            TestOpts {
                cloud: cloud.as_deref(),
                images: images.as_deref(),
                no_derived,
                json,
                ssh_key: ssh_key.as_deref(),
                instance,
                sets: &sets,
            },
        ),
        Command::Provision {
            machines,
            target,
            dir,
            instance,
            sets,
        } => provision_cmd(&dir, target.as_deref(), &machines, instance, &sets),
        Command::Status { json, cleanup } => lifecycle::status(json, cleanup.as_deref(), None),
        Command::Connect {
            machine,
            dir,
            target,
            ssh_key,
            instance,
        } => lifecycle::connect(&abs(&dir)?, target.as_deref(), instance, &machine, ssh_key.as_deref()),
        Command::Exec {
            machine,
            dir,
            target,
            ssh_key,
            instance,
            command,
        } => lifecycle::exec(&abs(&dir)?, target.as_deref(), instance, &machine, &command, ssh_key.as_deref()),
        Command::Capture {
            machine,
            network,
            dir,
            target,
            ssh_key,
            instance,
            args,
        } => lifecycle::capture(&abs(&dir)?, target.as_deref(), instance, &machine, &network, &args, ssh_key.as_deref()),
        Command::Tc {
            action,
            network,
            dir,
            target,
            instance,
            delay,
            jitter,
            loss,
            rate,
        } => lifecycle::tc(
            &abs(&dir)?,
            target.as_deref(),
            instance,
            &action,
            &network,
            core::Tc { delay, jitter, loss, rate },
        ),
        Command::Message { dir, instance, sets } => {
            let (spec, _) = load_settings(&dir, None, &sets)?;
            let problems = core::validate(&spec);
            if !problems.is_empty() {
                for p in &problems {
                    eprintln!("✗ {p}");
                }
                return Err("fix the spec first (`isoloom validate`)".into());
            }
            let spec = match instance {
                Some(n) => core::instance::apply(&spec, n)?,
                None => spec,
            };
            match core::resolved::render_message(&spec, instance)? {
                Some(m) => println!("{}", m.trim_end()),
                None => eprintln!("the spec has no `message:`"),
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Graph {
            dir,
            format,
            out,
            instance,
            sets,
        } => {
            let snapshot = snapshot_of(&dir, instance, &sets)?;
            let text = match format.as_str() {
                "d2" => core::report::graph_d2(&snapshot),
                "dot" => core::report::graph_dot(&snapshot),
                other => return Err(format!("unknown format `{other}`; `d2` or `dot`").into()),
            };
            let Some(out) = out else {
                print!("{text}");
                return Ok(ExitCode::SUCCESS);
            };
            let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("");
            if matches!(ext, "svg" | "png") {
                // Rendered by the format's own tool, from a sibling source file.
                let src = out.with_extension(&format);
                std::fs::write(&src, &text)?;
                let (program, args): (&str, Vec<String>) = match format.as_str() {
                    "d2" => ("d2", vec![src.display().to_string(), out.display().to_string()]),
                    _ => (
                        "dot",
                        vec![format!("-T{ext}"), "-o".into(), out.display().to_string(), src.display().to_string()],
                    ),
                };
                match std::process::Command::new(program).args(&args).status() {
                    Ok(st) if st.success() => {
                        println!("✓ {} (source: {})", out.display(), src.display());
                        Ok(ExitCode::SUCCESS)
                    }
                    Ok(_) => Err(format!("{program} failed; the source is at {}", src.display()).into()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!(
                        "`{program}` isn't installed, so the diagram isn't rendered; the source is at {} ({})",
                        src.display(),
                        if format == "d2" {
                            "https://d2lang.com/tour/install"
                        } else {
                            "https://graphviz.org/download/"
                        }
                    )
                    .into()),
                    Err(e) => Err(e.into()),
                }
            } else {
                std::fs::write(&out, &text)?;
                println!("✓ {}", out.display());
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::Report { name, dir, md, instance, sets } => {
            if name == "list" {
                for (n, what) in core::report::REPORTS {
                    println!("{n:<12} {what}");
                }
                return Ok(ExitCode::SUCCESS);
            }
            let snapshot = snapshot_of(&dir, instance, &sets)?;
            print!("{}", core::report::report(&name, &snapshot, md)?);
            Ok(ExitCode::SUCCESS)
        }
        Command::Inspect {
            what,
            dir,
            yaml,
            instance,
            sets,
        } => {
            // `isoloom inspect examples/segmented` names the project, not a path in the snapshot.
            let (what, dir) = match (what, dir) {
                (Some(w), None) if w.contains('/') || std::path::Path::new(&w).is_dir() => (None, PathBuf::from(w)),
                (w, d) => (w, d.unwrap_or_else(|| PathBuf::from("."))),
            };
            let (spec, _) = load_settings(&dir, None, &sets)?;
            let problems = core::validate(&spec);
            if !problems.is_empty() {
                for p in &problems {
                    eprintln!("✗ {p}");
                }
                return Err("fix the spec first (`isoloom validate`)".into());
            }
            let spec = match instance {
                Some(n) => core::instance::apply(&spec, n)?,
                None => spec,
            };
            let all = core::resolved::resolve_with(&spec, instance);
            let value = match &what {
                Some(path) => {
                    core::resolved::lookup(&all, path).ok_or_else(|| format!("nothing at `{path}` in the snapshot; try `isoloom inspect` to see it all"))?
                }
                None => &all,
            };
            if yaml {
                print!("{}", serde_yaml_ng::to_string(value)?);
            } else {
                println!("{}", serde_json::to_string_pretty(value)?);
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Schema => {
            println!("{}", serde_json::to_string_pretty(&core::schema::schema())?);
            Ok(ExitCode::SUCCESS)
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
            let (folder, fallback, source) = source_of(&file)?;
            let draft = core::import::compose::draft(&text, &fallback, &source)?;
            write_draft(&draft, folder, out, force, stdout)
        }
        Command::Import(Import::Terraform { dir, out, force, stdout }) => {
            let dir = dir.unwrap_or_else(|| PathBuf::from("."));
            let mut files = Vec::new();
            for e in std::fs::read_dir(&dir).map_err(|e| format!("can't read {}: {e}", dir.display()))? {
                let p = e?.path();
                if p.extension().and_then(|x| x.to_str()) == Some("tf") {
                    files.push((p.file_name().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p)?));
                }
            }
            files.sort();
            if files.is_empty() {
                return Err(format!("no .tf files in {}", dir.display()).into());
            }
            let folder = dir.canonicalize()?;
            let fallback = folder.file_name().and_then(|n| n.to_str()).unwrap_or("env").to_string();
            let source = format!("{} (.tf files)", folder.file_name().and_then(|n| n.to_str()).unwrap_or("."));
            let draft = core::import::terraform::draft(&files, &fallback, &source)?;
            write_draft(&draft, folder, out, force, stdout)
        }
        Command::Import(Import::Vagrant { file, out, force, stdout }) => {
            let file = file.unwrap_or_else(|| PathBuf::from("Vagrantfile"));
            if !file.is_file() {
                return Err(format!("no {} here; pass the Vagrantfile", file.display()).into());
            }
            let recorded = record_vagrantfile(&file)?;
            let (folder, fallback, source) = source_of(&file)?;
            let draft = core::import::vagrant::draft(&recorded, &fallback, &source)?;
            write_draft(&draft, folder, out, force, stdout)
        }
        Command::Coverage { markdown, all, json } => {
            use core::coverage::{anchor, capitalize, formats, markdown as md, section};
            if markdown {
                print!("{}", md());
                return Ok(ExitCode::SUCCESS);
            }
            if json {
                let out: Vec<serde_json::Value> = formats()
                    .iter()
                    .map(|f| {
                        let score = f.score();
                        let mut sections: Vec<serde_json::Value> = Vec::new();
                        let mut collapsed = 0;
                        for (key, s) in &f.rows {
                            if f.collapse_not_portable && matches!(s, core::coverage::Support::NotPortable { .. }) {
                                collapsed += 1;
                                continue;
                            }
                            let (title, name) = section(key);
                            if sections.last().and_then(|x| x["title"].as_str()) != Some(title) {
                                sections.push(serde_json::json!({ "title": title, "rows": [] }));
                            }
                            let row = serde_json::json!({
                                "key": name,
                                "every_target": s.portable(),
                                "implemented": s.implemented(),
                                "note": capitalize(&s.note()),
                            });
                            sections.last_mut().unwrap()["rows"].as_array_mut().unwrap().push(row);
                        }
                        serde_json::json!({
                            "name": f.name,
                            "anchor": anchor(f.name),
                            "source": f.source,
                            "features": f.rows.len(),
                            "score": { "percent": score.percent(), "portable": score.portable, "done": score.done, "partly": score.partly, "to_do": score.to_do },
                            "collapsed_not_portable": collapsed,
                            "sections": sections,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&out)?);
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
        Command::Defaults { dir, system, json } => {
            let value = if system {
                core::defaults::builtin()
            } else {
                let r = core::defaults::load(&dir, None, &[])?;
                let mut merged = core::defaults::builtin();
                core::defaults::merge(&mut merged, &r.merged);
                if json {
                    let sources: serde_json::Map<String, serde_json::Value> = r.sources.iter().map(|(k, v)| (k.clone(), serde_json::json!(v))).collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &serde_json::json!({ "defaults": serde_json::to_value(&merged)?, "sources": sources, "user_file": core::defaults::user_file(), "project_file": dir.join(core::defaults::PROJECT_FILE) })
                        )?
                    );
                    return Ok(ExitCode::SUCCESS);
                }
                let builtin_leaves = core::defaults::leaves(&core::defaults::builtin(), "");
                let width = r
                    .sources
                    .iter()
                    .map(|(k, _)| k.len())
                    .chain(builtin_leaves.iter().map(String::len))
                    .max()
                    .unwrap_or(0);
                for leaf in core::defaults::leaves(&merged, "") {
                    let value = leaf
                        .split('.')
                        .try_fold(&merged, |v, k| v.get(k))
                        .map(|v| serde_yaml_ng::to_string(v).unwrap_or_default().trim().to_string())
                        .unwrap_or_default();
                    let source = r.sources.iter().find(|(k, _)| *k == leaf).map(|(_, s)| s.as_str()).unwrap_or("built in");
                    println!("{leaf:<width$}  {value:<24}  {source}");
                }
                println!(
                    "\nfiles: {} (yours), {} (the project's); environment ISOLOOM_<KEY>; -s defaults.<key>=<value>",
                    core::defaults::user_file().display(),
                    dir.join(core::defaults::PROJECT_FILE).display()
                );
                return Ok(ExitCode::SUCCESS);
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&serde_json::to_value(&value)?)?);
            } else {
                print!("{}", serde_yaml_ng::to_string(&value)?);
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Check { dir, images, sets } => {
            let (spec, settings) = load_settings(&dir, images.as_deref(), &sets)?;
            // Generators assume a validated spec (they `expect` valid CIDRs and addresses). A spec
            // that parses but is invalid must fail with the field and reason, not a panic.
            let problems = core::validate(&spec);
            if !problems.is_empty() {
                for p in &problems {
                    eprintln!("✗ {p}");
                }
                eprintln!("fix the spec first (`isoloom validate`)");
                return Ok(ExitCode::FAILURE);
            }
            let (files, _) = core::generate_all(&spec);
            let files = core::defaults::apply_to_files(files, &settings.defaults);
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

/// `isoloom run`/`down`: pick the target, generate its files, then bring it up (or tear it down)
/// with the tool that owns that output. `down` is the same dispatch with the inverse command.
/// Loads and validates the spec, applies the image table, picks the target (the one named, the
/// only possibility, or an error listing them) and writes that target's files, so a run or a
/// test is always against the current spec.
fn prepare(
    dir: &std::path::Path,
    target: Option<&str>,
    images: Option<&std::path::Path>,
    instance: Option<u8>,
    sets: &[String],
) -> Result<(core::Spec, PathBuf, core::Target, core::defaults::Defaults), Box<dyn std::error::Error>> {
    let (spec, settings) = load_settings(dir, images, sets)?;
    // Absolute, so the paths we hand to docker/vagrant/terraform don't depend on their working
    // directory (compose runs from `dir`, terraform and vagrant from the module folder).
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let problems = core::validate(&spec);
    if !problems.is_empty() {
        for p in &problems {
            eprintln!("✗ {p}");
        }
        return Err("fix the spec first (`isoloom validate`)".into());
    }

    let possible = core::effective(&spec);
    let t = match target {
        Some(id) => core::Target::ALL
            .into_iter()
            .find(|t| t.id() == id)
            .ok_or_else(|| format!("unknown target `{id}`"))?,
        None => match possible.as_slice() {
            [one] => *one,
            [] => return Err("this spec has no runnable target; see `isoloom targets`".into()),
            many => {
                let ids: Vec<&str> = many.iter().map(|t| t.id()).collect();
                return Err(format!("several targets are possible ({}); pass one, e.g. `isoloom run {}`", ids.join(", "), ids[0]).into());
            }
        },
    };
    if !possible.contains(&t) {
        return Err(format!("this spec can't run on `{}`; see `isoloom targets`", t.id()).into());
    }

    let files = match instance {
        Some(n) => core::generate_instance(&spec, t, n),
        None => core::generate(&spec, t),
    }
    .map_err(|e| format!("can't generate `{}`: {e}", t.id()))?;
    let files = core::defaults::apply_to_files(files, &settings.defaults);
    for f in &files {
        let path = dir.join(&f.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &f.contents)?;
    }
    // The spec as the instance, so names and ports downstream are the instance's.
    let spec = match instance {
        Some(n) => core::instance::apply(&spec, n)?,
        None => spec,
    };
    Ok((spec, dir, t, settings.defaults))
}

/// The spec with `-s` overrides and the image table applied, and the defaults in effect.
fn load_settings(
    dir: &std::path::Path,
    images: Option<&std::path::Path>,
    sets: &[String],
) -> Result<(core::Spec, core::defaults::Resolved), Box<dyn std::error::Error>> {
    let spec = core::load_with(dir, sets)?;
    let settings = core::defaults::load(dir, images, sets)?;
    let spec = settings.defaults.images.apply(&spec);
    Ok((spec, settings))
}

fn run_cmd(
    dir: &std::path::Path,
    target: Option<&str>,
    cloud: Option<&str>,
    images: Option<&std::path::Path>,
    instance: Option<u8>,
    sets: &[String],
    down: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let (spec, dir, t, defaults) = prepare(dir, target, images, instance, sets)?;
    let dir = &dir;
    if !down {
        let ready = core::host::check(t, cloud.or((t == core::Target::CloudVm).then_some("aws")));
        if !ready.ready {
            return Err(format!(
                "this machine can't run `{}` yet: {} (see `isoloom doctor`)",
                t.id(),
                ready.summary().trim_start_matches("not ready: ")
            )
            .into());
        }
    }
    if t == core::Target::External && !down {
        lifecycle::external_up(dir, &spec, instance)?;
    }
    let (program, mut args, wd) = bring_up(dir, t, cloud, instance, down)?;
    // The preferred Vagrant provider, from the defaults.
    if !down
        && program == "vagrant"
        && let Some(p) = &defaults.vagrant.provider
    {
        args.extend(["--provider".to_string(), p.clone()]);
    }
    eprintln!("{} {} ({})", if down { "Tearing down" } else { "Running" }, t.id(), wd.display());
    let status = std::process::Command::new(&program).args(&args).current_dir(&wd).status();
    match status {
        Ok(s) if s.success() => {
            // Remember what is up on this host, for status / connect / exec / capture.
            let recorded = core::registry::load().and_then(|mut reg| {
                if down {
                    reg.remove(dir, t, instance);
                } else {
                    reg.upsert(core::registry::Entry {
                        name: spec.name.clone(),
                        dir: dir.clone(),
                        target: t,
                        instance,
                        cloud: cloud.map(str::to_string),
                        started: core::registry::now(),
                    });
                }
                core::registry::save(&reg)
            });
            if let Err(e) = recorded {
                eprintln!("note: couldn't update {}: {e}", core::registry::path().display());
            }
            // The spec's message, now that the environment is up.
            if !down && let Ok(Some(m)) = core::resolved::render_message(&spec, instance) {
                println!("\n{}", m.trim_end());
            }
            Ok(ExitCode::SUCCESS)
        }
        Ok(s) => Err(format!("{program} exited with {s}").into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!("`{program}` isn't installed").into()),
        Err(e) => Err(Box::new(e)),
    }
}

/// One check runner to execute: where it stands, and the command that runs it and prints the
/// `isoloom-check:` lines.
struct Runner {
    label: String,
    program: String,
    args: Vec<String>,
    wd: PathBuf,
    env: Vec<(String, String)>,
}

#[derive(serde::Serialize)]
struct Outcome {
    position: String,
    name: String,
    ok: bool,
    detail: String,
}

/// What `isoloom test` was asked beyond the target.
struct TestOpts<'a> {
    cloud: Option<&'a str>,
    images: Option<&'a std::path::Path>,
    no_derived: bool,
    json: bool,
    ssh_key: Option<&'a std::path::Path>,
    instance: Option<u8>,
    sets: &'a [String],
}

fn test_cmd(dir: &std::path::Path, target: Option<&str>, opts: TestOpts) -> Result<ExitCode, Box<dyn std::error::Error>> {
    use core::checks::{self, Line, Position};
    let TestOpts {
        cloud,
        images,
        no_derived,
        json,
        ssh_key,
        instance,
        sets,
    } = opts;
    let (spec, dir, t, _) = prepare(dir, target, images, instance, sets)?;
    let plan = checks::plan(&spec);
    let expected: Vec<&checks::Resolved> = plan.iter().filter(|c| !(no_derived && c.derived)).collect();
    if expected.is_empty() {
        if !json {
            println!("nothing to check: no `checks:` in the spec, and no derived checks (no services other machines could reach)");
        }
        return Ok(ExitCode::SUCCESS);
    }
    let groups = checks::by_position(&spec, &plan);
    let default_pos = checks::default_position(&spec);
    let runner_name = |pos: &Position| {
        if *pos == default_pos {
            "isoloom-check".to_string()
        } else {
            format!("isoloom-check-{}", pos.id())
        }
    };
    let out = dir.join(core::instance::output_dir(instance));
    let derived_env = |runner: &mut Runner| {
        if no_derived {
            runner.env.push(("ISOLOOM_DERIVED".into(), "0".into()));
        }
    };
    let s = |x: &str| x.to_string();

    let mut runners: Vec<Runner> = Vec::new();
    // A manifest written for this run (Kubernetes with --no-derived), removed at the end.
    let mut temp: Option<PathBuf> = None;
    match t {
        core::Target::Docker | core::Target::Hosted => {
            let f = out.join("docker/compose.yml").display().to_string();
            for (pos, _) in &groups {
                let mut args = vec![
                    s("compose"),
                    s("--progress"),
                    s("quiet"),
                    s("-f"),
                    f.clone(),
                    s("--profile"),
                    s("check"),
                    s("run"),
                    s("--rm"),
                ];
                if no_derived {
                    args.extend([s("-e"), s("ISOLOOM_DERIVED=0")]);
                }
                args.push(runner_name(pos));
                runners.push(Runner {
                    label: pos.label(),
                    program: s("docker"),
                    args,
                    wd: dir.clone(),
                    env: vec![],
                });
            }
        }
        core::Target::Vagrant | core::Target::DockerVm | core::Target::Hybrid => {
            let sub = match t {
                core::Target::Vagrant => "vagrant",
                core::Target::DockerVm => "docker-vm",
                _ => "hybrid",
            };
            let mut r = Runner {
                label: "every machine".into(),
                program: s("vagrant"),
                args: vec![s("provision"), s("--provision-with"), s("checks")],
                wd: out.join(sub),
                env: vec![],
            };
            derived_env(&mut r);
            runners.push(r);
        }
        core::Target::Kubernetes => {
            let ns = format!("isoloom-{}", spec.name);
            let kdir = out.join("kubernetes/checks").display().to_string();
            let names: Vec<String> = groups.iter().map(|(p, _)| runner_name(p)).collect();
            // The Jobs are static manifests: with --no-derived they are rendered here, every
            // runner gets ISOLOOM_DERIVED=0, and that is what gets applied.
            let manifests = if no_derived {
                let patched = derived_off(&kubectl_kustomize(&kdir)?)?;
                let nonce = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let dir = std::env::temp_dir().join(format!("isoloom-k8s-{}-{nonce:x}", std::process::id()));
                std::fs::create_dir(&dir)?;
                let file = dir.join("checks.yaml");
                std::fs::write(&file, patched)?;
                temp = Some(dir);
                format!("kubectl apply -f {} >/dev/null", core::checks::sq(&file.display().to_string()))
            } else {
                format!("kubectl kustomize --load-restrictor LoadRestrictionsNone {kdir} | kubectl apply -f - >/dev/null")
            };
            // Fresh Jobs (a Job can't be re-run), then each one's logs once it has finished either way.
            let apply = format!("kubectl -n {ns} delete job {} --ignore-not-found >/dev/null && {manifests}", names.join(" "));
            runners.push(Runner {
                label: "starting the check Jobs".into(),
                program: s("sh"),
                args: vec![s("-c"), apply],
                wd: dir.clone(),
                env: vec![],
            });
            for (pos, _) in &groups {
                let job = runner_name(pos);
                let script = format!(
                    "i=0; until [ \"$(kubectl -n {ns} get job {job} -o jsonpath='{{.status.conditions[*].type}}' 2>/dev/null)\" != \"\" ]; do i=$((i+2)); [ $i -ge 300 ] && break; sleep 2; done; kubectl -n {ns} logs job/{job} --all-containers 2>/dev/null || true"
                );
                runners.push(Runner {
                    label: pos.label(),
                    program: s("sh"),
                    args: vec![s("-c"), script],
                    wd: dir.clone(),
                    env: vec![],
                });
            }
        }
        core::Target::CloudVm => {
            let module = out.join("cloud-vm").join(cloud.unwrap_or("aws"));
            let outputs = terraform_output(&module)?;
            let Some(list) = outputs.get("checks").and_then(|v| v.as_array()) else {
                return Err("the module has no `checks` output: run `isoloom run cloud-vm` first".into());
            };
            for entry in list {
                let host = entry["host"].as_str().unwrap_or_default();
                let user = entry["user"].as_str().unwrap_or("root");
                let command = entry["command"].as_str().unwrap_or_default();
                let position = entry["position"].as_str().unwrap_or_default();
                let mut r = ssh_runner(ssh_key, user, host, command, no_derived);
                r.label = if position == "networks" {
                    s("from the environment's networks")
                } else {
                    format!("from {position}")
                };
                runners.push(r);
            }
        }
        core::Target::CloudDocker => {
            let cloud = cloud.ok_or("`cloud-docker` needs --cloud (aws, azure, gcp, digitalocean, linode, oci)")?;
            let outputs = terraform_output(&out.join("cloud-docker").join(cloud))?;
            let host = outputs["ip"]
                .as_str()
                .ok_or("the module has no `ip` output: run `isoloom run cloud-docker` first")?;
            let user = outputs["ssh_user"].as_str().unwrap_or("root");
            for (pos, _) in &groups {
                let command = format!(
                    "cd /opt/isoloom && sudo docker compose -f .isoloom/docker/compose.yml --profile check run --rm -e ISOLOOM_DERIVED {}",
                    runner_name(pos)
                );
                let mut r = ssh_runner(ssh_key, user, host, &command, no_derived);
                r.label = pos.label();
                runners.push(r);
            }
        }
        core::Target::Proxmox => {
            // Each runner piped to its machine over SSH, through the router.
            let outputs = terraform_output(&out.join("proxmox"))?;
            let router = outputs["address"]
                .as_str()
                .ok_or("the module has no `address` output: run `isoloom run proxmox` first")?
                .to_string();
            let Some(list) = outputs.get("checks").and_then(|v| v.as_array()) else {
                return Err("the module has no `checks` output: run `isoloom run proxmox` first".into());
            };
            for entry in list {
                let host = entry["host"].as_str().unwrap_or_default();
                let user = entry["user"].as_str().unwrap_or("isoloom");
                let position = entry["position"].as_str().unwrap_or_default();
                let script = out.join("proxmox/checks").join(format!("{position}.sh"));
                let derived = if no_derived { "ISOLOOM_DERIVED=0 " } else { "" };
                let mut r = ssh_runner(ssh_key, user, host, &format!("{derived}sh -s"), false);
                r.args.insert(0, format!("isoloom@{router}"));
                r.args.insert(0, s("-J"));
                let ssh_line = format!(
                    "ssh {} < {}",
                    r.args.iter().map(|a| core::checks::sq(a)).collect::<Vec<_>>().join(" "),
                    core::checks::sq(&script.display().to_string())
                );
                r.program = s("sh");
                r.args = vec![s("-c"), ssh_line];
                r.label = if position == "networks" {
                    s("from the environment's networks")
                } else {
                    format!("from {position}")
                };
                runners.push(r);
            }
        }
        core::Target::External => {
            // Each machine's runner, piped to its shell over SSH.
            let machines = lifecycle::external_machines(&out)?;
            for (pos, _) in &groups {
                let core::checks::Position::Machine(m) = pos else { continue };
                let Some(e) = machines.get(m) else { continue };
                let script = out.join("external/checks").join(format!("{m}.sh"));
                if !script.exists() {
                    continue;
                }
                let derived = if no_derived { "ISOLOOM_DERIVED=0 " } else { "" };
                let mut r = ssh_runner(
                    ssh_key.or(e.key.as_deref().map(std::path::Path::new)),
                    &e.user,
                    &e.address,
                    &format!("{derived}sh -s"),
                    false,
                );
                r.args.insert(r.args.len() - 2, format!("-p{}", e.port));
                // `sh -s < script`: through a shell so the runner's stdin is the file.
                let ssh_line = format!(
                    "ssh {} < {}",
                    r.args.iter().map(|a| core::checks::sq(a)).collect::<Vec<_>>().join(" "),
                    core::checks::sq(&script.display().to_string())
                );
                r.program = s("sh");
                r.args = vec![s("-c"), ssh_line];
                r.label = pos.label();
                runners.push(r);
            }
        }
    }

    // A controller halted after provisioning runs checks again: booted for them, halted after.
    let _rehalt = match t {
        core::Target::Vagrant | core::Target::DockerVm | core::Target::Hybrid => wake_controller(&out, t, json)?,
        _ => None,
    };

    // Run each, reading the PASS/FAIL lines (whatever prefix the tool adds), streaming the rest.
    let mut outcomes: Vec<Outcome> = Vec::new();
    let mut finished = 0usize;
    let mut broken: Vec<String> = Vec::new();
    for r in &runners {
        if !json {
            println!("{}", r.label);
        }
        let mut cmd = std::process::Command::new(&r.program);
        cmd.args(&r.args).current_dir(&r.wd).stdout(std::process::Stdio::piped());
        for (k, v) in &r.env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!("`{}` isn't installed", r.program),
            _ => e.to_string(),
        })?;
        let stdout = child.stdout.take().expect("piped");
        let mut ended = false;
        use std::io::BufRead;
        for line in std::io::BufReader::new(stdout).lines() {
            let line = line?;
            // Vagrant prefixes each VM's output with its name: it says which machine ran the check.
            let position = line
                .split_once("isoloom-check: ")
                .map(|(pre, _)| pre.trim().trim_end_matches(':').to_string())
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| r.label.clone());
            match checks::parse_line(&line) {
                Some(Line::Pass(name)) => {
                    if !json {
                        println!("  ✓ {name}");
                    }
                    outcomes.push(Outcome {
                        position,
                        name,
                        ok: true,
                        detail: String::new(),
                    });
                }
                Some(Line::Fail(name, why)) => {
                    if !json {
                        println!("  ✗ {name}: {why}");
                    }
                    outcomes.push(Outcome {
                        position,
                        name,
                        ok: false,
                        detail: why,
                    });
                }
                Some(Line::End(_, _)) => {
                    ended = true;
                    finished += 1;
                }
                None if !json && !line.trim().is_empty() => println!("    {line}"),
                None => {}
            }
        }
        let _ = child.wait();
        if !ended && r.label != "starting the check Jobs" && !(matches!(t, core::Target::Vagrant | core::Target::DockerVm | core::Target::Hybrid)) {
            broken.push(r.label.clone());
        }
    }
    let _ = finished;
    if let Some(d) = temp {
        let _ = std::fs::remove_dir_all(d);
    }
    let passed = outcomes.iter().filter(|o| o.ok).count();
    let failed = outcomes.iter().filter(|o| !o.ok).count();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "target": t.id(),
                "passed": passed,
                "failed": failed,
                "unfinished": broken,
                "results": outcomes,
            }))?
        );
    } else {
        println!();
        if failed == 0 && broken.is_empty() {
            println!("{passed} passed");
        } else {
            println!("{passed} passed, {failed} failed");
        }
        for b in &broken {
            println!("✗ the runner {b} didn't finish (see its output above)");
        }
    }
    Ok(if failed == 0 && broken.is_empty() && !outcomes.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// `terraform output -json` of a module, as a map of output name to value.
fn terraform_output(module: &std::path::Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let out = std::process::Command::new("terraform")
        .args(["output", "-json"])
        .current_dir(module)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "`terraform` isn't installed".to_string()
            } else {
                e.to_string()
            }
        })?;
    if !out.status.success() {
        return Err(format!(
            "terraform output failed in {}: {}",
            module.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    let raw: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    // Each output is { "value": ..., "type": ... }: keep the values.
    let mut map = serde_json::Map::new();
    if let Some(obj) = raw.as_object() {
        for (k, v) in obj {
            map.insert(k.clone(), v.get("value").cloned().unwrap_or(serde_json::Value::Null));
        }
    }
    Ok(serde_json::Value::Object(map))
}

/// `kubectl kustomize` of a folder, as text.
/// `isoloom provision`: the environment's provisioning again, in place (see the command's help).
fn provision_cmd(
    dir: &std::path::Path,
    target: Option<&str>,
    machines: &[String],
    instance: Option<u8>,
    sets: &[String],
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let (spec, dir, t, _) = prepare(dir, target, None, instance, sets)?;
    if let Some(m) = machines.iter().find(|m| !spec.machines.contains_key(*m)) {
        return Err(format!("no machine named `{m}` in the spec").into());
    }
    let sub = match t {
        core::Target::Vagrant => "vagrant",
        core::Target::DockerVm => "docker-vm",
        core::Target::Hybrid => "hybrid",
        other => {
            return Err(format!(
                "`isoloom provision` runs on local VMs (vagrant, docker-vm, hybrid) for now, not `{}`",
                other.id()
            )
            .into());
        }
    };
    let out = dir.join(core::instance::output_dir(instance));
    let wd = out.join(sub);
    if !wd.join(".vagrant").is_dir() {
        return Err(format!("nothing runs here yet: `isoloom run {}` first", t.id()).into());
    }
    let vagrant = |args: &[&str]| -> Result<bool, Box<dyn std::error::Error>> {
        Ok(std::process::Command::new("vagrant").args(args).current_dir(&wd).status()?.success())
    };
    if spec.provision.is_empty() {
        // No controller steps: each machine's own, again.
        let mut args = vec!["provision"];
        args.extend(machines.iter().map(String::as_str));
        return Ok(if vagrant(&args)? { ExitCode::SUCCESS } else { ExitCode::FAILURE });
    }
    let _rehalt = wake_controller(&out, t, false)?;
    // The project's current files (playbooks edited since), then the playbooks.
    if !vagrant(&["provision", CONTROLLER, "--provision-with", "file,project"])? {
        return Err("couldn't copy the project to the controller".into());
    }
    println!(
        "provisioning {} from the controller",
        if machines.is_empty() {
            "every machine".to_string()
        } else {
            machines.join(", ")
        }
    );
    // The script on stdin: no quoting through the host's shell, whatever the host.
    let mut child = std::process::Command::new("vagrant")
        .args(["ssh", CONTROLLER, "-c", "sudo sh -s"])
        .current_dir(&wd)
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().expect("piped");
        stdin.write_all(core::generate::provision_script(&spec, machines).as_bytes())?;
    }
    Ok(if child.wait()?.success() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// Halts the controller again when dropped (it was booted for the checks).
struct Rehalt(PathBuf);

impl Drop for Rehalt {
    fn drop(&mut self) {
        let _ = std::process::Command::new("vagrant")
            .args(["halt", CONTROLLER])
            .current_dir(&self.0)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

const CONTROLLER: &str = "isoloom-controller";

/// Boots the controller of a Vagrant output when it is halted (as it is after provisioning);
/// the guard halts it again.
fn wake_controller(out: &std::path::Path, t: core::Target, quiet: bool) -> Result<Option<Rehalt>, Box<dyn std::error::Error>> {
    let wd = out.join(match t {
        core::Target::Vagrant => "vagrant",
        core::Target::DockerVm => "docker-vm",
        _ => "hybrid",
    });
    let vagrantfile = std::fs::read_to_string(wd.join("Vagrantfile")).unwrap_or_default();
    if !vagrantfile.contains(&format!("config.vm.define \"{CONTROLLER}\"")) {
        return Ok(None);
    }
    let state = |wd: &std::path::Path| -> Result<bool, Box<dyn std::error::Error>> {
        let out = std::process::Command::new("vagrant")
            .args(["status", CONTROLLER, "--machine-readable"])
            .current_dir(wd)
            .output()?;
        Ok(controller_running(&String::from_utf8_lossy(&out.stdout)))
    };
    if state(&wd)? {
        // Running, unless it's about to halt (provisioning just ended): then wait for it.
        let halting = std::process::Command::new("vagrant")
            .args(["ssh", CONTROLLER, "-c", "test -e /run/isoloom-halting"])
            .current_dir(&wd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !halting {
            return Ok(None);
        }
        for _ in 0..60 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if !state(&wd)? {
                break;
            }
        }
    }
    if !quiet {
        println!("booting the controller for the checks (it halts after provisioning)");
    }
    let ok = std::process::Command::new("vagrant")
        .args(["up", CONTROLLER, "--no-provision"])
        .current_dir(&wd)
        .stdout(if quiet { std::process::Stdio::null() } else { std::process::Stdio::inherit() })
        .status()?
        .success();
    if !ok {
        return Err("couldn't boot the controller (vagrant up isoloom-controller)".into());
    }
    Ok(Some(Rehalt(wd)))
}

/// Whether `vagrant status --machine-readable` says the controller runs.
fn controller_running(machine_readable: &str) -> bool {
    // `time,machine,state,<state>` lines.
    machine_readable.lines().any(|l| {
        let f: Vec<&str> = l.split(',').collect();
        f.len() >= 4 && f[1] == CONTROLLER && f[2] == "state" && f[3] == "running"
    })
}

fn kubectl_kustomize(dir: &str) -> Result<String, Box<dyn std::error::Error>> {
    let out = std::process::Command::new("kubectl")
        .args(["kustomize", "--load-restrictor", "LoadRestrictionsNone", dir])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "`kubectl` isn't installed".to_string()
            } else {
                e.to_string()
            }
        })?;
    if !out.status.success() {
        return Err(format!("kubectl kustomize failed: {}", String::from_utf8_lossy(&out.stderr).trim()).into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The rendered check manifests with `ISOLOOM_DERIVED=0` in every Job's containers, so the
/// runners skip the derived checks. Other documents (the ConfigMaps) pass through.
fn derived_off(yaml: &str) -> Result<String, Box<dyn std::error::Error>> {
    use serde::Deserialize;
    use serde_yaml_ng::Value;
    let mut docs = Vec::new();
    for doc in serde_yaml_ng::Deserializer::from_str(yaml) {
        let mut v = Value::deserialize(doc)?;
        if v.get("kind").and_then(Value::as_str) == Some("Job")
            && let Some(containers) = v
                .get_mut("spec")
                .and_then(|s| s.get_mut("template"))
                .and_then(|t| t.get_mut("spec"))
                .and_then(|s| s.get_mut("containers"))
                .and_then(Value::as_sequence_mut)
        {
            for c in containers.iter_mut() {
                let env: Value = serde_yaml_ng::from_str("name: ISOLOOM_DERIVED\nvalue: \"0\"\n")?;
                match c.get_mut("env").and_then(Value::as_sequence_mut) {
                    Some(list) => list.push(env),
                    None => {
                        if let Some(m) = c.as_mapping_mut() {
                            m.insert(Value::from("env"), Value::Sequence(vec![env]));
                        }
                    }
                }
            }
        }
        docs.push(serde_yaml_ng::to_string(&v)?);
    }
    Ok(docs.join("---\n"))
}

/// A check runner reached over SSH (the cloud targets).
fn ssh_runner(key: Option<&std::path::Path>, user: &str, host: &str, command: &str, no_derived: bool) -> Runner {
    let mut args = vec![
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "BatchMode=yes".to_string(),
    ];
    if let Some(k) = key {
        args.extend(["-i".to_string(), k.display().to_string()]);
    }
    args.push(format!("{user}@{host}"));
    let derived = if no_derived { "ISOLOOM_DERIVED=0 " } else { "" };
    args.push(format!("{derived}{command}"));
    Runner {
        label: format!("on {host}"),
        program: "ssh".to_string(),
        args,
        wd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        env: vec![],
    }
}

/// The resolved snapshot of a project (as an instance when asked), after validation.
fn snapshot_of(dir: &std::path::Path, instance: Option<u8>, sets: &[String]) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let (spec, _) = load_settings(dir, None, sets)?;
    let problems = core::validate(&spec);
    if !problems.is_empty() {
        for p in &problems {
            eprintln!("✗ {p}");
        }
        return Err("fix the spec first (`isoloom validate`)".into());
    }
    let spec = match instance {
        Some(n) => core::instance::apply(&spec, n)?,
        None => spec,
    };
    Ok(core::resolved::resolve_with(&spec, instance))
}

/// A project folder as an absolute path (the registry and the tools' working directories need one).
fn abs(dir: &std::path::Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    std::fs::canonicalize(dir).map_err(|e| format!("{}: {e}", dir.display()).into())
}

/// The command (and its working directory) that brings a target up or tears it down.
fn bring_up(
    dir: &std::path::Path,
    t: core::Target,
    cloud: Option<&str>,
    instance: Option<u8>,
    down: bool,
) -> Result<(String, Vec<String>, PathBuf), Box<dyn std::error::Error>> {
    let s = |x: &str| x.to_string();
    let out = dir.join(core::instance::output_dir(instance));
    Ok(match t {
        core::Target::Docker | core::Target::Hosted => {
            let f = out.join("docker/compose.yml");
            // Down with the `check` profile too, so the runners' stand-ins go as well.
            let args = if down {
                vec![s("compose"), s("-f"), f.display().to_string(), s("--profile"), s("check"), s("down"), s("-v")]
            } else {
                vec![s("compose"), s("-f"), f.display().to_string(), s("up"), s("-d"), s("--build"), s("--wait")]
            };
            (s("docker"), args, dir.to_path_buf())
        }
        core::Target::Vagrant | core::Target::DockerVm | core::Target::Hybrid => {
            let sub = match t {
                core::Target::Vagrant => "vagrant",
                core::Target::DockerVm => "docker-vm",
                _ => "hybrid",
            };
            let args = if down { vec![s("destroy"), s("-f")] } else { vec![s("up")] };
            (s("vagrant"), args, out.join(sub))
        }
        core::Target::Kubernetes => {
            // kustomize build, piped to kubectl; a shell keeps it one command.
            let kdir = out.join("kubernetes");
            let build = format!("kubectl kustomize --load-restrictor LoadRestrictionsNone {}", kdir.display());
            let verb = if down { "delete --ignore-not-found=true" } else { "apply" };
            (s("sh"), vec![s("-c"), format!("{build} | kubectl {verb} -f -")], dir.to_path_buf())
        }
        core::Target::Proxmox => tf_run(down, out.join("proxmox")),
        core::Target::CloudVm => {
            // AWS is always generated; the others only for the specs they support, so --cloud
            // picks the module and defaults to aws. A cloud with no module for this lab (its
            // driver declined the spec) has no directory, and Terraform says so.
            let cloud = cloud.unwrap_or("aws");
            let dir = out.join("cloud-vm").join(cloud);
            if !dir.exists() {
                return Err(format!(
                    "`cloud-vm` has no `{cloud}` module for this lab (either the cloud is unknown or its driver can't run this spec); run `isoloom generate` and check .isoloom/cloud-vm/"
                )
                .into());
            }
            tf_run(down, dir)
        }
        core::Target::CloudDocker => {
            let cloud = cloud.ok_or("`cloud-docker` needs --cloud (aws, azure, gcp, digitalocean, linode, oci)")?;
            tf_run(down, out.join("cloud-docker").join(cloud))
        }
        // Nothing to create or destroy: `run` provisions over SSH (see lifecycle::external_up),
        // `down` only forgets the environment.
        core::Target::External => (s("true"), vec![], dir.to_path_buf()),
    })
}

/// Terraform brings a module up with init then apply, and down with destroy. `run` shells out
/// once, so init+apply go through a short shell; the cloud's own credentials come from the
/// environment, as Terraform expects.
fn tf_run(down: bool, module: PathBuf) -> (String, Vec<String>, PathBuf) {
    let script = if down {
        "terraform init -input=false && terraform destroy -auto-approve".to_string()
    } else {
        "terraform init -input=false && terraform apply -auto-approve".to_string()
    };
    ("sh".to_string(), vec!["-c".to_string(), script], module)
}

#[cfg(test)]
mod tests {
    #[test]
    fn derived_off_reaches_every_job_and_leaves_the_rest() {
        let yaml = "apiVersion: batch/v1\nkind: Job\nmetadata:\n  name: a\nspec:\n  template:\n    spec:\n      containers:\n      - name: check\n        image: x\n---\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: m\ndata:\n  k: v\n";
        let out = super::derived_off(yaml).unwrap();
        assert_eq!(out.matches("ISOLOOM_DERIVED").count(), 1, "{out}");
        assert!(out.contains("value: '0'") || out.contains("value: \"0\""), "{out}");
        assert!(out.contains("kind: ConfigMap") && out.contains("k: v"), "{out}");
    }
}

#[cfg(test)]
mod controller_tests {
    #[test]
    fn the_controllers_state_comes_from_its_status_line() {
        let up = "1700000000,isoloom-controller,metadata,provider,libvirt\n1700000000,isoloom-controller,state,running\n";
        let down = "1700000000,isoloom-controller,state,shutoff\n1700000000,web,state,running\n";
        assert!(super::controller_running(up));
        assert!(!super::controller_running(down));
    }
}

#[cfg(test)]
mod provision_tests {
    fn example() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/ansible-pair")
    }

    #[test]
    fn provision_names_machines_of_the_spec() {
        let e = super::provision_cmd(&example(), Some("vagrant"), &["ghost".into()], None, &[]).unwrap_err();
        assert!(e.to_string().contains("no machine named `ghost`"), "{e}");
    }

    #[test]
    fn provision_runs_on_local_vms_for_now() {
        let e = super::provision_cmd(&example(), Some("proxmox"), &[], None, &[]).unwrap_err();
        assert!(e.to_string().contains("local VMs"), "{e}");
    }

    #[test]
    fn provision_needs_a_running_environment() {
        let e = super::provision_cmd(&example(), Some("vagrant"), &["web".into()], None, &[]).unwrap_err();
        assert!(e.to_string().contains("isoloom run vagrant"), "{e}");
    }
}
