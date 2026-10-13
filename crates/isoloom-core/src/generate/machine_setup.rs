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

use crate::model::Machine;
use crate::shell::quote;

/// The machine's settings, as a root shell script for a Linux VM. `None` when it has none.
pub fn linux(m: &Machine) -> Option<String> {
    if m.environment.is_empty() && m.sysctls.is_empty() && m.ulimits.is_empty() {
        return None;
    }
    let mut sh = String::from("set -e\nmkdir -p /etc/systemd/system.conf.d\n");
    environment(m, &mut sh);
    sysctls(m, &mut sh);
    ulimits(m, &mut sh);
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
pub fn sudo_command(m: &Machine) -> Option<String> {
    linux(m).map(|sh| format!("sudo sh -c {}", quote(&sh)))
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
