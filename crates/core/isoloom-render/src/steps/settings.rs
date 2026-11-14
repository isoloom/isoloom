//! The `Settings` step: a VM's own settings that a container gets from its runtime, its fixed
//! `environment`, its `sysctls` and its `ulimits`. One script per machine, run as root before its
//! steps, by every VM generator (Vagrant, Proxmox, the clouds), so the same spec behaves the
//! same on all.
//!
//! - environment: `/etc/environment` (logins) and systemd's `DefaultEnvironment` (every
//!   service, so a service installed by the steps sees it as a container's process would);
//! - sysctls: `/etc/sysctl.d/90-isoloom.conf`, applied at once and at every boot;
//! - ulimits: `/etc/security/limits.d/90-isoloom.conf` (logins) and systemd's `DefaultLimit*`;
//! - tmpfs: a memory-backed mount per path, in `/etc/fstab` (every boot) and mounted at once, with
//!   its size and mode; a path that is a tmpfs already (`/run`) takes them by a remount.
//!
//! Idempotent: run again, it writes the same files.

use isoloom_spec::model::Machine;
use isoloom_spec::shell::quote;

use crate::powershell;

/// The machine's settings, as a root shell script for a Linux VM. `None` when it has none.
pub(super) fn linux(m: &Machine) -> Option<String> {
    if m.environment.is_empty() && m.sysctls.is_empty() && m.ulimits.is_empty() && m.tmpfs.is_empty() {
        return None;
    }
    // systemd reads its defaults when it starts: re-executing it applies them to the services
    // started from now on (the machine's steps install them next).
    Some(format!(
        "set -e\nmkdir -p /etc/systemd/system.conf.d\n{}{}{}{}systemctl daemon-reexec 2>/dev/null || true\n",
        environment(m),
        sysctls(m),
        ulimits(m),
        tmpfs(m)
    ))
}

/// The fixed environment, for logins and for every service. `/etc/environment` holds one
/// variable a line with no escapes, so a value of several lines reaches logins from a
/// `/etc/profile.d` script instead; systemd reads it escaped (`\n`), on the one line its
/// configuration allows.
fn environment(m: &Machine) -> String {
    if m.environment.is_empty() {
        return String::new();
    }
    let keys = m.environment.keys().map(|k| regex_escape(k)).collect::<Vec<_>>().join("|");
    let (one_line, several): (Vec<_>, Vec<_>) = m.environment.iter().partition(|(_, v)| !v.contains(['\n', '\r']));
    let lines: String = one_line.iter().map(|(k, v)| format!("{k}={v}\n")).collect::<Vec<_>>().concat();
    let systemd = m
        .environment
        .iter()
        .map(|(k, v)| format!("\"{k}={}\"", systemd_escape(v)))
        .collect::<Vec<_>>()
        .join(" ");
    let profile = if several.is_empty() {
        String::new()
    } else {
        let exports = several.iter().map(|(k, v)| format!("export {k}={}\n", quote(v))).collect::<Vec<_>>().concat();
        format!("printf %s {} > /etc/profile.d/90-isoloom-environment.sh\n", quote(&exports))
    };
    format!(
        "grep -v -E '^({keys})=' /etc/environment > /etc/environment.isoloom 2>/dev/null || true\nprintf %s {} >> /etc/environment.isoloom && mv /etc/environment.isoloom /etc/environment\n{profile}printf '[Manager]\\nDefaultEnvironment=%s\\n' {} > /etc/systemd/system.conf.d/90-isoloom-environment.conf\n",
        quote(&lines),
        quote(&systemd)
    )
}

/// A value inside a double-quoted systemd assignment: backslashes and quotes escaped, and the
/// line breaks a configuration line can't hold written as C escapes, which systemd reads back.
fn systemd_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r")
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

/// The memory-backed mounts, as a container's `tmpfs` gets them: each path's line in
/// `/etc/fstab` (replacing one it had), then the mount now. A path that is already a tmpfs (`/run`,
/// often `/tmp`) keeps its files and takes the size and mode by a remount; any other path gets a
/// fresh, empty one, as in a container. Paths are absolute with no whitespace (validation), so
/// they are fstab fields as written.
fn tmpfs(m: &Machine) -> String {
    m.tmpfs
        .iter()
        .map(|t| {
            let options: Vec<String> = t
                .size()
                .map(|s| format!("size={s}"))
                .into_iter()
                .chain(t.mode().map(|mode| format!("mode={mode}")))
                .collect();
            let remount = if options.is_empty() {
                String::new()
            } else {
                format!("mount -o {} \"$p\"", quote(&format!("remount,{}", options.join(","))))
            };
            let options = if options.is_empty() { "defaults".to_owned() } else { options.join(",") };
            let line = format!("tmpfs {} tmpfs {options} 0 0\n", t.path());
            format!(
                "p={path}\nmkdir -p \"$p\"\nawk -v p=\"$p\" '$2 != p' /etc/fstab > /etc/fstab.isoloom && printf %s {line} >> /etc/fstab.isoloom && mv /etc/fstab.isoloom /etc/fstab\nif [ \"$(awk -v p=\"$p\" '$2 == p {{ t = $3 }} END {{ print t }}' /proc/mounts)\" = tmpfs ]; then {remount}{sep}; else mount \"$p\"; fi\n",
                path = quote(t.path()),
                line = quote(&line),
                sep = if remount.is_empty() { ":" } else { "" },
            )
        })
        .collect::<Vec<_>>()
        .concat()
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
