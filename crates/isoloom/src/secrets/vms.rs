//! Secrets on the VM targets: each machine that gets some, and each Docker host whose
//! containers do, is written to over its own connection (`vagrant ssh`, SSH through the
//! Proxmox router, SSH to a cloud VM or an existing host; WinRM or OpenSSH to a Windows
//! machine, see `windows`). The values travel on the connection's standard input, read by a
//! script with no value in it (`isoloom_render::secrets::receive`, or its Windows form), so none
//! is in a command line, a generated file, the user data, Terraform's state or a Vagrant
//! provisioner.
//!
//! A machine's set-up waits for them (its `Secrets` step) inside the target's tool, so delivery
//! runs while the tool does: every few seconds, each machine still waiting is tried until it
//! answers. On Proxmox the tool returns before the set-up (cloud-init) runs, and an existing
//! host has no set-up of its own: they are delivered to after the tool, and before, instead.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use isoloom_plan::{MachinePlan, MachineShape, SecretFile};
use isoloom_render::secrets as render;
use isoloom_runtime::events::{Scope, State};
use isoloom_spec::Target;
use isoloom_spec::model::Winrm;
use isoloom_spec::shell::quote;

use super::Values;
use super::windows::{self, Tunnel};
use crate::machine::{Env, on_machine};
use crate::prepare::Prepared;
use crate::{output, target};

/// Seconds between two rounds of tries.
const PAUSE: Duration = Duration::from_secs(3);
/// Seconds one try may take before it is given up (a connection that hangs).
const TRY_LIMIT: Duration = Duration::from_secs(120);

/// A machine (or Docker host) the secrets go to.
pub struct Recipient {
    /// The machine, as the messages name it.
    pub who: String,
    /// Its secrets, in the order the spec declares them.
    names: Vec<String>,
    /// Their files on it (none on a Docker host: Compose reads the kept values).
    files: Vec<SecretFile>,
    /// How it is reached.
    reach: Reach,
    /// What it runs, and so how its delivery is spoken.
    guest: Guest,
}

/// The system a recipient runs, as far as delivery cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Guest {
    /// Linux: `sh` reads the values.
    Linux,
    /// Windows over OpenSSH (`vm.image.winssh`, an existing host).
    WindowsSsh,
    /// Windows over WinRM; HTTPS and NTLM when `ssl`.
    WindowsWinrm { ssl: bool },
}

impl Guest {
    /// What machine `mp` runs, on target `t`.
    fn of(mp: &MachinePlan, t: Target) -> Guest {
        if !mp.is_windows() {
            Guest::Linux
        } else if mp.winssh || t == Target::External {
            Guest::WindowsSsh
        } else {
            Guest::WindowsWinrm {
                ssl: mp.winrm == Some(Winrm::Ssl),
            }
        }
    }
}

/// How a recipient is reached.
enum Reach {
    /// `vagrant ssh` in the target's folder, to the machine named (the only one when `None`).
    Vagrant { dir: PathBuf, machine: Option<String> },
    /// The way `isoloom exec` reaches the machine.
    Exec(String),
    /// SSH at the `<user>@<address>` its Terraform set-up writes in this file.
    Announced(PathBuf),
}

/// Where a target's tool runs from, and which cloud: what finding the recipients needs.
pub struct Place<'a> {
    /// The output folder (`.isoloom` or an instance's).
    pub out: PathBuf,
    /// The cloud of `cloud-vm` and `cloud-docker`.
    pub cloud: Option<&'a str>,
    /// The instance.
    pub instance: Option<u8>,
}

/// Everything that gets secrets on `p`'s target, in start order: its VMs, then its Docker host.
pub fn recipients(p: &Prepared, place: &Place) -> Vec<Recipient> {
    let t = p.target;
    let vm = |name: &str| match t {
        Target::Vagrant | Target::Hybrid => Some(Reach::Vagrant {
            dir: place.out.join(target::vm_folder(t)),
            machine: Some(name.to_string()),
        }),
        Target::Proxmox | Target::External => Some(Reach::Exec(name.to_string())),
        Target::CloudVm => Some(Reach::Announced(reach_file(&place.out, "cloud-vm", place.cloud.unwrap_or("aws"), name))),
        _ => None,
    };
    let mut out: Vec<Recipient> = p
        .plan
        .start_order
        .iter()
        .filter_map(|name| Some((name, p.plan.machines.get(name)?)))
        .filter(|(_, m)| matches!(m.shape, MachineShape::Vm | MachineShape::External) && !m.secrets.is_empty())
        .filter_map(|(name, m)| {
            Some(Recipient {
                who: name.clone(),
                names: m.secrets.iter().map(|s| s.name.clone()).collect(),
                files: m.secrets.clone(),
                reach: vm(name)?,
                guest: Guest::of(m, t),
            })
        })
        .collect();
    let host = match t {
        Target::DockerVm => Some(Reach::Vagrant {
            dir: place.out.join(target::vm_folder(t)),
            machine: None,
        }),
        Target::Hybrid => Some(Reach::Vagrant {
            dir: place.out.join(target::vm_folder(t)),
            machine: Some("isoloom-docker".into()),
        }),
        Target::CloudDocker => Some(Reach::Announced(reach_file(
            &place.out,
            "cloud-docker",
            place.cloud.unwrap_or_default(),
            "environment",
        ))),
        _ => None,
    };
    let names = render::of_containers(&p.plan);
    if let Some(reach) = host.filter(|_| !names.is_empty()) {
        out.push(Recipient {
            who: "the Docker host".into(),
            names,
            files: Vec::new(),
            reach,
            guest: Guest::Linux,
        });
    }
    out
}

/// The file a Terraform set-up announces its machine in.
fn reach_file(out: &Path, dir: &str, cloud: &str, name: &str) -> PathBuf {
    out.join(dir).join(cloud).join(render::REACH_DIR).join(name)
}

/// Whether this host can reach every recipient: python3 with pywinrm when one is a Windows
/// machine reached over WinRM. Asked before the target's tool starts, so a missing helper
/// stops the run at once instead of after the set-up's wait.
pub fn reachable(recipients: &[Recipient]) -> Result<(), String> {
    if recipients.iter().any(|r| matches!(r.guest, Guest::WindowsWinrm { .. })) {
        windows::winrm_ready()
    } else {
        Ok(())
    }
}

/// The folder of announcements of `p`'s Terraform module, emptied before a run: an address
/// from an earlier run would be another VM's.
pub fn forget_announcements(p: &Prepared, place: &Place) {
    let module = match p.target {
        Target::CloudVm => place.out.join("cloud-vm").join(place.cloud.unwrap_or("aws")),
        Target::CloudDocker => place.out.join("cloud-docker").join(place.cloud.unwrap_or_default()),
        _ => return,
    };
    let _ = std::fs::remove_dir_all(module.join(render::REACH_DIR));
}

/// Delivers to each recipient, trying the ones that don't answer again every few seconds while
/// `go_on` says so. What is left: each recipient never reached, with the last reason.
pub fn deliver(p: &Prepared, place: &Place, values: &Values, recipients: Vec<Recipient>, go_on: &dyn Fn() -> bool) -> Vec<Missed> {
    let env = Env {
        dir: &p.dir,
        spec: &p.spec,
        target: p.target,
        cloud: place.cloud,
        instance: place.instance,
        ssh_key: None,
    };
    let mut pending: Vec<(Recipient, String)> = recipients.into_iter().map(|r| (r, String::from("not tried"))).collect();
    loop {
        pending.retain_mut(|(r, why)| match attempt(&env, r, values) {
            Ok(()) => {
                delivered(p.target, &r.who);
                false
            }
            Err(e) => {
                *why = e;
                true
            }
        });
        if pending.is_empty() || !go_on() {
            return pending.into_iter().map(|(recipient, why)| Missed { recipient, why }).collect();
        }
        std::thread::sleep(PAUSE);
    }
}

/// A recipient delivery didn't reach, and the last reason.
pub struct Missed {
    /// The recipient.
    pub recipient: Recipient,
    /// Why the last try failed.
    pub why: String,
}

/// What was missed, one `<who>: <why>` per line, for an error message.
pub fn report(missed: &[Missed]) -> String {
    missed
        .iter()
        .map(|m| format!("{}: {}", m.recipient.who, m.why))
        .collect::<Vec<_>>()
        .join("\n  ")
}

/// A `go_on` that says yes until `limit` has passed.
pub fn for_at_most(limit: Duration) -> impl Fn() -> bool {
    let end = Instant::now() + limit;
    move || Instant::now() < end
}

/// The step event (or line) saying a recipient has its secrets. Only an event: the step that
/// runs meanwhile (the tool's) stays the current one.
fn delivered(t: Target, who: &str) {
    let at = Scope::step(Some(t.id()), "secrets").on(who);
    output::events().step(&at, State::Done, None);
    if !output::json() {
        eprintln!("Secrets delivered to {who}");
    }
}

/// One try: the receiving script on the recipient, as root, the values on its standard input.
fn attempt(env: &Env, r: &Recipient, values: &Values) -> Result<(), String> {
    let values = isoloom_runtime::secrets::delivery_input(values, &r.names).ok_or("a secret has no value")?;
    let (mut c, _tunnel, input) = if r.guest == Guest::Linux {
        let line = as_root(&render::receive(&r.names, &r.files));
        (command(env, &r.reach, &line)?, None, values)
    } else {
        let (c, tunnel) = windows_command(env, &r.reach, r.guest)?;
        (c, tunnel, render::windows::input(&r.names, &r.files, &values))
    };
    c.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        // A machine that isn't up yet closes it at once: the exit status says so.
        let _ = stdin.write_all(&input);
    }
    let end = Instant::now() + TRY_LIMIT;
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(s) => break s,
            None if Instant::now() >= end => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("no answer".into());
            }
            None => std::thread::sleep(Duration::from_millis(200)),
        }
    };
    if status.success() {
        return Ok(());
    }
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = std::io::Read::read_to_string(&mut e, &mut err);
    }
    Err(err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("it failed").trim().to_string())
}

/// The command reaching the recipient with `line` to run there.
fn command(env: &Env, reach: &Reach, line: &str) -> Result<Command, String> {
    match reach {
        Reach::Vagrant { dir, machine } => {
            let mut c = crate::tools::process::command("vagrant");
            c.current_dir(dir).arg("ssh");
            c.args(machine.iter());
            c.args(["-c", line, "--", "-T"]);
            Ok(c)
        }
        Reach::Exec(machine) => on_machine(env, machine, Some(line), false, false).map_err(|e| e.to_string()),
        Reach::Announced(file) => {
            let text = std::fs::read_to_string(file).map_err(|_| "its address isn't known yet".to_string())?;
            let (user, host) = text.trim().split_once('@').ok_or("its address isn't known yet")?;
            let mut c = crate::tools::process::command("ssh");
            c.args(["-o", "StrictHostKeyChecking=accept-new", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10"]);
            c.arg(format!("{user}@{host}")).arg(line);
            Ok(c)
        }
    }
}

/// The command reaching a Windows recipient, with the tunnel it goes through (Proxmox): the
/// bootstrap over OpenSSH, or over WinRM.
fn windows_command(env: &Env, reach: &Reach, guest: Guest) -> Result<(Command, Option<Tunnel>), String> {
    match (reach, guest) {
        (Reach::Vagrant { dir, machine: Some(m) }, Guest::WindowsSsh) => Ok((windows::vagrant_ssh(dir, m), None)),
        (Reach::Vagrant { dir, machine: Some(m) }, Guest::WindowsWinrm { ssl }) => Ok((windows::over_winrm(&windows::vagrant_endpoint(dir, m, ssl)?), None)),
        (Reach::Exec(m), _) if env.target == Target::External => {
            let e = env
                .spec
                .machines
                .get(m)
                .and_then(|m| m.external.as_ref())
                .ok_or("it has no `external:` address")?;
            Ok((windows::ssh(e), None))
        }
        (Reach::Exec(m), _) => {
            let module = env.dir.join(isoloom_spec::instance::output_dir(env.instance)).join("proxmox");
            let outputs = crate::tools::terraform::output(&module).map_err(|e| e.to_string())?;
            let router = outputs["address"].as_str().ok_or("the module has no `address` output yet")?;
            let host = outputs["machines"][m.as_str()].as_str().ok_or("its address isn't known yet")?;
            let (endpoint, tunnel) = windows::through_router(&module, router, host)?;
            Ok((windows::over_winrm(&endpoint), Some(tunnel)))
        }
        (Reach::Announced(file), _) => {
            let text = std::fs::read_to_string(file).map_err(|_| "its address isn't known yet".to_string())?;
            let module = file.parent().and_then(Path::parent).ok_or("its module isn't known")?;
            let endpoint = windows::announced(&text, module).ok_or("its address isn't known yet")??;
            Ok((windows::over_winrm(&endpoint), None))
        }
        _ => Err("a Windows machine isn't reached this way".into()),
    }
}

/// `script` run as root: directly when the login is root, else through `sudo`.
fn as_root(script: &str) -> String {
    let q = quote(script);
    format!("if [ \"$(id -u)\" = 0 ]; then exec sh -c {q}; fi; exec sudo sh -c {q}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_windows_machine_is_reached_its_own_way() {
        use super::Guest;
        use isoloom_spec::Target;
        let spec = isoloom_spec::parse(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  l: { networks: { lan: 10 }, vm: { os: debian-12 } }\n  w: { networks: { lan: 11 }, vm: { os: windows-server-2022 } }\n  s: { networks: { lan: 12 }, vm: { os: windows-server-2022, image: { winssh: true } } }\n  t: { networks: { lan: 13 }, vm: { os: windows-server-2022, image: { winrm: ssl } } }\n",
        )
        .unwrap();
        let env = isoloom_plan::Environment::build(&spec, Target::Vagrant, None);
        let of = |m: &str, t| Guest::of(&env.machines[m], t);
        assert_eq!(of("l", Target::Vagrant), Guest::Linux);
        assert_eq!(of("w", Target::Vagrant), Guest::WindowsWinrm { ssl: false });
        assert_eq!(of("t", Target::Vagrant), Guest::WindowsWinrm { ssl: true });
        assert_eq!(of("s", Target::Vagrant), Guest::WindowsSsh);
        assert_eq!(of("w", Target::External), Guest::WindowsSsh);
    }

    #[test]
    fn root_or_sudo() {
        let line = super::as_root("echo hi");
        assert_eq!(line, "if [ \"$(id -u)\" = 0 ]; then exec sh -c 'echo hi'; fi; exec sudo sh -c 'echo hi'");
    }
}
