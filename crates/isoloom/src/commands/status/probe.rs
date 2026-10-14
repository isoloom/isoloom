//! What the target's tool says about an environment: `running (3/3)`, `partly (1/3)`,
//! `stopped`, `applied (12 resources)`, `deploying`, or why it can't tell.

use std::path::Path;

use isoloom_core as core;
use isoloom_core::Target;
use isoloom_core::registry::Entry;

use crate::external::external_machines;
use crate::target;
use crate::tools::{compose, kubectl};

/// The live state of a recorded environment.
pub fn probe(e: &Entry) -> String {
    let out = e.dir.join(core::instance::output_dir(e.instance));
    if !out.is_dir() {
        return "stale (folder gone)".into();
    }
    let result = match e.target {
        Target::Docker | Target::Hosted => docker(e, &out),
        Target::Vagrant | Target::DockerVm | Target::Hybrid => vagrant(e, &out),
        Target::Kubernetes => kubernetes(e),
        Target::CloudServices => Ok(cloud_services(&out)),
        Target::External => Ok(external(&out)),
        Target::Proxmox | Target::CloudVm | Target::CloudDocker => terraform(e, &out),
    };
    result.unwrap_or_else(|why| format!("unknown ({why})"))
}

/// A tool's output, run from `wd`. Bounded: a wedged hypervisor service must not hang `status`.
fn run(program: &str, args: &[&str], wd: &Path) -> Result<String, String> {
    let cwd = std::env::current_dir().ok();
    let _ = std::env::set_current_dir(wd);
    let out = core::host::run_limited_in(program, args, 30, crate::spec::child_env());
    if let Some(c) = cwd {
        let _ = std::env::set_current_dir(c);
    }
    out.map_err(|e| e.to_string())?.ok_or_else(|| format!("{program} failed"))
}

fn counted(running: usize, total: usize) -> String {
    match (running, total) {
        (_, 0) => "stopped".to_string(),
        (r, t) if r == t => format!("running ({r}/{t})"),
        (0, _) => "stopped".to_string(),
        (r, t) => format!("partly ({r}/{t})"),
    }
}

fn docker(e: &Entry, out: &Path) -> Result<String, String> {
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
        let items = compose::ps_items(&text);
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

fn vagrant(e: &Entry, out: &Path) -> Result<String, String> {
    run("vagrant", &["status", "--machine-readable"], &out.join(target::vm_folder(e.target))).map(|text| {
        // The spec's machines: Isoloom's own (router, controller, tools) are infra, and a
        // halted controller is its normal state once provisioning is done.
        let states: Vec<&str> = text
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split(',').collect();
                (f.len() >= 4 && f[2] == "state" && !f[1].starts_with("isoloom-")).then_some(f[3])
            })
            .collect();
        counted(states.iter().filter(|s| **s == "running").count(), states.len())
    })
}

fn kubernetes(e: &Entry) -> Result<String, String> {
    let ns = kubectl::namespace(&e.name);
    run("kubectl", &["-n", &ns, "get", "deploy", "-o", "json"], &e.dir).map(|text| {
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let items = v["items"].as_array().cloned().unwrap_or_default();
        let ready = items.iter().filter(|d| d["status"]["readyReplicas"].as_u64().unwrap_or(0) >= 1).count();
        counted(ready, items.len())
    })
}

/// What the module's state holds: deployed resources, or nothing; `deploying` while an apply
/// runs (`up.sh` holds its process id in `deploying`).
fn cloud_services(out: &Path) -> String {
    let state = out.join("cloud-services/terraform.tfstate");
    let n = std::fs::read_to_string(&state)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["resources"].as_array().map(Vec::len))
        .unwrap_or(0);
    let deploying = std::fs::read_to_string(out.join("cloud-services/deploying")).is_ok_and(|pid| alive(pid.trim()));
    match (deploying, n) {
        (true, 0) => "deploying".to_string(),
        (true, n) => format!("deploying ({n} resources so far)"),
        (false, 0) => "not deployed".to_string(),
        (false, n) => format!("deployed ({n} resources)"),
    }
}

/// Whether the process `pid` still runs (on Windows, assumed: the shell's ids aren't Windows').
fn alive(pid: &str) -> bool {
    if pid.is_empty() || !pid.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if cfg!(windows) {
        return true;
    }
    crate::tools::process::command("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Each machine answers SSH, or not.
fn external(out: &Path) -> String {
    let machines = external_machines(out).unwrap_or_default();
    let up = machines
        .values()
        .filter(|m| {
            let mut args = vec!["-o".to_string(), "BatchMode=yes".into(), "-o".into(), "ConnectTimeout=5".into()];
            args.extend(m.ssh_args());
            args.push("true".into());
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            core::host::run_limited_in("ssh", &refs, 10, crate::spec::child_env()).ok().flatten().is_some()
        })
        .count();
    counted(up, machines.len())
}

fn terraform(e: &Entry, out: &Path) -> Result<String, String> {
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
