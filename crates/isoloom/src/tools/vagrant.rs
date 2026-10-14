//! Vagrant for the local VM targets (vagrant, docker-vm, hybrid): the controller VM that runs
//! the provisioning and is halted after it, the baseline snapshot, and libvirt's quirks.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use isoloom_core as core;
use isoloom_core::Target;

use crate::{Res, target};

/// The controller VM, which runs the environment's `provision:` playbooks and the checks.
pub const CONTROLLER: &str = "isoloom-controller";

/// The baseline snapshot of a Vagrant environment: saved by `run`, restored by `reset`.
pub const BASELINE: &str = "isoloom-baseline";

/// Halts the controller again when dropped (it was booted for the checks).
pub struct Rehalt(PathBuf);

impl Drop for Rehalt {
    fn drop(&mut self) {
        let _ = Command::new("vagrant")
            .args(["halt", CONTROLLER])
            .current_dir(&self.0)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Boots the controller of a Vagrant output when it is halted (as it is after provisioning);
/// the guard halts it again.
pub fn wake_controller(out: &Path, t: Target, quiet: bool) -> Res<Option<Rehalt>> {
    let wd = out.join(target::vm_folder(t));
    let vagrantfile = std::fs::read_to_string(wd.join("Vagrantfile")).unwrap_or_default();
    if !vagrantfile.contains(&format!("config.vm.define \"{CONTROLLER}\"")) {
        return Ok(None);
    }
    if is_running(&wd)? {
        // Running, unless it's about to halt (provisioning just ended): then wait for it.
        if !is_halting(&wd) {
            return Ok(None);
        }
        for _ in 0..60 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if !is_running(&wd)? {
                break;
            }
        }
    }
    if !quiet {
        println!("booting the controller for the checks (it halts after provisioning)");
    }
    let ok = Command::new("vagrant")
        .args(["up", CONTROLLER, "--no-provision"])
        .current_dir(&wd)
        .stdout(if quiet { Stdio::null() } else { Stdio::inherit() })
        .status()?
        .success();
    if !ok {
        return Err("couldn't boot the controller (vagrant up isoloom-controller)".into());
    }
    Ok(Some(Rehalt(wd)))
}

/// Whether Vagrant says the controller of the folder runs.
fn is_running(wd: &Path) -> Res<bool> {
    let out = Command::new("vagrant")
        .args(["status", CONTROLLER, "--machine-readable"])
        .current_dir(wd)
        .output()?;
    Ok(controller_running(&String::from_utf8_lossy(&out.stdout)))
}

/// Whether the controller is about to halt itself (its provisioning just ended).
fn is_halting(wd: &Path) -> bool {
    Command::new("vagrant")
        .args(["ssh", CONTROLLER, "-c", "test -e /run/isoloom-halting"])
        .current_dir(wd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Whether `vagrant status --machine-readable` says the controller runs.
fn controller_running(machine_readable: &str) -> bool {
    // `time,machine,state,<state>` lines.
    machine_readable.lines().any(|l| {
        let f: Vec<&str> = l.split(',').collect();
        f.len() >= 4 && f[1] == CONTROLLER && f[2] == "state" && f[3] == "running"
    })
}

/// Whether a Vagrant folder's machines run on libvirt (`.vagrant/machines/<m>/libvirt/id`).
pub fn vagrant_uses_libvirt(wd: &Path) -> bool {
    std::fs::read_dir(wd.join(".vagrant/machines"))
        .into_iter()
        .flatten()
        .flatten()
        .any(|m| m.path().join("libvirt/id").is_file())
}

/// The spec's published UDP services, as `<machine>:<port>/udp (published as <host>)`.
pub fn udp_forwards(spec: &core::Spec) -> Vec<String> {
    spec.machines
        .iter()
        .flat_map(|(name, m)| {
            m.services
                .iter()
                .filter(|s| !s.protocol.is_tcp())
                .filter_map(move |s| s.publish.map(|h| format!("{name}:{}/udp (published as {h})", s.port)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libvirt_is_told_from_vagrants_machine_folders() {
        let dir = std::env::temp_dir().join(format!("isoloom-libvirt-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".vagrant/machines/dns/virtualbox")).unwrap();
        std::fs::write(dir.join(".vagrant/machines/dns/virtualbox/id"), "x").unwrap();
        assert!(!vagrant_uses_libvirt(&dir));
        std::fs::create_dir_all(dir.join(".vagrant/machines/web/libvirt")).unwrap();
        std::fs::write(dir.join(".vagrant/machines/web/libvirt/id"), "x").unwrap();
        assert!(vagrant_uses_libvirt(&dir));
        std::fs::remove_dir_all(&dir).ok();
        let spec = isoloom_core::parse(
            "version: 1\nname: t\nnetworks: { lab: { cidr: 10.9.0.0/24 } }\nmachines:\n  dns: { networks: { lab: 5 }, services: [{ port: 53, protocol: udp, publish: 10053 }, { port: 53, publish: 1053 }], vm: { os: debian-12 } }\n",
        )
        .unwrap();
        assert_eq!(udp_forwards(&spec), ["dns:53/udp (published as 10053)"]);
    }

    #[test]
    fn the_controllers_state_comes_from_its_status_line() {
        let up = "1700000000,isoloom-controller,metadata,provider,libvirt\n1700000000,isoloom-controller,state,running\n";
        let down = "1700000000,isoloom-controller,state,shutoff\n1700000000,web,state,running\n";
        assert!(super::controller_running(up));
        assert!(!super::controller_running(down));
    }
}
