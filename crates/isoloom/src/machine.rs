//! A command on a machine of a running environment, through the target's own tool:
//! `docker compose exec`, `vagrant ssh`, `kubectl exec`, `ssh` with Terraform's outputs.
//! `connect`, `exec` and `capture` build on it.

use std::path::Path;
use std::process::Command;

use isoloom_spec::{Target, shell};

use isoloom_runtime::events::ErrorCode;

use crate::failure::{Failure, fail};
use crate::tools::{compose, kubectl, terraform};
use crate::{Res, spec, target};

/// A running environment to talk to.
pub struct Env<'a> {
    pub dir: &'a Path,
    pub spec: &'a isoloom_spec::Spec,
    pub target: Target,
    pub cloud: Option<&'a str>,
    pub instance: Option<u8>,
    pub ssh_key: Option<&'a Path>,
}

/// The environment `connect`, `exec` and `capture` reach: the spec as the instance and the
/// target it runs on (named, or what `run` recorded).
pub struct Running {
    pub spec: isoloom_spec::Spec,
    pub target: Target,
    pub cloud: Option<String>,
}

impl Running {
    /// The project's running environment on the target named, or the one `run` recorded.
    pub fn find(dir: &Path, target: Option<&str>, instance: Option<u8>) -> Res<Self> {
        let spec = spec::load_as(dir, instance)?;
        let (target, cloud) = target::pick(dir, target, instance, &spec)?;
        Ok(Self { spec, target, cloud })
    }

    /// The environment to run commands in, with the SSH key for the cloud targets.
    pub fn env<'a>(&'a self, dir: &'a Path, instance: Option<u8>, ssh_key: Option<&'a Path>) -> Env<'a> {
        Env {
            dir,
            spec: &self.spec,
            target: self.target,
            cloud: self.cloud.as_deref(),
            instance,
            ssh_key,
        }
    }
}

/// One command asked of a machine: what to run in its shell and how.
struct Call<'a> {
    env: &'a Env<'a>,
    machine: &'a str,
    /// The command, or the login shell, for the machine's `sh -c`.
    inner: String,
    /// A command was given (not only a shell to open).
    command: bool,
    tty: bool,
    root: bool,
}

impl Call<'_> {
    /// The output folder of the environment.
    fn out(&self) -> std::path::PathBuf {
        self.env.dir.join(isoloom_spec::instance::output_dir(self.env.instance))
    }

    /// `sudo ` when the command runs as root.
    fn sudo(&self) -> &'static str {
        if self.root { "sudo " } else { "" }
    }

    /// `sh -c <inner>` as one shell word list, with `sudo` when asked.
    fn sh_c(&self) -> String {
        format!("{}sh -c {}", self.sudo(), shell::quote(&self.inner))
    }

    /// The flags of `docker compose exec`: a terminal, or none.
    fn compose_flags(&self) -> &'static str {
        if self.tty { "-it" } else { "-T" }
    }
}

/// A command that runs `cmd` (or opens a shell) on `machine` of the environment, with a
/// terminal when `tty`. `sudo` wraps it for the VM targets when `root` is asked.
pub fn on_machine(env: &Env, machine: &str, cmd: Option<&str>, tty: bool, root: bool) -> Res<Command> {
    let call = Call {
        env,
        machine,
        inner: shell_of(cmd),
        command: cmd.is_some(),
        tty,
        root,
    };
    let spec = env.spec;
    // A tool: its container (Docker) or VM (local VMs).
    if spec.tools.contains_key(machine) && !spec.machines.contains_key(machine) {
        return tool(&call);
    }
    let m = spec.machines.get(machine).ok_or_else(|| {
        let message = format!(
            "no machine named `{machine}` (machines: {})",
            spec.machines.keys().cloned().collect::<Vec<_>>().join(", ")
        );
        Failure::new(ErrorCode::MachineUnknown, message).machine(machine)
    })?;
    match env.target {
        Target::Docker | Target::Hosted => docker(&call, m),
        Target::Vagrant | Target::Hybrid => local_vm(&call, m),
        Target::DockerVm => docker_vm(&call, m),
        Target::Kubernetes => kubernetes(&call, m),
        Target::CloudVm => cloud_vm(&call, m),
        Target::CloudDocker => cloud_docker(&call, m),
        Target::CloudServices => Err(fail(
            ErrorCode::Unsupported,
            "cloud services have no machines to reach: use their outputs (`isoloom message`)",
        )),
        Target::External => external(&call, m),
        Target::Proxmox | Target::Esxi => behind_router(&call, env.target.id()),
    }
}

/// The login shell, or a command, in a machine's own shell.
fn shell_of(cmd: Option<&str>) -> String {
    match cmd {
        Some(c) => c.to_string(),
        None => "command -v bash >/dev/null 2>&1 && exec bash || exec sh".to_string(),
    }
}

/// A tool of the spec: its container on Docker, its VM on the local VMs.
fn tool(call: &Call) -> Res<Command> {
    let Env { dir, target, instance, .. } = *call.env;
    let unit = format!("isoloom-tool-{}", call.machine);
    match target {
        Target::Docker | Target::Hosted => {
            let mut c = compose::quiet(dir, instance);
            c.arg("exec").arg(call.compose_flags());
            c.args([unit.as_str(), "sh", "-c", &call.inner]);
            Ok(c)
        }
        Target::Vagrant | Target::Hybrid if call.machine == "shell" => {
            let mut c = crate::tools::process::command("vagrant");
            c.current_dir(call.out().join(target::vm_folder(target)));
            c.args(["ssh", &unit]);
            if call.command || call.root {
                c.args(["-c", &call.sh_c()]);
            }
            Ok(c)
        }
        other => Err(fail(ErrorCode::Unsupported, format!("tool `{}` has no form on {}", call.machine, other.id()))),
    }
}

/// A machine without `docker:` is supplied by the runner on the container targets.
fn container_only(call: &Call, m: &isoloom_spec::Machine, what: &str) -> Res<()> {
    if m.docker.is_none() {
        let message = format!(
            "`{}` has no `docker:`: on {what} it is supplied by the runner, so there is nothing of it to reach",
            call.machine
        );
        return Err(Failure::new(ErrorCode::Unsupported, message).machine(call.machine).into());
    }
    Ok(())
}

/// Whether the machine runs Windows, which has no shell to drop into.
fn is_windows(m: &isoloom_spec::Machine) -> bool {
    m.vm.as_ref().is_some_and(|v| isoloom_spec::images::is_windows(&v.os))
}

fn docker(call: &Call, m: &isoloom_spec::Machine) -> Res<Command> {
    container_only(call, m, "Docker")?;
    let mut c = compose::quiet(call.env.dir, call.env.instance);
    c.arg("exec").arg(call.compose_flags());
    c.args([call.machine, "sh", "-c", &call.inner]);
    Ok(c)
}

fn local_vm(call: &Call, m: &isoloom_spec::Machine) -> Res<Command> {
    let Env { spec, target, .. } = *call.env;
    let machine = call.machine;
    let mut c = crate::tools::process::command("vagrant");
    c.current_dir(call.out().join(target::vm_folder(target)));
    if target == Target::Hybrid && m.docker.is_some() {
        // A container of a hybrid environment lives on the Docker host VM.
        let flags = if call.tty { "-it" } else { "-i" };
        c.args([
            "ssh",
            "isoloom-docker",
            "-c",
            &format!("sudo docker exec {flags} {machine} sh -c {}", shell::quote(&call.inner)),
        ]);
    } else if is_windows(m) {
        return Err(Failure::new(ErrorCode::Unsupported, windows_hint(spec, machine)).machine(machine).into());
    } else {
        c.args(["ssh", machine]);
        if call.command || call.root {
            c.args(["-c", &call.sh_c()]);
        }
        if !call.tty {
            c.arg("--");
            c.arg("-T");
        }
    }
    Ok(c)
}

/// `docker compose exec` on the VM (or cloud host) that runs the Compose file.
fn compose_on_host(call: &Call) -> String {
    format!(
        "cd /opt/isoloom && sudo docker compose -f {}/docker/compose.yml exec {} {} sh -c {}",
        isoloom_spec::instance::output_dir(call.env.instance),
        call.compose_flags(),
        call.machine,
        shell::quote(&call.inner)
    )
}

fn docker_vm(call: &Call, m: &isoloom_spec::Machine) -> Res<Command> {
    container_only(call, m, "Docker")?;
    let mut c = crate::tools::process::command("vagrant");
    c.current_dir(call.out().join("docker-vm"));
    c.args(["ssh", "-c", &compose_on_host(call)]);
    Ok(c)
}

fn kubernetes(call: &Call, m: &isoloom_spec::Machine) -> Res<Command> {
    container_only(call, m, "Kubernetes")?;
    let mut c = crate::tools::process::command("kubectl");
    c.args(["-n", &kubectl::namespace(&call.env.spec.name), "exec"]);
    if call.tty {
        c.arg("-it");
    }
    c.args([&format!("deploy/{}", call.machine), "--", "sh", "-c", &call.inner]);
    c.current_dir(call.env.dir);
    Ok(c)
}

/// The machine's address in a module's `machines` output.
fn address_in(outputs: &serde_json::Value, machine: &str) -> Res<String> {
    Ok(outputs["machines"][machine]
        .as_str()
        .ok_or_else(|| {
            Failure::new(
                ErrorCode::NotRunning,
                format!("no address for `{machine}` in the module's outputs; is it applied?"),
            )
            .machine(machine)
        })?
        .to_string())
}

fn cloud_vm(call: &Call, m: &isoloom_spec::Machine) -> Res<Command> {
    if is_windows(m) {
        return Err(Failure::new(ErrorCode::Unsupported, windows_hint(call.env.spec, call.machine))
            .machine(call.machine)
            .into());
    }
    let cloud = target::vm_cloud(call.env.dir, call.env.instance, call.env.spec, call.env.cloud)?;
    let module = call.out().join("cloud-vm").join(cloud);
    let outputs = terraform::output(&module)?;
    let host = address_in(&outputs, call.machine)?;
    let user = outputs["ssh_users"][call.machine].as_str().unwrap_or("isoloom").to_string();
    let mut c = ssh(call.env.ssh_key, &user, &host, call.tty, None);
    c.arg(call.sh_c());
    Ok(c)
}

fn cloud_docker(call: &Call, m: &isoloom_spec::Machine) -> Res<Command> {
    container_only(call, m, "Docker")?;
    let cloud = target::docker_cloud(call.env.cloud)?;
    let outputs = terraform::output(&call.out().join("cloud-docker").join(cloud))?;
    let host = outputs["ip"].as_str().ok_or("the module has no `ip` output; is it applied?")?.to_string();
    let user = outputs["ssh_user"].as_str().unwrap_or("root").to_string();
    let mut c = ssh(call.env.ssh_key, &user, &host, call.tty, None);
    c.arg(compose_on_host(call));
    Ok(c)
}

fn external(call: &Call, m: &isoloom_spec::Machine) -> Res<Command> {
    let e = m.external.as_ref().ok_or_else(|| format!("`{}` has no `external:` address", call.machine))?;
    let key = call.env.ssh_key.or(e.key.as_deref().map(Path::new));
    let mut c = ssh(key, e.user.as_deref().unwrap_or("root"), &e.address, call.tty, None);
    if let Some(p) = e.port {
        c.arg(format!("-p{p}"));
    }
    c.arg(call.sh_c());
    Ok(c)
}

/// Proxmox and ESXi (`dir`, the target's folder): through the router (the only VM on the
/// uplink), as the `isoloom` user.
fn behind_router(call: &Call, dir: &str) -> Res<Command> {
    let outputs = terraform::output(&call.out().join(dir))?;
    let router = outputs["address"]
        .as_str()
        .ok_or("the module has no `address` output; is it applied?")?
        .to_string();
    let host = address_in(&outputs, call.machine)?;
    let jump = format!("isoloom@{router}");
    let mut c = ssh(call.env.ssh_key, "isoloom", &host, call.tty, Some(&jump));
    c.arg(call.sh_c());
    Ok(c)
}

fn ssh(key: Option<&Path>, user: &str, host: &str, tty: bool, jump: Option<&str>) -> Command {
    let mut c = crate::tools::process::command("ssh");
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

/// A shell line that finds the interface holding `addr` (in `$IF`), or fails saying `missing`.
pub fn find_interface(addr: &str, missing: &str) -> String {
    format!(
        "IF=$(ip -o -4 addr show | awk '$4 ~ /^{}\\//{{print $2}}' | head -n 1); [ -n \"$IF\" ] || {{ echo '{missing}' >&2; exit 1; }}",
        addr.replace('.', "\\.")
    )
}

/// How to reach a Windows machine, which has no shell to drop into.
fn windows_hint(spec: &isoloom_spec::Spec, machine: &str) -> String {
    let r = isoloom_targets::resolved::resolve(spec);
    let addr = isoloom_targets::resolved::lookup(&r, &format!("machines.{machine}.addresses"))
        .and_then(|a| a.as_object())
        .and_then(|a| a.values().next())
        .and_then(|v| v.as_str())
        .unwrap_or("its address");
    format!(
        "`{machine}` runs Windows: reach it over RDP or WinRM at {addr} (local VMs: user vagrant, password vagrant; `vagrant rdp {machine}` once the RDP forward is enabled in the Vagrantfile)"
    )
}
