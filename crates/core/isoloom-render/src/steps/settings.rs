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
//!   its size and mode; a path that is a tmpfs already (`/run`) takes them by a remount;
//! - dns: the resolver the machine asks (its servers, its domain then its search domains, its
//!   options), in whichever of systemd-resolved, NetworkManager, dhclient or udhcpc keeps it, so a
//!   lease doesn't put the network's back; on Windows, every interface's servers and the suffixes.
//!
//! Idempotent: run again, it writes the same files.

use isoloom_spec::model::{Dns, Machine};
use isoloom_spec::shell::quote;

use crate::powershell;

/// The machine's settings, as a root shell script for a Linux VM. `None` when it has none.
pub(super) fn linux(m: &Machine) -> Option<String> {
    if m.environment.is_empty() && m.sysctls.is_empty() && m.ulimits.is_empty() && m.tmpfs.is_empty() && m.dns.is_none() {
        return None;
    }
    // systemd reads its defaults when it starts: re-executing it applies them to the services
    // started from now on (the machine's steps install them next).
    Some(format!(
        "set -e\nmkdir -p /etc/systemd/system.conf.d\n{}{}{}{}{}systemctl daemon-reexec 2>/dev/null || true\n",
        environment(m),
        sysctls(m),
        ulimits(m),
        tmpfs(m),
        m.dns.as_ref().map(linux_dns).unwrap_or_default()
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

/// The machine's resolver on Linux: its servers (else the ones it has), its domain then its
/// search domains, its options. Kept where the system's resolver manager reads it:
/// systemd-resolved's own configuration (its servers asked for every name, `~.`), with a
/// resolv.conf of its stub when there are options; else a plain resolv.conf, with NetworkManager
/// told to leave it (`dns=none`) and dhclient and udhcpc told not to rewrite it from a lease.
fn linux_dns(d: &Dns) -> String {
    let domains: Vec<&str> = d.domain.iter().map(String::as_str).chain(d.search.iter().map(String::as_str)).collect();
    let servers = d.servers.join(" ");
    let search = domains.join(" ");
    let options = d.options.join(" ");
    // systemd-resolved: these servers for every name (`~.`), else only the domains.
    let resolved = if !servers.is_empty() {
        format!("[Resolve]\nDNS={servers}\nDomains={}\n", [search.as_str(), "~."].join(" ").trim())
    } else if !search.is_empty() {
        format!("[Resolve]\nDomains={search}\n")
    } else {
        "[Resolve]\n".to_owned()
    };
    let lines = |nameservers: &str| {
        let search = if search.is_empty() { String::new() } else { format!("search {search}\n") };
        let options = if options.is_empty() { String::new() } else { format!("options {options}\n") };
        [nameservers, &search, &options].concat()
    };
    let own: String = d.servers.iter().flat_map(|s| ["nameserver ", s.as_str(), "\n"]).collect();
    let dhclient: String = [
        (!d.servers.is_empty()).then(|| format!("supersede domain-name-servers {};\n", d.servers.join(", "))),
        (!domains.is_empty()).then(|| {
            let quoted: Vec<String> = domains.iter().map(|x| format!("\"{x}\"")).collect();
            format!("supersede domain-search {};\n", quoted.join(", "))
        }),
    ]
    .into_iter()
    .flatten()
    .collect();
    let stub = if options.is_empty() {
        String::new()
    } else {
        format!(
            "rm -f /etc/resolv.conf && printf %s {} > /etc/resolv.conf\n",
            quote(&lines("nameserver 127.0.0.53\n"))
        )
    };
    let nameservers = if own.is_empty() {
        "\"$(grep '^nameserver' /etc/resolv.conf 2>/dev/null || true)\n\"".to_owned()
    } else {
        quote(&own)
    };
    format!(
        "if systemctl is-active -q systemd-resolved 2>/dev/null; then\n  mkdir -p /etc/systemd/resolved.conf.d && printf %s {resolved} > /etc/systemd/resolved.conf.d/90-isoloom.conf\n  systemctl restart systemd-resolved\n  {stub}else\n  if systemctl is-active -q NetworkManager 2>/dev/null; then mkdir -p /etc/NetworkManager/conf.d && printf '[main]\\ndns=none\\n' > /etc/NetworkManager/conf.d/90-isoloom-dns.conf && systemctl reload NetworkManager; fi\n  if [ -f /etc/dhcp/dhclient.conf ]; then grep -v '# isoloom$' /etc/dhcp/dhclient.conf > /etc/dhcp/dhclient.conf.isoloom || true; printf %s {dhclient} | sed 's/$/ # isoloom/' >> /etc/dhcp/dhclient.conf.isoloom && mv /etc/dhcp/dhclient.conf.isoloom /etc/dhcp/dhclient.conf; fi\n  if [ -d /etc/udhcpc ]; then printf 'RESOLV_CONF=\"no\"\\n' > /etc/udhcpc/udhcpc.conf; fi\n  n={nameservers}\n  rm -f /etc/resolv.conf.isoloom && printf '%s' \"$n\" | grep . > /etc/resolv.conf.isoloom || true\n  printf %s {rest} >> /etc/resolv.conf.isoloom && rm -f /etc/resolv.conf && mv /etc/resolv.conf.isoloom /etc/resolv.conf\nfi\n",
        resolved = quote(&resolved),
        stub = stub,
        dhclient = quote(&dhclient),
        nameservers = nameservers,
        rest = quote(&lines("")),
    )
}

/// The machine's settings on a Windows VM: its environment (machine-wide variables) and its
/// resolver (every interface's servers, its domain then its search domains as the suffixes).
/// Windows has no sysctls, ulimits, tmpfs or resolver options: validation keeps them off Windows
/// machines.
pub(super) fn windows(m: &Machine) -> Option<String> {
    let mut parts: Vec<String> = m
        .environment
        .iter()
        .map(|(k, v)| {
            format!(
                "[Environment]::SetEnvironmentVariable({}, {}, 'Machine')",
                powershell::quote(k),
                powershell::quote(v)
            )
        })
        .collect();
    if let Some(d) = &m.dns {
        let list = |items: &mut dyn Iterator<Item = &String>| items.map(|s| powershell::quote(s)).collect::<Vec<_>>().join(", ");
        if !d.servers.is_empty() {
            parts.push(format!(
                "Get-NetAdapter | Where-Object Status -eq 'Up' | Set-DnsClientServerAddress -ServerAddresses @({})",
                list(&mut d.servers.iter())
            ));
        }
        let mut suffixes = d.domain.iter().chain(&d.search);
        let suffixes = list(&mut suffixes);
        if !suffixes.is_empty() {
            parts.push(format!("Set-DnsClientGlobalSetting -SuffixSearchList @({suffixes})"));
        }
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// `text` matched literally in an extended regular expression.
fn regex_escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| ".[]{}()*+?^$|\\".contains(c).then_some('\\').into_iter().chain([c]))
        .collect()
}

#[cfg(test)]
mod tests;
