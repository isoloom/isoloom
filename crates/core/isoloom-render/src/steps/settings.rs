//! The `Settings` step: a VM's own settings that a container gets from its runtime, its fixed
//! `environment`, its `sysctls` and its `ulimits`. One script per machine, run as root before its
//! steps, by every VM generator (Vagrant, Proxmox, the clouds), so the same spec behaves the
//! same on all.
//!
//! - environment: `/etc/environment` (logins) and systemd's `DefaultEnvironment` (every
//!   service, so a service installed by the steps sees it as a container's process would);
//! - sysctls: `/etc/sysctl.d/90-isoloom.conf`, applied at once and at every boot;
//! - ulimits: `/etc/security/limits.d/90-isoloom.conf` (logins) and systemd's `DefaultLimit*`.
//!
//! Idempotent: run again, it writes the same files.

use isoloom_spec::model::Machine;
use isoloom_spec::shell::quote;

use crate::powershell;

/// The machine's settings, as a root shell script for a Linux VM. `None` when it has none.
pub(super) fn linux(m: &Machine) -> Option<String> {
    if m.environment.is_empty() && m.sysctls.is_empty() && m.ulimits.is_empty() {
        return None;
    }
    // systemd reads its defaults when it starts: re-executing it applies them to the services
    // started from now on (the machine's steps install them next).
    Some(format!(
        "set -e\nmkdir -p /etc/systemd/system.conf.d\n{}{}{}systemctl daemon-reexec 2>/dev/null || true\n",
        environment(m),
        sysctls(m),
        ulimits(m)
    ))
}

/// The fixed environment, for logins and for every service.
fn environment(m: &Machine) -> String {
    if m.environment.is_empty() {
        return String::new();
    }
    let keys = m.environment.keys().map(|k| regex_escape(k)).collect::<Vec<_>>().join("|");
    let lines: String = m.environment.iter().map(|(k, v)| format!("{k}={v}\n")).collect::<Vec<_>>().concat();
    let systemd = m
        .environment
        .iter()
        .map(|(k, v)| format!("\"{k}={}\"", v.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "grep -v -E '^({keys})=' /etc/environment > /etc/environment.isoloom 2>/dev/null || true\nprintf %s {} >> /etc/environment.isoloom && mv /etc/environment.isoloom /etc/environment\nprintf '[Manager]\\nDefaultEnvironment=%s\\n' {} > /etc/systemd/system.conf.d/90-isoloom-environment.conf\n",
        quote(&lines),
        quote(&systemd)
    )
}

/// The kernel parameters, applied now and at every boot.
fn sysctls(m: &Machine) -> String {
    if m.sysctls.is_empty() {
        return String::new();
    }
    let lines: String = m.sysctls.iter().map(|(k, v)| format!("{k} = {v}\n")).collect::<Vec<_>>().concat();
    format!(
        "printf %s {} > /etc/sysctl.d/90-isoloom.conf\nsysctl -q -p /etc/sysctl.d/90-isoloom.conf\n",
        quote(&lines)
    )
}

/// The resource limits, for logins (every user and root) and for every service.
fn ulimits(m: &Machine) -> String {
    if m.ulimits.is_empty() {
        return String::new();
    }
    let limits: String = m
        .ulimits
        .iter()
        .flat_map(|(name, u)| ["*", "root"].map(|who| format!("{who} soft {name} {}\n{who} hard {name} {}\n", u.soft(), u.hard())))
        .collect();
    let defaults: String = m
        .ulimits
        .iter()
        .map(|(name, u)| format!("DefaultLimit{}={}:{}\n", name.to_uppercase(), u.soft(), u.hard()))
        .collect::<Vec<_>>()
        .concat();
    format!(
        "mkdir -p /etc/security/limits.d && printf %s {} > /etc/security/limits.d/90-isoloom.conf\nprintf %s {} > /etc/systemd/system.conf.d/90-isoloom-limits.conf\n",
        quote(&limits),
        quote(&format!("[Manager]\n{defaults}"))
    )
}

/// The machine's environment on a Windows VM (machine-wide variables). Windows has no sysctls
/// or ulimits: validation keeps them off Windows machines.
pub(super) fn windows(m: &Machine) -> Option<String> {
    if m.environment.is_empty() {
        return None;
    }
    Some(
        m.environment
            .iter()
            .map(|(k, v)| {
                format!(
                    "[Environment]::SetEnvironmentVariable({}, {}, 'Machine')",
                    powershell::quote(k),
                    powershell::quote(v)
                )
            })
            .collect::<Vec<_>>()
            .join("; "),
    )
}

/// `text` matched literally in an extended regular expression.
fn regex_escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| ".[]{}()*+?^$|\\".contains(c).then_some('\\').into_iter().chain([c]))
        .collect()
}

#[cfg(test)]
mod tests;
