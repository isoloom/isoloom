//! Around a running environment: `status` (what `run` brought up on this host, and whether it
//! still runs), `connect` (a shell on a machine), `exec` (a command on one or every machine)
//! and `capture` (packets on a machine's interface). Each talks to the target's own tool:
//! `docker compose exec`, `vagrant ssh`, `kubectl exec`, `ssh` with Terraform's outputs.

use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

use isoloom_core as core;
use isoloom_core::registry::{self, Entry};
use isoloom_core::{Target, checks::sq};

type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// `isoloom status`: every environment in the registry, with its live state.
pub fn status(json: bool, cleanup: Option<&str>, ssh_key: Option<&Path>) -> Res<ExitCode> {
    let mut reg = registry::load()?;
    if let Some(name) = cleanup {
        let targets: Vec<Entry> = reg.environments.iter().filter(|e| e.name == name || e.dir.ends_with(name)).cloned().collect();
        if targets.is_empty() {
            return Err(format!("no environment named `{name}` in {}", registry::path().display()).into());
        }
        for e in targets {
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
            reg.remove(&e.dir, e.target, e.instance);
        }
        registry::save(&reg)?;
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

/// What the target's tool says about an environment: `running (3/3)`, `partly (1/3)`,
/// `stopped`, `applied (12 resources)`, or why it can't tell.
fn probe(e: &Entry) -> String {
    let out = e.dir.join(core::instance::output_dir(e.instance));
    if !out.is_dir() {
        return "stale (folder gone)".into();
    }
    let run = |program: &str, args: &[&str], wd: &Path| -> Result<String, String> {
        match Command::new(program).args(args).current_dir(wd).stderr(Stdio::null()).output() {
            Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).to_string()),
            Ok(_) => Err(format!("{program} failed")),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(format!("{program} not installed")),
            Err(err) => Err(err.to_string()),
        }
    };
    let counted = |running: usize, total: usize| match (running, total) {
        (_, 0) => "stopped".to_string(),
        (r, t) if r == t => format!("running ({r}/{t})"),
        (0, _) => "stopped".to_string(),
        (r, t) => format!("partly ({r}/{t})"),
    };
    let result = match e.target {
        Target::Docker | Target::Hosted => {
            let f = out.join("docker/compose.yml").display().to_string();
            run("docker", &["compose", "-f", &f, "ps", "--format", "json"], &e.dir).map(|text| {
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
            c.args([
                "compose",
                "--progress",
                "quiet",
                "-f",
                &out.join("docker/compose.yml").display().to_string(),
                "exec",
            ]);
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
                    &format!("sudo docker exec {flags} {machine} sh -c {}", sq(&inner)),
                ]);
            } else if windows {
                return Err(windows_hint(spec, machine).into());
            } else {
                c.args(["ssh", machine]);
                if cmd.is_some() || root {
                    c.args(["-c", &format!("{sudo}sh -c {}", sq(&inner))]);
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
                    sq(&inner)
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
            c = ssh(ssh_key, &user, &host, tty);
            c.arg(format!("{sudo}sh -c {}", sq(&inner)));
        }
        Target::CloudDocker => {
            container_only("Docker")?;
            let cloud = cloud.ok_or("`cloud-docker` needs --cloud (aws, azure, gcp, digitalocean, linode, oci)")?;
            let outputs = super::terraform_output(&out.join("cloud-docker").join(cloud))?;
            let host = outputs["ip"].as_str().ok_or("the module has no `ip` output; is it applied?")?.to_string();
            let user = outputs["ssh_user"].as_str().unwrap_or("root").to_string();
            let flags = if tty { "-it" } else { "-T" };
            c = ssh(ssh_key, &user, &host, tty);
            c.arg(format!(
                "cd /opt/isoloom && sudo docker compose -f {}/docker/compose.yml exec {flags} {machine} sh -c {}",
                core::instance::output_dir(instance),
                sq(&inner)
            ));
        }
        Target::Proxmox => return Err("reaching machines on Proxmox comes next".into()),
    }
    Ok(c)
}

fn ssh(key: Option<&Path>, user: &str, host: &str, tty: bool) -> Command {
    let mut c = Command::new("ssh");
    c.args(["-o", "StrictHostKeyChecking=accept-new"]);
    if let Some(k) = key {
        c.arg("-i").arg(k);
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
                sq(a)
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
    if machine != "all" && !spec.groups.contains_key(machine) {
        let mut c = on_machine(&env, machine, Some(&cmd), false, false)?;
        let status = c.status().map_err(|e| tool_error(&c, e))?;
        return Ok(if status.success() { ExitCode::SUCCESS } else { ExitCode::FAILURE });
    }
    let mut failed = false;
    let targets: Vec<String> = if machine == "all" {
        spec.machines.keys().cloned().collect()
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
        args.iter().map(|a| sq(a)).collect::<Vec<_>>().join(" ")
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
            let f = dir.join(core::instance::output_dir(instance)).join("docker/compose.yml").display().to_string();
            let id = Command::new("docker")
                .args(["compose", "-f", &f, "ps", "-q", machine])
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
