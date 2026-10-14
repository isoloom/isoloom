//! The `external` target: machines that already exist, reached over SSH. `run` provisions them
//! from here; `test` and `status` reach them the same way.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;

use isoloom_plan::Environment;
use isoloom_runtime::events::{ErrorCode, Scope};
use isoloom_spec::Target;

use crate::failure::Failure;
use crate::tools::process;
use crate::{Res, output};

/// An external machine's SSH endpoint, as `.isoloom/external/machines.json` records it.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExternalMachine {
    pub address: String,
    pub user: String,
    pub port: u16,
    pub key: Option<String>,
}

impl ExternalMachine {
    /// `ssh` arguments up to the destination: host keys accepted on first use, its port and key.
    pub fn ssh_args(&self) -> Vec<String> {
        let mut a = vec!["-o".to_string(), "StrictHostKeyChecking=accept-new".into(), format!("-p{}", self.port)];
        if let Some(k) = &self.key {
            a.extend(["-i".to_string(), k.clone()]);
        }
        a.push(format!("{}@{}", self.user, self.address));
        a
    }
}

/// The external machines of a generated project (`<out>/external/machines.json`).
pub fn external_machines(out: &Path) -> Res<BTreeMap<String, ExternalMachine>> {
    let path = out.join("external/machines.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e} (run `isoloom generate --target external`)", path.display()))?;
    Ok(serde_json::from_str(&text)?)
}

/// `isoloom run external`: provisions the existing machines over SSH, in start order: the
/// project copied to /opt/isoloom, each machine's `.sh` steps run there, its `.yml` steps run
/// from here against it (a `reboot` step restarts it and waits until it's back), then the
/// environment's `provision:` playbooks with the inventory.
pub fn external_up(dir: &Path, plan: &Environment, instance: Option<u8>) -> Res<()> {
    let spec = plan.spec();
    let out = dir.join(isoloom_spec::instance::output_dir(instance));
    let machines = external_machines(&out)?;
    for name in &plan.start_order {
        let Some(m) = machines.get(name) else { continue };
        let Some(vm) = &spec.machines[name].vm else { continue };
        if isoloom_spec::images::is_windows(&vm.os) {
            note(&format!(
                "{name}: Windows; provision it with the environment's playbooks (per-machine steps over SSH come later)"
            ));
            continue;
        }
        let at = Scope::step(Some(Target::External.id()), "provision").on(name);
        output::begin(&at, Some(format!("copying the project to {}:/opt/isoloom", m.address)));
        note(&format!("{name}: copying the project to {}:/opt/isoloom", m.address));
        copy_project(dir, m, &at)?;
        for step in &vm.provision {
            note(&format!("{name}: {step}"));
            run_step(name, m, step, &at)?;
        }
        output::done(&at, None);
    }
    environment_playbooks(dir, &out, spec)
}

/// A progress line on the terminal (none with `--json`: the steps say it).
fn note(line: &str) {
    if !output::json() {
        eprintln!("{line}");
    }
}

/// Runs a tool to completion for step `at`; a failure names it with its first arguments.
fn run(program: &str, args: &[String], stdin: Option<Stdio>, at: &Scope) -> Res<()> {
    let mut c = crate::tools::process::command(program);
    c.args(args);
    if let Some(i) = stdin {
        c.stdin(i);
    }
    if process::run_step(&mut c, at)?.success() {
        Ok(())
    } else {
        let message = format!("{program} {} failed", args.iter().take(3).cloned().collect::<Vec<_>>().join(" "));
        Err(Failure::new(ErrorCode::ToolFailed, message).into())
    }
}

/// The project copied to the machine's /opt/isoloom, through `tar` over SSH.
fn copy_project(dir: &Path, m: &ExternalMachine, at: &Scope) -> Res<()> {
    // The entries tar takes, listed here: without version control, the tools' state and
    // what .isoloomignore lists; symbolic links as links.
    let list: Vec<u8> = isoloom_render::ignore::entries(dir, &isoloom_render::ignore::Rules::read(dir))?
        .into_iter()
        .flat_map(|e| e.into_bytes().into_iter().chain([0]))
        .collect();
    let mut tar = crate::tools::process::command("tar")
        .args(["-czf", "-", "-C", &dir.display().to_string(), "--null", "--no-recursion", "-T", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("tar: {e}"))?;
    let (Some(mut stdin), Some(stdout)) = (tar.stdin.take(), tar.stdout.take()) else {
        return Err("tar: no pipes".into());
    };
    let writer = std::thread::spawn(move || std::io::Write::write_all(&mut stdin, &list));
    let mut a = m.ssh_args();
    a.push("sudo mkdir -p /opt/isoloom && sudo chown \"$(id -un)\" /opt/isoloom && tar -xzf - -C /opt/isoloom".into());
    run("ssh", &a, Some(Stdio::from(stdout)), at)?;
    writer.join().map_err(|_| "tar: the list writer stopped")?.map_err(|e| format!("tar: {e}"))?;
    if !tar.wait()?.success() {
        return Err("tar failed".into());
    }
    Ok(())
}

/// One of a machine's own steps: a reboot, a shell script run there, or a playbook run from here.
fn run_step(name: &str, m: &ExternalMachine, step: &str, at: &Scope) -> Res<()> {
    if isoloom_spec::model::is_reboot(step) {
        return reboot_external(name, &m.ssh_args(), at);
    }
    if isoloom_plan::lifecycle::has_extension(step, "sh") {
        let mut a = m.ssh_args();
        a.push(format!("cd /opt/isoloom && sudo sh {step}"));
        return run("ssh", &a, None, at);
    }
    let mut a = vec!["-i".to_string(), format!("{},", m.address), "-u".into(), m.user.clone(), "--become".into()];
    if let Some(k) = &m.key {
        a.extend(["--private-key".to_string(), k.clone()]);
    }
    a.extend(["-e".to_string(), format!("ansible_port={}", m.port), step.to_string()]);
    run("ansible-playbook", &a, None, at)
}

/// The environment's `provision:` playbooks, with the generated inventory.
fn environment_playbooks(dir: &Path, out: &Path, spec: &isoloom_spec::Spec) -> Res<()> {
    let at = Scope::step(Some(Target::External.id()), "provision");
    for step in &spec.provision {
        note(&format!("environment: {}", step.ansible));
        let mut a = vec!["-i".to_string(), out.join("external/inventory.ini").display().to_string()];
        for inv in &step.inventory {
            a.extend(["-i".to_string(), dir.join(inv).display().to_string()]);
        }
        // As JSON: `-e k=v` splits values on spaces and makes everything a string.
        if !step.vars.is_empty() {
            a.extend(["-e".to_string(), serde_json::to_string(&step.vars)?]);
        }
        a.push(dir.join(&step.ansible).display().to_string());
        output::begin(&at, Some(step.ansible.clone()));
        run("ansible-playbook", &a, None, &at)?;
        output::done(&at, None);
    }
    Ok(())
}

/// A `reboot` step on an existing machine: restart it over SSH, then wait until it answers
/// again from a new boot (its boot id changed), for up to 10 minutes.
fn reboot_external(name: &str, ssh: &[String], at: &Scope) -> Res<()> {
    let before = boot_id(ssh, false).ok_or_else(|| format!("{name}: can't read its boot id over SSH before the reboot"))?;
    // The connection may drop before ssh returns: its status says nothing.
    let mut c = crate::tools::process::command("ssh");
    c.args(ssh).arg("sudo sh -c 'systemctl --no-block reboot 2>/dev/null || reboot'");
    let _ = process::run_step(&mut c, at)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_secs(5));
        if boot_id(ssh, true).is_some_and(|now| now != before) {
            note(&format!("{name}: back after the reboot"));
            return Ok(());
        }
    }
    Err(format!("{name}: not back 10 minutes after the reboot step (waiting needs SSH without a password prompt: a key or an agent)").into())
}

/// The machine's boot id over SSH. Polled without prompting (`BatchMode`): a password prompt
/// would stop every try.
fn boot_id(ssh: &[String], batch: bool) -> Option<String> {
    const BOOT_ID: &str = "cat /proc/sys/kernel/random/boot_id";
    let mut c = crate::tools::process::command("ssh");
    if batch {
        c.args(["-o", "BatchMode=yes"]);
    }
    c.args(["-o", "ConnectTimeout=5"]).args(ssh).arg(BOOT_ID);
    let out = c.stderr(Stdio::null()).output().ok().filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
}
