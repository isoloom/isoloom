//! A VM's own settings that a container gets from its runtime: its fixed `environment`, its
//! `sysctls` and its `ulimits`. One script per machine, run as root before its steps, by every
//! VM generator (Vagrant, Proxmox, the clouds), so the same spec behaves the same on all.
//!
//! - environment: `/etc/environment` (logins) and systemd's `DefaultEnvironment` (every
//!   service, so a service installed by the steps sees it as a container's process would);
//! - sysctls: `/etc/sysctl.d/90-isoloom.conf`, applied at once and at every boot;
//! - ulimits: `/etc/security/limits.d/90-isoloom.conf` (logins) and systemd's `DefaultLimit*`.
//!
//! Idempotent: run again, it writes the same files.

use std::fmt::Write as _;

use crate::model::{Machine, Spec};
use crate::shell::quote;

/// Installs packages with the machine's own package manager (Debian and Ubuntu, or the
/// RHEL family: Rocky, AlmaLinux, CentOS, Fedora).
pub const PKG: &str = "pkg() { if command -v apt-get >/dev/null; then apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \"$@\"; elif command -v dnf >/dev/null; then dnf install -y -q \"$@\"; else yum install -y -q \"$@\"; fi; }";

/// The machine's settings, as a root shell script for a Linux VM. `None` when it has none.
pub fn linux(spec: &Spec, name: &str, m: &Machine) -> Option<String> {
    let shared: Vec<&String> = m.volumes.keys().filter(|v| spec.volumes.contains_key(*v)).collect();
    if m.environment.is_empty() && m.sysctls.is_empty() && m.ulimits.is_empty() && shared.is_empty() {
        return None;
    }
    let mut sh = String::from("set -e\nmkdir -p /etc/systemd/system.conf.d\n");
    environment(m, &mut sh);
    sysctls(m, &mut sh);
    ulimits(m, &mut sh);
    for v in shared {
        shared_volume(spec, name, m, v, &mut sh);
    }
    // systemd reads its defaults when it starts: re-executing it applies them to the services
    // started from now on (the machine's steps install them next).
    sh.push_str("systemctl daemon-reexec 2>/dev/null || true\n");
    Some(sh)
}

fn environment(m: &Machine, sh: &mut String) {
    if m.environment.is_empty() {
        return;
    }
    let keys = m.environment.keys().map(|k| regex_escape(k)).collect::<Vec<_>>().join("|");
    let lines: String = m.environment.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    let systemd = m
        .environment
        .iter()
        .map(|(k, v)| format!("\"{k}={}\"", v.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(" ");
    let _ = writeln!(
        sh,
        "grep -v -E '^({keys})=' /etc/environment > /etc/environment.isoloom 2>/dev/null || true\nprintf %s {} >> /etc/environment.isoloom && mv /etc/environment.isoloom /etc/environment\nprintf '[Manager]\\nDefaultEnvironment=%s\\n' {} > /etc/systemd/system.conf.d/90-isoloom-environment.conf",
        quote(&lines),
        quote(&systemd)
    );
}

fn sysctls(m: &Machine, sh: &mut String) {
    if m.sysctls.is_empty() {
        return;
    }
    let lines: String = m.sysctls.iter().map(|(k, v)| format!("{k} = {v}\n")).collect();
    let _ = writeln!(
        sh,
        "printf %s {} > /etc/sysctl.d/90-isoloom.conf\nsysctl -q -p /etc/sysctl.d/90-isoloom.conf",
        quote(&lines)
    );
}

fn ulimits(m: &Machine, sh: &mut String) {
    if m.ulimits.is_empty() {
        return;
    }
    let mut limits = String::new();
    let mut systemd = String::from("[Manager]\n");
    for (name, u) in &m.ulimits {
        for who in ["*", "root"] {
            let _ = writeln!(limits, "{who} soft {name} {}\n{who} hard {name} {}", u.soft(), u.hard());
        }
        let _ = writeln!(systemd, "DefaultLimit{}={}:{}", name.to_uppercase(), u.soft(), u.hard());
    }
    let _ = writeln!(
        sh,
        "mkdir -p /etc/security/limits.d && printf %s {} > /etc/security/limits.d/90-isoloom.conf\nprintf %s {} > /etc/systemd/system.conf.d/90-isoloom-limits.conf",
        quote(&limits),
        quote(&systemd)
    );
}

/// [`linux`] as one command for a non-root SSH user.
pub fn sudo_command(spec: &Spec, name: &str, m: &Machine) -> Option<String> {
    linux(spec, name, m).map(|sh| format!("sudo sh -c {}", quote(&sh)))
}

/// A shared volume (see [`crate::model::SharedVolume`]): its owner serves its directory over
/// NFS to the machines that mount it, each of which mounts it at its own path (at boot too),
/// waiting for the owner as long as it takes to come up (15 minutes at most).
fn shared_volume(spec: &Spec, name: &str, m: &Machine, v: &str, sh: &mut String) {
    let path = &m.volumes[v];
    let owner = spec.volume_owner(v).expect("validated: a shared volume has an owner");
    let _ = writeln!(sh, "{PKG}\nmkdir -p {}", quote(path));
    if owner == name {
        let clients = spec
            .volume_users(v)
            .into_iter()
            .filter(|u| *u != name)
            .map(|u| format!("{}(rw,sync,no_subtree_check,no_root_squash)", super::address_for(spec, name, u)))
            .collect::<Vec<_>>()
            .join(" ");
        let tag = format!("# isoloom-volume-{v}");
        let _ = writeln!(
            sh,
            "if command -v apt-get >/dev/null; then pkg nfs-kernel-server; else pkg nfs-utils; fi\n\
             grep -v -F {tag_q} /etc/exports > /etc/exports.isoloom 2>/dev/null || true\n\
             printf '%s %s %s\\n' {path_q} {clients_q} {tag_q} >> /etc/exports.isoloom && mv /etc/exports.isoloom /etc/exports\n\
             systemctl enable --now nfs-server 2>/dev/null || systemctl enable --now nfs-kernel-server\n\
             exportfs -ra",
            tag_q = quote(&tag),
            path_q = quote(path),
            clients_q = quote(&clients),
        );
    } else {
        let source = format!("{}:{}", super::address_for(spec, name, owner), spec.machines[owner].volumes[v]);
        let fstab = format!("{source} {path} nfs defaults,_netdev 0 0");
        let _ = writeln!(
            sh,
            "if command -v apt-get >/dev/null; then pkg nfs-common; else pkg nfs-utils; fi\n\
             grep -qF {source_q} /etc/fstab || printf '%s\\n' {fstab_q} >> /etc/fstab\n\
             i=0; until mountpoint -q {path_q} || mount {path_q} 2>/dev/null; do i=$((i+1)); if [ $i -ge 180 ]; then echo {err} >&2; exit 1; fi; sleep 5; done",
            source_q = quote(&format!("{source} ")),
            fstab_q = quote(&fstab),
            path_q = quote(path),
            err = quote(&format!("volume {v}: {owner} doesn't serve it (NFS)")),
        );
    }
}

/// The machine's environment on a Windows VM (machine-wide variables). Windows has no sysctls
/// or ulimits: validation keeps them off Windows machines.
pub fn windows(m: &Machine) -> Option<String> {
    if m.environment.is_empty() {
        return None;
    }
    let ps = |s: &str| format!("'{}'", s.replace('\'', "''"));
    Some(
        m.environment
            .iter()
            .map(|(k, v)| format!("[Environment]::SetEnvironmentVariable({}, {}, 'Machine')", ps(k), ps(v)))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

/// `text` matched literally in an extended regular expression.
fn regex_escape(text: &str) -> String {
    text.chars().fold(String::new(), |mut out, c| {
        if ".[]{}()*+?^$|\\".contains(c) {
            out.push('\\');
        }
        out.push(c);
        out
    })
}
