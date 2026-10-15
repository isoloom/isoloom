//! Each set-up step of a VM ([`Step`]) written once: as a root shell script for Linux, and as
//! PowerShell where Windows has the step. A VM backend only picks how the scripts reach the
//! machine (a Vagrant provisioner, cloud-init, `remote-exec`, WinRM), so every target sets a
//! machine up the same way.

use isoloom_plan::Environment;
use isoloom_plan::lifecycle::{RunKind, Step};
use isoloom_spec::shell::quote;

use crate::network::{ipv6, more_addresses};
use crate::{constraints, powershell, secrets};

mod disks;
mod files;
mod network;
mod process;
mod settings;
mod shared_volume;
mod wait;

/// Where the project is unpacked on a VM.
pub const PROJECT_DIR: &str = "/opt/isoloom";
/// The project's archive, uploaded before the steps run.
pub const PROJECT_ARCHIVE: &str = "/tmp/isoloom-project.tgz";
/// The inputs, uploaded before the steps run (`KEY=value` lines).
pub const INPUTS_FILE: &str = "/tmp/isoloom-inputs.env";
/// Where the inputs are kept when a reboot comes (`/tmp` is emptied at boot).
pub const KEPT_INPUTS_FILE: &str = "/var/lib/isoloom/inputs.env";
/// Installs Docker from Docker's own script, unless it is there (a root script): the Docker
/// hosts of `docker-vm` and `cloud-docker`.
pub const INSTALL_DOCKER: &str = "command -v docker >/dev/null || curl -fsSL https://get.docker.com | sh";

/// Where the project is unpacked on a Windows VM, and the machine's own scripts with it.
const WINDOWS_PROJECT_DIR: &str = "C:\\isoloom";

/// Installs packages with the machine's own package manager (Debian and Ubuntu, or the RHEL
/// family: Rocky, AlmaLinux, CentOS, Fedora).
const PKG: &str = "pkg() { if command -v apt-get >/dev/null; then apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \"$@\"; elif command -v dnf >/dev/null; then dnf install -y -q \"$@\"; else yum install -y -q \"$@\"; fi; }";

/// This boot's id: it changes at each boot.
const BOOT_ID: &str = "/proc/sys/kernel/random/boot_id";

/// `step` of `machine` as a root shell script for a Linux VM; `None` when it has nothing to do
/// there.
#[must_use]
pub fn linux(env: &Environment, machine: &str, step: &Step) -> Option<String> {
    match step {
        Step::Prelude => constraints::linux_prelude(env.spec()),
        Step::DiskGrow => Some(disks::GROW_ROOT_SH.to_owned()),
        Step::Disks { disks } => disks::linux(disks),
        Step::Settings => env.spec().machines.get(machine).and_then(settings::linux),
        Step::Hosts { entries } => network::hosts(entries),
        Step::Routes { routes, default } => network::routes(routes, *default),
        Step::Ipv6 { addresses, routes } => Some(ipv6::linux(addresses, routes)),
        Step::MoreAddresses { addresses } => Some(more_addresses::linux(addresses)),
        Step::Trunks => network::trunks(env, machine),
        Step::Volumes { paths } => (!paths.is_empty()).then(|| format!("mkdir -p {}\n", paths.iter().map(|p| quote(p)).collect::<Vec<_>>().join(" "))),
        Step::SharedVolume { volume, path, role } => Some(shared_volume::linux(volume, path, role)),
        Step::Project => Some(format!(
            "mkdir -p {PROJECT_DIR} && tar -xzf {PROJECT_ARCHIVE} -C {PROJECT_DIR} --strip-components=1 && rm -f {PROJECT_ARCHIVE}\n"
        )),
        Step::Files { files } => Some(files::linux(files)),
        Step::Secrets => env.machines.get(machine).map(|mp| secrets::step(machine, &mp.secrets)),
        Step::Inputs { persist } => persist.then(|| format!("mkdir -p /var/lib/isoloom && install -m 600 {INPUTS_FILE} {KEPT_INPUTS_FILE}\n")),
        Step::WaitFor { machine: dep, ports } => Some(wait::linux(dep, ports)),
        Step::Run { path, kind } => run(env, machine, path, *kind),
        Step::Reboot => Some(format!("{}(sleep 2 && reboot) >/dev/null 2>&1 &\n", record_boot())),
        Step::Egress => Some(network::egress(env)),
        Step::Tc => network::tc(env, machine),
        Step::Before { paths } => process::before(env, machine, paths),
        Step::Command => process::command(env, machine),
        Step::Setup { paths } => process::setup(env, machine, paths),
        Step::Ready => Some(format!(
            "{}mkdir -p /var/lib/isoloom && echo ready > /var/lib/isoloom/ready\n",
            process::healthy(env, machine).unwrap_or_default()
        )),
    }
}

/// `step` of `machine` as PowerShell for a Windows VM; `None` when Windows has no such step (or
/// nothing to do for it).
#[must_use]
pub fn windows(env: &Environment, machine: &str, step: &Step) -> Option<String> {
    match step {
        Step::DiskGrow => Some(disks::GROW_ROOT_PS1.to_owned()),
        Step::Disks { disks } => disks::windows(disks),
        Step::Settings => env.spec().machines.get(machine).and_then(settings::windows),
        Step::Hosts { entries } => network::windows_hosts(entries),
        Step::Routes { routes, .. } => network::windows_routes(routes),
        Step::Ipv6 { addresses, routes } => Some(ipv6::windows(addresses, routes)),
        Step::MoreAddresses { addresses } => Some(more_addresses::windows(addresses)),
        Step::Project => Some(format!(
            "$archive = 'C:\\Windows\\Temp\\isoloom-project.tgz'; $dest = '{WINDOWS_PROJECT_DIR}'; $strip = 1\n{}",
            powershell::UNTAR
        )),
        Step::Files { files } => Some(files::windows(files)),
        Step::Secrets => env.machines.get(machine).map(|mp| secrets::windows::step(machine, &mp.secrets)),
        Step::WaitFor { machine: dep, ports } => wait::windows(dep, ports),
        Step::Run {
            path,
            kind: RunKind::PowerShell,
        } => Some(format!(
            "Set-Location {WINDOWS_PROJECT_DIR}; & powershell -NoProfile -ExecutionPolicy Bypass -File {}; if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}",
            powershell::quote(path)
        )),
        Step::Before { paths } => process::windows_before(env, machine, paths),
        Step::Command => process::windows_command(env, machine),
        Step::Setup { paths } => Some(process::windows_setup(paths)),
        Step::Ready => Some(format!(
            "{}New-Item -ItemType Directory -Force {WINDOWS_PROJECT_DIR} | Out-Null; Set-Content {WINDOWS_PROJECT_DIR}\\ready ready",
            process::windows_healthy(env, machine).unwrap_or_default()
        )),
        // Linux's alone: the constraints, trunks and impairment are Linux tools; a Windows VM's
        // volumes, inputs and reboots are its own scripts'.
        Step::Prelude
        | Step::Trunks
        | Step::Volumes { .. }
        | Step::SharedVolume { .. }
        | Step::Inputs { .. }
        | Step::Run { .. }
        | Step::Reboot
        | Step::Egress
        | Step::Tc => None,
    }
}

/// One `provision` step, from the project folder, with the machine's inputs in its environment;
/// `None` for a PowerShell script, which only [`windows`] runs.
fn run(env: &Environment, machine: &str, path: &str, kind: RunKind) -> Option<String> {
    let inputs = inputs_file(env, machine).map(|f| format!("set -a; . {f}; set +a; ")).unwrap_or_default();
    match kind {
        RunKind::PowerShell => None,
        RunKind::Shell => Some(format!("cd {PROJECT_DIR} && sh -c {}\n", quote(&format!("{inputs}sh {path}")))),
        // Installed with the machine's own package manager; its inventory is the machine, by
        // its name (a play may name it), run locally.
        RunKind::Playbook => Some(format!(
            "{PKG}\ncommand -v ansible-playbook >/dev/null || pkg ansible-core\ncd {PROJECT_DIR} && sh -c {}\n",
            quote(&format!("{inputs}ansible-playbook -c local -i {machine}, {path}"))
        )),
    }
}

/// Where machine `machine`'s inputs are, when it reads any: kept on disk when a reboot comes or
/// a service reads them at every boot, else where they were uploaded.
fn inputs_file(env: &Environment, machine: &str) -> Option<&'static str> {
    let m = env.machines.get(machine).filter(|m| !m.inputs.is_empty())?;
    let kept = m.lifecycle.iter().any(|s| matches!(s, Step::Inputs { persist: true }));
    Some(if kept { KEPT_INPUTS_FILE } else { INPUTS_FILE })
}

/// Records this boot's id, for [`reboot_check`]: a transport that restarts the machine itself
/// (Terraform's `remote-exec`) runs it before restarting.
#[must_use]
pub fn record_boot() -> String {
    format!("mkdir -p /var/lib/isoloom && cp {BOOT_ID} /var/lib/isoloom/boot-id\n")
}

/// After a reboot step: fails when the machine didn't actually restart.
#[must_use]
pub fn reboot_check() -> String {
    format!("if [ \"$(cat {BOOT_ID})\" = \"$(cat /var/lib/isoloom/boot-id)\" ]; then echo 'the reboot step: the machine did not restart' >&2; exit 1; fi\n")
}

#[cfg(test)]
mod tests;
