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
        /// Your own image table (YAML): OS images, and the access machine when the spec leaves
        /// it to the runner. See https://www.isoloom.com/en/docs/images
        #[arg(long, value_name = "FILE")]
        images: Option<PathBuf>,
    },
    /// Fail when the generated files under .isoloom/ don't match the spec (for CI).
    Check {
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The image table the files were generated with (see `generate --images`).
        #[arg(long, value_name = "FILE")]
        images: Option<PathBuf>,
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
    },
    /// Tear down what `run` started for a target (the inverse tool: compose down, vagrant
    /// destroy, kubectl delete, or terraform destroy).
    Down {
        target: Option<String>,
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long)]
        cloud: Option<String>,
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
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
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
        Command::Generate { dir, target, images } => {
            let spec = core::load(&dir)?;
            let problems = core::validate(&spec);
            if !problems.is_empty() {
                for p in &problems {
                    eprintln!("✗ {p}");
                }
                eprintln!("fix the spec first (`isoloom validate`)");
                return Ok(ExitCode::FAILURE);
            }
            let spec = with_images(spec, images.as_deref())?;
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
        Command::Run { target, dir, cloud, images } => run_cmd(&dir, target.as_deref(), cloud.as_deref(), images.as_deref(), false),
        Command::Down { target, dir, cloud } => run_cmd(&dir, target.as_deref(), cloud.as_deref(), None, true),
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
        Command::Check { dir, images } => {
            let spec = core::load(&dir)?;
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
            let spec = with_images(spec, images.as_deref())?;
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

/// `isoloom run`/`down`: pick the target, generate its files, then bring it up (or tear it down)
/// with the tool that owns that output. `down` is the same dispatch with the inverse command.
fn run_cmd(
    dir: &std::path::Path,
    target: Option<&str>,
    cloud: Option<&str>,
    images: Option<&std::path::Path>,
    down: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let spec = core::load(dir)?;
    // Absolute, so the paths we hand to docker/vagrant/terraform don't depend on their working
    // directory (compose runs from `dir`, terraform and vagrant from the module folder).
    let dir = &std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let problems = core::validate(&spec);
    if !problems.is_empty() {
        for p in &problems {
            eprintln!("✗ {p}");
        }
        return Err("fix the spec first (`isoloom validate`)".into());
    }
    let spec = with_images(spec, images)?;

    // Pick the target: the one named, or the only possibility, else ask.
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

    // Generate just this target's files (so a run is always against the current spec).
    let files = core::generate(&spec, t).map_err(|e| format!("can't generate `{}`: {e}", t.id()))?;
    for f in &files {
        let path = dir.join(&f.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &f.contents)?;
    }

    let (program, args, wd) = bring_up(dir, t, cloud, down)?;
    eprintln!("{} {} ({})", if down { "Tearing down" } else { "Running" }, t.id(), wd.display());
    let status = std::process::Command::new(&program).args(&args).current_dir(&wd).status();
    match status {
        Ok(s) if s.success() => Ok(ExitCode::SUCCESS),
        Ok(s) => Err(format!("{program} exited with {s}").into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!("`{program}` isn't installed").into()),
        Err(e) => Err(Box::new(e)),
    }
}

/// The command (and its working directory) that brings a target up or tears it down.
fn bring_up(dir: &std::path::Path, t: core::Target, cloud: Option<&str>, down: bool) -> Result<(String, Vec<String>, PathBuf), Box<dyn std::error::Error>> {
    let s = |x: &str| x.to_string();
    let out = dir.join(core::OUTPUT_DIR);
    Ok(match t {
        core::Target::Docker | core::Target::Hosted => {
            let f = out.join("docker/compose.yml");
            let args = if down {
                vec![s("compose"), s("-f"), f.display().to_string(), s("down"), s("-v")]
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
        core::Target::CloudVm => tf_run(down, out.join("cloud-vm/aws")),
        core::Target::CloudDocker => {
            let cloud = cloud.ok_or("`cloud-docker` needs --cloud (aws, azure, gcp, digitalocean, linode, oci)")?;
            tf_run(down, out.join("cloud-docker").join(cloud))
        }
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

/// The spec with the user's image table applied, when one is given.
fn with_images(spec: core::Spec, file: Option<&std::path::Path>) -> Result<core::Spec, Box<dyn std::error::Error>> {
    let Some(file) = file else { return Ok(spec) };
    let text = std::fs::read_to_string(file).map_err(|e| format!("can't read {}: {e}", file.display()))?;
    let table = core::images::Table::parse(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(table.apply(&spec))
}
