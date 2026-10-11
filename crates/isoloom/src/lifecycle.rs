//! Around a running environment: `status` (what `run` brought up on this host, and whether it
//! still runs), `connect` (a shell on a machine), `exec` (a command on one or every machine)
//! and `capture` (packets on a machine's interface). Each talks to the target's own tool:
//! `docker compose exec`, `vagrant ssh`, `kubectl exec`, `ssh` with Terraform's outputs.

use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

use isoloom_core as core;
use isoloom_core::registry::{self, Entry};
use isoloom_core::{Target, shell};

type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// `isoloom status`: every environment in the registry, with its live state.
pub fn status(json: bool, cleanup: Option<&str>, ssh_key: Option<&Path>) -> Res<ExitCode> {
    let reg = registry::load()?;
    if let Some(name) = cleanup {
        let targets: Vec<Entry> = reg.environments.iter().filter(|e| e.name == name || e.dir.ends_with(name)).cloned().collect();
        if targets.is_empty() {
            return Err(format!("no environment named `{name}` in {}", registry::path().display()).into());
        }
        for e in &targets {
            if e.dir.join(core::instance::output_dir(e.instance)).is_dir() {
                let (program, args, wd) = super::bring_up(&e.dir, e.target, e.cloud.as_deref(), e.instance, true)?;
                eprintln!("Tearing down {} on {} ({})", e.name, e.target.id(), wd.display());
                let ok = Command::new(&program)
                    .args(&args)
                    .current_dir(&wd)
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if !ok {
                    eprintln!("✗ {program} failed; the entry is removed anyway (check for leftovers by hand)");
                }
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
        let _ = ssh_key;
        return Ok(ExitCode::SUCCESS);
    }

    let rows: Vec<(Entry, String)> = reg.environments.iter().map(|e| (e.clone(), probe(e))).collect();
    if json {
        let list: Vec<serde_json::Value> = rows
            .iter()
            .map(|(e, state)| {
                serde_json::json!({
                    "name": e.name, "dir": e.dir, "target": e.target.id(), "instance": e.instance, "cloud": e.cloud, "started": e.started, "state": state,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "registry": registry::path(), "environments": list }))?
        );
        return Ok(ExitCode::SUCCESS);
    }
    if rows.is_empty() {
        println!(
            "no environment is up on this host (none recorded in {}); `isoloom run <target>` starts one",
            registry::path().display()
        );
        return Ok(ExitCode::SUCCESS);
    }
    let w = |f: fn(&(Entry, String)) -> usize| rows.iter().map(f).max().unwrap_or(0);
    let (wn, wt, ws) = (w(|r| r.0.name.len()).max(4), w(|r| target_label(&r.0).len()).max(6), w(|r| r.1.len()).max(5));
    let head = ["NAME", "TARGET", "STATE", "STARTED", "FOLDER"];
    println!("{:<wn$}  {:<wt$}  {:<ws$}  {:<20}  {}", head[0], head[1], head[2], head[3], head[4]);
    for (e, state) in &rows {
        println!(
            "{:<wn$}  {:<wt$}  {:<ws$}  {:<20}  {}",
            e.name,
            target_label(e),
            state,
            e.started,
            e.dir.display()
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn target_label(e: &Entry) -> String {
    let mut label = match &e.cloud {
        Some(c) => format!("{} ({c})", e.target.id()),
        None => e.target.id().to_string(),
    };
    if let Some(n) = e.instance {
        label.push_str(&format!(" #{n}"));
    }
    label
}

/// The Compose project name the registry recorded for an environment on Docker: set when a
/// tool embedding Isoloom ran its Compose file under its own name.
pub fn compose_project(dir: &Path, instance: Option<u8>) -> Option<String> {
    let reg = registry::load().ok()?;
    reg.for_dir(dir)
        .into_iter()
        .find(|e| e.instance == instance && matches!(e.target, Target::Docker | Target::Hosted))
        .and_then(|e| e.project.clone())
}

/// `[-p <project>] -f <compose file>` for an environment's Compose file, under the project
/// name it runs as (see [`compose_project`]).
pub fn compose_files(dir: &Path, instance: Option<u8>) -> Vec<String> {
    let f = dir.join(core::instance::output_dir(instance)).join("docker/compose.yml").display().to_string();
    match compose_project(dir, instance) {
        Some(p) => vec!["-p".into(), p, "-f".into(), f],
        None => vec!["-f".into(), f],
    }
}

/// The host ports a local Docker environment really got, as (machine, port, host port): its
/// Compose file publishes on free loopback ports unless `ISOLOOM_PUBLISH_FIXED` is set, so the
/// spec's `publish:` values aren't where it answers. Empty when Compose can't say.
pub fn docker_published(dir: &Path, instance: Option<u8>) -> Vec<(String, u16, u16)> {
    let Ok(out) = Command::new("docker")
        .arg("compose")
        .args(compose_files(dir, instance))
        .args(["ps", "--format", "json"])
        .current_dir(dir)
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    // Compose prints one JSON object per line (older versions: one array).
    let items: Vec<serde_json::Value> =
        serde_json::from_str::<Vec<serde_json::Value>>(&text).unwrap_or_else(|_| text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect());
    published_of(&items)
}

/// (service, container port, host port) for each port `docker compose ps` lists as published.
fn published_of(items: &[serde_json::Value]) -> Vec<(String, u16, u16)> {
    let mut found = Vec::new();
    for i in items {
        let Some(service) = i["Service"].as_str() else { continue };
        for p in i["Publishers"].as_array().into_iter().flatten() {
            let port = p["TargetPort"].as_u64().and_then(|n| u16::try_from(n).ok());
            let host = p["PublishedPort"].as_u64().and_then(|n| u16::try_from(n).ok()).filter(|h| *h != 0);
            if let (Some(port), Some(host)) = (port, host)
                && !found.iter().any(|(s, c, _): &(String, u16, u16)| s == service && *c == port)
            {
                found.push((service.to_string(), port, host));
            }
        }
    }
    found
}

/// What the target's tool says about an environment: `running (3/3)`, `partly (1/3)`,
/// `stopped`, `applied (12 resources)`, or why it can't tell.
fn probe(e: &Entry) -> String {
    let out = e.dir.join(core::instance::output_dir(e.instance));
    if !out.is_dir() {
        return "stale (folder gone)".into();
    }
    // Bounded: a wedged hypervisor service must not hang `status`.
    let run = |program: &str, args: &[&str], wd: &Path| -> Result<String, String> {
        let cwd = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(wd);
        let out = core::host::run_limited(program, args, 30);
        if let Some(c) = cwd {
            let _ = std::env::set_current_dir(c);
        }
        out?.ok_or_else(|| format!("{program} failed"))
    };
    let counted = |running: usize, total: usize| match (running, total) {
        (_, 0) => "stopped".to_string(),
        (r, t) if r == t => format!("running ({r}/{t})"),
        (0, _) => "stopped".to_string(),
        (r, t) => format!("partly ({r}/{t})"),
    };
    let result = match e.target {
        Target::Docker | Target::Hosted => {
            let mut args = vec!["compose".to_string()];
            if let Some(p) = &e.project {
                args.extend(["-p".to_string(), p.clone()]);
            }
            args.extend([
                "-f".to_string(),
                out.join("docker/compose.yml").display().to_string(),
                "ps".into(),
                "--format".into(),
                "json".into(),
            ]);
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            run("docker", &args, &e.dir).map(|text| {
                // Compose prints one JSON object per line (older versions: one array).
                let items: Vec<serde_json::Value> = serde_json::from_str::<Vec<serde_json::Value>>(&text)
                    .unwrap_or_else(|_| text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect());
                let machines: Vec<&serde_json::Value> = items
                    .iter()
                    .filter(|i| {
                        i["Service"]
                            .as_str()
                            .is_some_and(|s| !s.starts_with("isoloom-") && !s.contains("-init-") && !s.ends_with("-routes"))
                    })
                    .collect();
                let running = machines.iter().filter(|i| i["State"].as_str() == Some("running")).count();
                counted(running, machines.len())
            })
        }
        Target::Vagrant | Target::DockerVm | Target::Hybrid => {
            let sub = match e.target {
                Target::Vagrant => "vagrant",
                Target::DockerVm => "docker-vm",
                _ => "hybrid",
            };
            run("vagrant", &["status", "--machine-readable"], &out.join(sub)).map(|text| {
                let states: Vec<&str> = text
                    .lines()
                    .filter_map(|l| {
                        let f: Vec<&str> = l.split(',').collect();
                        (f.len() >= 4 && f[2] == "state").then_some(f[3])
                    })
                    .collect();
                counted(states.iter().filter(|s| **s == "running").count(), states.len())
            })
        }
        Target::Kubernetes => {
            let ns = format!("isoloom-{}", e.name);
            run("kubectl", &["-n", &ns, "get", "deploy", "-o", "json"], &e.dir).map(|text| {
                let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                let items = v["items"].as_array().cloned().unwrap_or_default();
                let ready = items.iter().filter(|d| d["status"]["readyReplicas"].as_u64().unwrap_or(0) >= 1).count();
                counted(ready, items.len())
            })
        }
        Target::CloudServices => {
            // What the module's state holds: deployed resources, or nothing.
            let state = out.join("cloud-services/terraform.tfstate");
            let n = std::fs::read_to_string(&state)
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| v["resources"].as_array().map(Vec::len))
                .unwrap_or(0);
            Ok(if n > 0 {
                format!("deployed ({n} resources)")
            } else {
                "not deployed".to_string()
            })
        }
        Target::External => {
            // Each machine answers SSH, or not.
            let machines = external_machines(&out).unwrap_or_default();
            let total = machines.len();
            let up = machines
                .values()
                .filter(|m| {
                    let mut args = vec![
                        "-o".to_string(),
                        "BatchMode=yes".into(),
                        "-o".into(),
                        "ConnectTimeout=5".into(),
                        "-o".into(),
                        "StrictHostKeyChecking=accept-new".into(),
                        format!("-p{}", m.port),
                    ];
                    if let Some(k) = &m.key {
                        args.extend(["-i".to_string(), k.clone()]);
                    }
                    args.push(format!("{}@{}", m.user, m.address));
                    args.push("true".into());
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    core::host::run_limited("ssh", &refs, 10).ok().flatten().is_some()
                })
                .count();
            Ok(counted(up, total))
        }
        Target::Proxmox | Target::CloudVm | Target::CloudDocker => {
            let module = match e.target {
                Target::Proxmox => out.join("proxmox"),
                Target::CloudVm => out.join("cloud-vm").join(e.cloud.as_deref().unwrap_or("aws")),
                _ => out.join("cloud-docker").join(e.cloud.as_deref().unwrap_or("aws")),
            };
            run("terraform", &["state", "list"], &module).map(|text| {
                let n = text.lines().filter(|l| !l.trim().is_empty()).count();
                if n == 0 { "no state".to_string() } else { format!("applied ({n} resources)") }
            })
        }
    };
    result.unwrap_or_else(|why| format!("unknown ({why})"))
}

/// The target of a project to talk to: the one named; else what `run` recorded for the
/// project (when there is exactly one); else the spec's single possible target.
pub fn pick(dir: &Path, target: Option<&str>, instance: Option<u8>, spec: &core::Spec) -> Res<(Target, Option<String>)> {
    let reg = registry::load().unwrap_or_default();
    let recorded: Vec<&Entry> = reg.for_dir(dir).into_iter().filter(|e| e.instance == instance).collect();
    if let Some(id) = target {
        let t = Target::ALL.into_iter().find(|t| t.id() == id).ok_or_else(|| format!("unknown target `{id}`"))?;
        let cloud = recorded.iter().find(|e| e.target == t).and_then(|e| e.cloud.clone());
        return Ok((t, cloud));
    }
    match recorded.as_slice() {
        [one] => Ok((one.target, one.cloud.clone())),
        [] => match core::effective(spec).as_slice() {
            [one] => Ok((*one, None)),
            many => Err(format!(
                "nothing recorded as running here{} (`isoloom status`); pass the target: {}",
                instance.map(|n| format!(" as instance {n}")).unwrap_or_default(),
                many.iter().map(|t| format!("--target {}", t.id())).collect::<Vec<_>>().join(", ")
            )
            .into()),
        },
        several => Err(format!(
            "several targets are up here ({}); pass one with --target",
            several.iter().map(|e| e.target.id()).collect::<Vec<_>>().join(", ")
        )
        .into()),
    }
}

/// The login shell, or a command, in a machine's own shell.
fn shell(cmd: Option<&str>) -> String {
    match cmd {
        Some(c) => c.to_string(),
        None => "command -v bash >/dev/null 2>&1 && exec bash || exec sh".to_string(),
    }
}

/// A running environment to talk to.
pub struct Env<'a> {
    pub dir: &'a Path,
    pub spec: &'a core::Spec,
    pub target: Target,
    pub cloud: Option<&'a str>,
    pub instance: Option<u8>,
    pub ssh_key: Option<&'a Path>,
}

/// A command that runs `cmd` (or opens a shell) on `machine` of the environment, with a
/// terminal when `tty`. `sudo` wraps it for the VM targets when `root` is asked.
pub fn on_machine(env: &Env, machine: &str, cmd: Option<&str>, tty: bool, root: bool) -> Res<Command> {
    let Env {
        dir,
        spec,
        target,
        cloud,
        instance,
        ssh_key,
    } = *env;
    // A tool: its container (Docker) or VM (local VMs).
    if spec.tools.contains_key(machine) && !spec.machines.contains_key(machine) {
        let out = dir.join(core::instance::output_dir(instance));
        let unit = format!("isoloom-tool-{machine}");
        let inner = shell(cmd);
        let mut c;
        match target {
            Target::Docker | Target::Hosted => {
                c = Command::new("docker");
                c.args(["compose", "--progress", "quiet"]).args(compose_files(dir, instance)).arg("exec");
                c.arg(if tty { "-it" } else { "-T" });
                c.args([unit.as_str(), "sh", "-c", &inner]);
                c.current_dir(dir);
            }
            Target::Vagrant | Target::Hybrid if machine == "shell" => {
                let sub = if target == Target::Vagrant { "vagrant" } else { "hybrid" };
                c = Command::new("vagrant");
                c.current_dir(out.join(sub));
                c.args(["ssh", &unit]);
                if cmd.is_some() || root {
                    c.args(["-c", &format!("{}sh -c {}", if root { "sudo " } else { "" }, shell::quote(&inner))]);
                }
            }
            other => return Err(format!("tool `{machine}` has no form on {}", other.id()).into()),
        }
        return Ok(c);
    }
    let m = spec.machines.get(machine).ok_or_else(|| {
        format!(
            "no machine named `{machine}` (machines: {})",
            spec.machines.keys().cloned().collect::<Vec<_>>().join(", ")
        )
    })?;
    let out = dir.join(core::instance::output_dir(instance));
    let windows = m.vm.as_ref().is_some_and(|v| core::images::is_windows(&v.os));
    let inner = shell(cmd);
    let sudo = if root { "sudo " } else { "" };
    let container_only = |what: &str| -> Res<()> {
        if m.docker.is_none() {
            return Err(format!("`{machine}` has no `docker:`: on {what} it is supplied by the runner, so there is nothing of it to reach").into());
        }
        Ok(())
    };
    let mut c;
    match target {
        Target::Docker | Target::Hosted => {
            container_only("Docker")?;
            c = Command::new("docker");
            c.args(["compose", "--progress", "quiet"]).args(compose_files(dir, instance)).arg("exec");
            c.arg(if tty { "-it" } else { "-T" });
            c.args([machine, "sh", "-c", &inner]);
            c.current_dir(dir);
        }
        Target::Vagrant | Target::Hybrid => {
            let sub = if target == Target::Vagrant { "vagrant" } else { "hybrid" };
            c = Command::new("vagrant");
            c.current_dir(out.join(sub));
            if target == Target::Hybrid && m.docker.is_some() {
                // A container of a hybrid environment lives on the Docker host VM.
                let flags = if tty { "-it" } else { "-i" };
                c.args([
                    "ssh",
                    "isoloom-docker",
                    "-c",
                    &format!("sudo docker exec {flags} {machine} sh -c {}", shell::quote(&inner)),
                ]);
            } else if windows {
                return Err(windows_hint(spec, machine).into());
            } else {
                c.args(["ssh", machine]);
                if cmd.is_some() || root {
                    c.args(["-c", &format!("{sudo}sh -c {}", shell::quote(&inner))]);
                }
                if !tty {
                    c.arg("--");
                    c.arg("-T");
                }
            }
        }
        Target::DockerVm => {
            container_only("Docker")?;
            let flags = if tty { "-it" } else { "-T" };
            c = Command::new("vagrant");
            c.current_dir(out.join("docker-vm"));
            c.args([
                "ssh",
                "-c",
                &format!(
                    "cd /opt/isoloom && sudo docker compose -f {}/docker/compose.yml exec {flags} {machine} sh -c {}",
                    core::instance::output_dir(instance),
                    shell::quote(&inner)
                ),
            ]);
        }
        Target::Kubernetes => {
            container_only("Kubernetes")?;
            c = Command::new("kubectl");
            c.args(["-n", &format!("isoloom-{}", spec.name), "exec"]);
            if tty {
                c.arg("-it");
            }
            c.args([&format!("deploy/{machine}"), "--", "sh", "-c", &inner]);
            c.current_dir(dir);
        }
        Target::CloudVm => {
            if windows {
                return Err(windows_hint(spec, machine).into());
            }
            let module = out.join("cloud-vm").join(cloud.unwrap_or("aws"));
            let outputs = super::terraform_output(&module)?;
            let host = outputs["machines"][machine]
                .as_str()
                .ok_or_else(|| format!("no address for `{machine}` in the module's outputs; is it applied?"))?
                .to_string();
            let user = outputs["ssh_users"][machine].as_str().unwrap_or("isoloom").to_string();
            c = ssh(ssh_key, &user, &host, tty, None);
            c.arg(format!("{sudo}sh -c {}", shell::quote(&inner)));
        }
        Target::CloudDocker => {
            container_only("Docker")?;
            let cloud = cloud.ok_or("`cloud-docker` needs --cloud (aws, azure, gcp, digitalocean, linode, oci)")?;
            let outputs = super::terraform_output(&out.join("cloud-docker").join(cloud))?;
            let host = outputs["ip"].as_str().ok_or("the module has no `ip` output; is it applied?")?.to_string();
            let user = outputs["ssh_user"].as_str().unwrap_or("root").to_string();
            let flags = if tty { "-it" } else { "-T" };
            c = ssh(ssh_key, &user, &host, tty, None);
            c.arg(format!(
                "cd /opt/isoloom && sudo docker compose -f {}/docker/compose.yml exec {flags} {machine} sh -c {}",
                core::instance::output_dir(instance),
                shell::quote(&inner)
            ));
        }
        Target::CloudServices => {
            return Err("cloud services have no machines to reach: use their outputs (`isoloom message`)".into());
        }
        Target::External => {
            let e = m.external.as_ref().ok_or_else(|| format!("`{machine}` has no `external:` address"))?;
            let key = ssh_key.or(e.key.as_deref().map(Path::new));
            c = ssh(key, e.user.as_deref().unwrap_or("root"), &e.address, tty, None);
            if let Some(p) = e.port {
                c.arg(format!("-p{p}"));
            }
            c.arg(format!("{sudo}sh -c {}", shell::quote(&inner)));
        }
        Target::Proxmox => {
            // Through the router (the only VM on the uplink), as the `isoloom` user.
            let outputs = super::terraform_output(&out.join("proxmox"))?;
            let router = outputs["address"]
                .as_str()
                .ok_or("the module has no `address` output; is it applied?")?
                .to_string();
            let host = outputs["machines"][machine]
                .as_str()
                .ok_or_else(|| format!("no address for `{machine}` in the module's outputs; is it applied?"))?
                .to_string();
            let jump = format!("isoloom@{router}");
            c = ssh(ssh_key, "isoloom", &host, tty, Some(&jump));
            c.arg(format!("{sudo}sh -c {}", shell::quote(&inner)));
        }
    }
    Ok(c)
}

fn ssh(key: Option<&Path>, user: &str, host: &str, tty: bool, jump: Option<&str>) -> Command {
    let mut c = Command::new("ssh");
    c.args(["-o", "StrictHostKeyChecking=accept-new"]);
    if let Some(k) = key {
        c.arg("-i").arg(k);
    }
    if let Some(j) = jump {
        c.arg("-J").arg(j);
    }
    if tty {
        c.arg("-t");
    }
    c.arg(format!("{user}@{host}"));
    c
}

/// How to reach a Windows machine, which has no shell to drop into.
fn windows_hint(spec: &core::Spec, machine: &str) -> String {
    let r = core::resolved::resolve(spec);
    let addr = core::resolved::lookup(&r, &format!("machines.{machine}.addresses"))
        .and_then(|a| a.as_object())
        .and_then(|a| a.values().next())
        .and_then(|v| v.as_str())
        .unwrap_or("its address");
    format!(
        "`{machine}` runs Windows: reach it over RDP or WinRM at {addr} (local VMs: user vagrant, password vagrant; `vagrant rdp {machine}` once the RDP forward is enabled in the Vagrantfile)"
    )
}

/// `isoloom connect <machine>`: a shell on the machine.
pub fn connect(dir: &Path, target: Option<&str>, instance: Option<u8>, machine: &str, ssh_key: Option<&Path>) -> Res<ExitCode> {
    let spec = load_as(dir, instance)?;
    let (t, cloud) = pick(dir, target, instance, &spec)?;
    if let Some(clones) = spec.clones.get(machine) {
        return Err(format!("`{machine}` is {} machines; connect to one of them: {}", clones.len(), clones.join(", ")).into());
    }
    let env = Env {
        dir,
        spec: &spec,
        target: t,
        cloud: cloud.as_deref(),
        instance,
        ssh_key,
    };
    let mut c = on_machine(&env, machine, None, terminal(), false)?;
    let status = c.status().map_err(|e| tool_error(&c, e))?;
    Ok(if status.success() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// `isoloom exec <machine|all> -- <command>`: the command on one machine, or on every machine
/// that can be reached, each line prefixed with the machine's name.
pub fn exec(dir: &Path, target: Option<&str>, instance: Option<u8>, machine: &str, command: &[String], ssh_key: Option<&Path>) -> Res<ExitCode> {
    if command.is_empty() {
        return Err("give the command after `--`, e.g. `isoloom exec web -- uname -a`".into());
    }
    let spec = load_as(dir, instance)?;
    let (t, cloud) = pick(dir, target, instance, &spec)?;
    // Each word quoted again for the machine's shell, so `exec web -- sh -c 'a; b'` arrives intact.
    let cmd = command
        .iter()
        .map(|a| {
            if a.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,@%+".contains(c)) {
                a.clone()
            } else {
                shell::quote(a)
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let env = Env {
        dir,
        spec: &spec,
        target: t,
        cloud: cloud.as_deref(),
        instance,
        ssh_key,
    };
    if machine != "all" && !spec.groups.contains_key(machine) && !spec.clones.contains_key(machine) {
        let mut c = on_machine(&env, machine, Some(&cmd), false, false)?;
        let status = c.status().map_err(|e| tool_error(&c, e))?;
        return Ok(if status.success() { ExitCode::SUCCESS } else { ExitCode::FAILURE });
    }
    let mut failed = false;
    let targets: Vec<String> = if machine == "all" {
        spec.machines.keys().cloned().collect()
    } else if let Some(clones) = spec.clones.get(machine) {
        clones.clone()
    } else {
        core::groups::members(&spec, machine)
    };
    for name in &targets {
        let mut c = match on_machine(&env, name, Some(&cmd), false, false) {
            Ok(c) => c,
            Err(why) => {
                println!("{name}: skipped ({why})");
                continue;
            }
        };
        c.stdout(Stdio::piped()).stderr(Stdio::piped());
        match c.output() {
            Ok(o) => {
                for line in String::from_utf8_lossy(&o.stdout).lines().chain(String::from_utf8_lossy(&o.stderr).lines()) {
                    println!("{name}: {line}");
                }
                if !o.status.success() {
                    println!("{name}: exited with {}", o.status);
                    failed = true;
                }
            }
            Err(e) => return Err(tool_error(&c, e).into()),
        }
    }
    Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

/// `isoloom capture <machine> <network> [-- tcpdump args]`: tcpdump on the machine's interface
/// on that network, found by its address from inside the machine's network namespace (so it
/// works the same on Docker Desktop, Linux, local VMs and cloud VMs).
pub fn capture(dir: &Path, target: Option<&str>, instance: Option<u8>, machine: &str, network: &str, args: &[String], ssh_key: Option<&Path>) -> Res<ExitCode> {
    let spec = load_as(dir, instance)?;
    let (t, cloud) = pick(dir, target, instance, &spec)?;
    let m = spec.machines.get(machine).ok_or_else(|| format!("no machine named `{machine}`"))?;
    if !m.networks.contains_key(network) {
        return Err(format!(
            "`{machine}` isn't on `{network}` (its networks: {})",
            m.networks.keys().cloned().collect::<Vec<_>>().join(", ")
        )
        .into());
    }
    let r = core::resolved::resolve(&spec);
    let on_docker = matches!(t, Target::Docker | Target::Hosted | Target::DockerVm | Target::CloudDocker | Target::Kubernetes);
    let key = if on_docker { "docker_addresses" } else { "addresses" };
    let addr = core::resolved::lookup(&r, &format!("machines.{machine}.{key}.{network}"))
        .and_then(|v| v.as_str())
        .ok_or("no address")?
        .to_string();
    let extra = if args.is_empty() {
        "-l -v".to_string()
    } else {
        args.iter().map(|a| shell::quote(a)).collect::<Vec<_>>().join(" ")
    };
    let find = format!(
        "IF=$(ip -o -4 addr show | awk '$4 ~ /^{}\\//{{print $2}}' | head -n 1); [ -n \"$IF\" ] || {{ echo 'no interface with {addr} in this machine' >&2; exit 1; }}",
        addr.replace('.', "\\.")
    );
    let tcpdump = format!(
        "command -v tcpdump >/dev/null 2>&1 || {{ echo 'tcpdump is not installed in {machine} (apt-get install tcpdump)' >&2; exit 1; }}; {find}; exec tcpdump -i \"$IF\" {extra}"
    );
    let mut c = match t {
        Target::Docker | Target::Hosted => {
            // The machine's image rarely has tcpdump: a netshoot container in its network namespace.
            if m.docker.is_none() {
                return Err(format!("`{machine}` has no `docker:`: on Docker it is supplied by the runner").into());
            }
            let id = Command::new("docker")
                .arg("compose")
                .args(compose_files(dir, instance))
                .args(["ps", "-q", machine])
                .current_dir(dir)
                .output()
                .map_err(|e| format!("docker: {e}"))?;
            let id = String::from_utf8_lossy(&id.stdout).trim().to_string();
            if id.is_empty() {
                return Err(format!("`{machine}` isn't running (`isoloom status`)").into());
            }
            let mut c = Command::new("docker");
            // No terminal: tcpdump only writes, and Docker forwards Ctrl-C to it anyway.
            c.args([
                "run",
                "--rm",
                "--net",
                &format!("container:{id}"),
                "nicolaka/netshoot",
                "sh",
                "-c",
                &format!("{find}; exec tcpdump -i \"$IF\" {extra}"),
            ]);
            c.current_dir(dir);
            c
        }
        Target::Vagrant | Target::CloudVm => on_machine(
            &Env {
                dir,
                spec: &spec,
                target: t,
                cloud: cloud.as_deref(),
                instance,
                ssh_key,
            },
            machine,
            Some(&tcpdump),
            terminal(),
            true,
        )?,
        other => return Err(format!("capture on {} comes next (Docker, local VMs and cloud VMs today)", other.id()).into()),
    };
    let status = c.status().map_err(|e| tool_error(&c, e))?;
    Ok(if status.success() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// `isoloom tc show|set|disable|reset <network>`: netem on a running environment (Docker and
/// local VMs), wherever the spec's `tc` applies: the router's interface into the network and
/// each machine's own interface on it.
pub fn tc(dir: &Path, target: Option<&str>, instance: Option<u8>, action: &str, network: &str, wanted: core::Tc) -> Res<ExitCode> {
    let spec = load_as(dir, instance)?;
    let (t, _) = pick(dir, target, instance, &spec)?;
    let net = spec.networks.get(network).ok_or_else(|| format!("no network named `{network}`"))?;
    if let Some(gw) = &net.gateway {
        return Err(format!("`{network}` is routed by its gateway machine `{gw}`, not by Isoloom's router").into());
    }
    let docker = matches!(t, Target::Docker | Target::Hosted);
    if !docker && !matches!(t, Target::Vagrant | Target::Hybrid) {
        return Err(format!("`tc` on {} comes next (Docker and local VMs today)", t.id()).into());
    }
    let r = core::resolved::resolve_with(&spec, instance);
    // Where: (who, the address its interface on the network has, how to run a command there).
    let mut at: Vec<(String, String, String)> = Vec::new();
    if let Some(a) = core::resolved::lookup(&r, &format!("networks.{network}.router")).and_then(|v| v.as_str()) {
        at.push((
            core::generate::router_name().to_string(),
            a.to_string(),
            core::generate::router_name().to_string(),
        ));
    }
    for (name, m) in &spec.machines {
        let windows = m.docker.is_none() && m.vm.as_ref().is_some_and(|v| v.os.starts_with("windows"));
        if !m.networks.contains_key(network) || windows {
            continue;
        }
        let key = if docker { "docker_addresses" } else { "addresses" };
        let Some(a) = core::resolved::lookup(&r, &format!("machines.{name}.{key}.{network}")).and_then(|v| v.as_str()) else {
            continue;
        };
        // On Docker, the machine's network sidecar has `tc` and the right to use it.
        let host = if docker { format!("{name}-routes") } else { name.clone() };
        if docker && m.docker.is_none() {
            continue;
        }
        at.push((name.clone(), a.to_string(), host));
    }
    if at.is_empty() {
        return Err(format!("nothing on `{network}` to impair").into());
    }
    let out = dir.join(core::instance::output_dir(instance));
    let mut ok = true;
    for (who, addr, host) in at {
        let find = format!(
            "IF=$(ip -o -4 addr show | awk '$4 ~ /^{}\\//{{print $2}}' | head -n 1); [ -n \"$IF\" ] || {{ echo 'no interface on {network}' >&2; exit 1; }}",
            addr.replace('.', "\\.")
        );
        let command = match action {
            "show" => format!("{find}; echo \"{who}:\"; tc qdisc show dev \"$IF\""),
            "disable" => format!("{find}; tc qdisc del dev \"$IF\" root 2>/dev/null; echo \"{who} on {network}: impairment off\""),
            "set" => {
                let netem = wanted.netem();
                if netem.is_empty() {
                    return Err("`set` takes --delay, --jitter, --loss and/or --rate".into());
                }
                format!("{find}; tc qdisc replace dev \"$IF\" root netem {netem} && echo \"{who} on {network}: {netem}\"")
            }
            "reset" => match &net.tc {
                Some(tc) => format!(
                    "{find}; tc qdisc replace dev \"$IF\" root netem {0} && echo \"{who} on {network}: {0} (the spec's)\"",
                    tc.netem()
                ),
                None => format!("{find}; tc qdisc del dev \"$IF\" root 2>/dev/null; echo \"{who} on {network}: the spec sets no impairment; off\""),
            },
            other => return Err(format!("unknown action `{other}`: show, set, disable or reset").into()),
        };
        let mut c = if docker {
            let mut c = Command::new("docker");
            c.args(["compose", "--progress", "quiet"])
                .args(compose_files(dir, instance))
                .args(["exec", "-T", &host, "sh", "-c", &command]);
            c.current_dir(dir);
            c
        } else {
            let sub = if t == Target::Vagrant { "vagrant" } else { "hybrid" };
            let mut c = Command::new("vagrant");
            c.current_dir(out.join(sub));
            c.args(["ssh", &host, "-c", &format!("sudo sh -c {}", shell::quote(&command))]);
            c
        };
        ok &= c.status().map_err(|e| tool_error(&c, e))?.success();
    }
    Ok(if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// An external machine's SSH endpoint, as `.isoloom/external/machines.json` records it.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExternalMachine {
    pub address: String,
    pub user: String,
    pub port: u16,
    pub key: Option<String>,
}

/// The external machines of a generated project (`<out>/external/machines.json`).
pub fn external_machines(out: &Path) -> Res<std::collections::BTreeMap<String, ExternalMachine>> {
    let path = out.join("external/machines.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e} (run `isoloom generate --target external`)", path.display()))?;
    Ok(serde_json::from_str(&text)?)
}

/// `isoloom run external`: provisions the existing machines over SSH, in start order: the
/// project copied to /opt/isoloom, each machine's `.sh` steps run there, its `.yml` steps run
/// from here against it, then the environment's `provision:` playbooks with the inventory.
pub fn external_up(dir: &Path, spec: &core::Spec, instance: Option<u8>) -> Res<()> {
    let out = dir.join(core::instance::output_dir(instance));
    let machines = external_machines(&out)?;
    let ssh_args = |m: &ExternalMachine| -> Vec<String> {
        let mut a = vec!["-o".to_string(), "StrictHostKeyChecking=accept-new".into(), format!("-p{}", m.port)];
        if let Some(k) = &m.key {
            a.extend(["-i".to_string(), k.clone()]);
        }
        a.push(format!("{}@{}", m.user, m.address));
        a
    };
    let run = |program: &str, args: &[String], stdin: Option<std::process::Stdio>| -> Res<()> {
        let mut c = Command::new(program);
        c.args(args);
        if let Some(i) = stdin {
            c.stdin(i);
        }
        let st = c.status().map_err(|e| tool_error(&c, e))?;
        if st.success() {
            Ok(())
        } else {
            Err(format!("{program} {} failed", args.iter().take(3).cloned().collect::<Vec<_>>().join(" ")).into())
        }
    };
    let order: Vec<String> = core::resolved::lookup(&core::resolved::resolve_with(spec, instance), "start_order")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    for name in &order {
        let Some(m) = machines.get(name) else { continue };
        let machine = &spec.machines[name];
        let Some(vm) = &machine.vm else { continue };
        if core::images::is_windows(&vm.os) {
            eprintln!("{name}: Windows; provision it with the environment's playbooks (per-machine steps over SSH come later)");
            continue;
        }
        eprintln!("{name}: copying the project to {}:/opt/isoloom", m.address);
        let tar = Command::new("tar")
            .args([
                "-czf",
                "-",
                "--exclude=.git",
                "--exclude=.vagrant",
                "--exclude=.terraform",
                "-C",
                &dir.display().to_string(),
                ".",
            ])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("tar: {e}"))?;
        let mut a = ssh_args(m);
        a.push("sudo mkdir -p /opt/isoloom && sudo chown \"$(id -un)\" /opt/isoloom && tar -xzf - -C /opt/isoloom".into());
        run("ssh", &a, Some(std::process::Stdio::from(tar.stdout.expect("piped"))))?;
        for step in &vm.provision {
            eprintln!("{name}: {step}");
            if step.ends_with(".sh") {
                let mut a = ssh_args(m);
                a.push(format!("cd /opt/isoloom && sudo sh {step}"));
                run("ssh", &a, None)?;
            } else {
                let mut a = vec!["-i".to_string(), format!("{},", m.address), "-u".into(), m.user.clone(), "--become".into()];
                if let Some(k) = &m.key {
                    a.extend(["--private-key".to_string(), k.clone()]);
                }
                a.extend(["-e".to_string(), format!("ansible_port={}", m.port), step.clone()]);
                run("ansible-playbook", &a, None)?;
            }
        }
    }
    for step in &spec.provision {
        eprintln!("environment: {}", step.ansible);
        let mut a = vec!["-i".to_string(), out.join("external/inventory.ini").display().to_string()];
        for inv in &step.inventory {
            a.extend(["-i".to_string(), dir.join(inv).display().to_string()]);
        }
        for (k, v) in &step.vars {
            a.extend(["-e".to_string(), format!("{k}={v}")]);
        }
        a.push(dir.join(&step.ansible).display().to_string());
        run("ansible-playbook", &a, None)?;
    }
    Ok(())
}

/// The spec, as the instance when one is named (its names and ports are the instance's).
fn load_as(dir: &Path, instance: Option<u8>) -> Res<core::Spec> {
    let spec = core::load(dir)?;
    Ok(match instance {
        Some(n) => core::instance::apply(&spec, n)?,
        None => spec,
    })
}

/// Whether a terminal is attached (so `-t` can be asked of the tools).
fn terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

fn tool_error(c: &Command, e: std::io::Error) -> String {
    let program = c.get_program().to_string_lossy();
    if e.kind() == std::io::ErrorKind::NotFound {
        format!("`{program}` isn't installed")
    } else {
        format!("{program}: {e}")
    }
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

/// See `isoloom gc`.
pub fn gc(remove: bool, images: bool) -> Res<ExitCode> {
    let managed = format!("label={}=true", core::generate::MANAGED_LABEL);
    let env_label = core::generate::ENVIRONMENT_LABEL;
    // Compose projects that still have a container, in any state: never collected (a parked
    // environment keeps its volumes, with the user's progress in them).
    let alive: std::collections::BTreeSet<String> = docker_lines(&["ps", "-a", "--format", "{{.Label \"com.docker.compose.project\"}}"])
        .into_iter()
        .collect();
    let orphans = |kind: &str| -> Vec<(String, String)> {
        docker_lines(&[
            kind,
            "ls",
            "--filter",
            &managed,
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
    };
    let networks = orphans("network");
    let volumes = orphans("volume");
    let mut unused_images: Vec<(String, String)> = Vec::new();
    if images {
        let used: std::collections::BTreeSet<String> = {
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
        unused_images = docker_lines(&["images", "--no-trunc", "--filter", &managed, "--format", "{{.ID}}\t{{.Repository}}:{{.Tag}}"])
            .into_iter()
            .filter_map(|l| {
                let (id, name) = l.split_once('\t')?;
                (!used.contains(id)).then(|| (id.to_string(), name.to_string()))
            })
            .collect();
    }
    if networks.is_empty() && volumes.is_empty() && unused_images.is_empty() {
        println!("nothing to collect");
        return Ok(ExitCode::SUCCESS);
    }
    for (kind, list) in [("network", &networks), ("volume", &volumes)] {
        for (name, env) in list {
            println!("{kind} {name} ({env})");
        }
    }
    for (_, what) in &unused_images {
        println!("image {what}");
    }
    if !remove {
        println!(
            "\n{} left over; `isoloom gc --yes{}` removes them",
            networks.len() + volumes.len() + unused_images.len(),
            if images { " --images" } else { "" }
        );
        return Ok(ExitCode::SUCCESS);
    }
    let mut failed = 0;
    let mut rm = |args: &[&str]| {
        if !Command::new("docker").args(args).stdout(Stdio::null()).status().is_ok_and(|s| s.success()) {
            failed += 1;
        }
    };
    for (n, _) in &networks {
        rm(&["network", "rm", n]);
    }
    for (n, _) in &volumes {
        rm(&["volume", "rm", n]);
    }
    for (id, _) in &unused_images {
        rm(&["image", "rm", id]);
    }
    if failed > 0 {
        eprintln!("✗ {failed} could not be removed (in use?)");
        return Ok(ExitCode::from(1));
    }
    println!("removed");
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_ports_come_from_compose_ps() {
        let items: Vec<serde_json::Value> = serde_json::from_str(
            r#"[{"Service":"web","Publishers":[{"URL":"127.0.0.1","TargetPort":80,"PublishedPort":32772,"Protocol":"tcp"},{"URL":"::1","TargetPort":80,"PublishedPort":32772,"Protocol":"tcp"}]},
                {"Service":"cache","Publishers":[{"URL":"","TargetPort":6379,"PublishedPort":0,"Protocol":"tcp"}]},
                {"Service":"isoloom-check"}]"#,
        )
        .unwrap();
        assert_eq!(published_of(&items), [("web".to_string(), 80, 32772)]);
    }
}
