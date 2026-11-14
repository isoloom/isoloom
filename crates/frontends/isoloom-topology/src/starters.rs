//! Starter set-up scripts, so a downloaded project with provisioning steps runs as is.
//!
//! A topology lists a machine's provisioning steps (`provision/db.sh`), but nobody writes the
//! files. Each shell step gets a script that installs what the machine's ports call for and makes
//! it listen on the network (PostgreSQL, MariaDB and Redis listen on localhost by default, so the
//! project's checks couldn't reach them). The steps are portable, so a script only installs and
//! configures: Isoloom runs it when it builds the machine's container image, or on the VM, and the
//! services are started only where an init system runs them. A `.ps1` step gets a script that
//! installs IIS for its web ports and opens each other port in the firewall, an Ansible playbook
//! a minimal one (on Windows, one the controller runs over WinRM).
//! They are starting points to edit, not a configuration management system.

mod packages;

use indexmap::IndexMap;
use isoloom_manifest::RelPath;
use isoloom_manifest::ignore;
use isoloom_spec::Spec;
use isoloom_spec::net::NetworkBlock;
use isoloom_spec::syntax::{Machine, Octets, OsRef, Step, has_extension};

use crate::ProjectFile;
use crate::controller::ControllerPlaybook;
use packages::{Family, PACKAGES, install, start, wanted};

/// Each network's block, by the spec's name for it ([`crate::draft::Draft::blocks`]).
type Blocks = IndexMap<String, NetworkBlock>;

/// Adds to `files` one starter per provisioning step, the machines' own then the controller's:
/// at paths inside the project, which no file has yet (Isoloom's own aside).
pub(crate) fn add_starters(files: &mut Vec<ProjectFile>, spec: &Spec, blocks: &Blocks, controller: &[ControllerPlaybook]) {
    for (name, m) in &spec.machines {
        let (Some(os), Some(app)) = (m.os, m.app()) else {
            continue;
        };
        for step in &app.steps {
            let (path, contents) = match step {
                Step::Run { script, .. } if has_extension(script.as_str(), "ps1") => (script.as_str(), powershell(blocks, name, m, os)),
                Step::Run { script, .. } => (script.as_str(), shell(blocks, name, m, os)),
                Step::Playbook(file) => (file.as_str(), playbook(name, PlaybookOn::Machine)),
                _ => continue,
            };
            add(files, path, contents);
        }
    }
    for p in controller {
        add(files, &p.path, playbook(&p.machine, PlaybookOn::Controller));
    }
}

/// Adds the starter at `path`, unless the path isn't one inside the project as Isoloom reads one
/// ([`RelPath::parse`]: relative, `/`-separated, no `..` segment), is Isoloom's (a name it reads
/// the spec from, [`isoloom_spec::SPEC_FILES`], or one its rule for its outputs takes,
/// [`ignore::OUTPUTS`]: `.isoloom/`, an instance's `.isoloom-<n>/`, any `.isoloom*` at the top,
/// which no copy of the project holds), or already has a file.
fn add(files: &mut Vec<ProjectFile>, path: &str, contents: String) {
    let isoloom_s = isoloom_spec::SPEC_FILES.contains(&path) || ignore::Rules::parse(ignore::OUTPUTS).ignored(path);
    let leaves = RelPath::parse(path).is_err();
    if !leaves && !isoloom_s && !files.iter().any(|s| s.path == path) {
        files.push(ProjectFile {
            path: path.to_owned(),
            contents,
        });
    }
}

/// The machine's ports, by number, each once (a TCP and a UDP port may share one).
fn ports(m: &Machine) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    for (port, _) in transports(m) {
        if !out.contains(&port) {
            out.push(port);
        }
    }
    out
}

/// The machine's ports, by number and transport (`TCP`, `UDP`), each once.
fn transports(m: &Machine) -> Vec<(u16, &'static str)> {
    let mut out: Vec<(u16, &'static str)> = Vec::new();
    for p in m.ports.values() {
        let Some(port) = p.port else { continue };
        let found = (port.get(), if p.protocol.is_tcp() { "TCP" } else { "UDP" });
        if !out.contains(&found) {
            out.push(found);
        }
    }
    out
}

/// The machine's addresses, as `10.10.1.12 (lan)`.
fn addresses(blocks: &Blocks, m: &Machine) -> Vec<String> {
    m.networks
        .iter()
        .filter_map(|(net, octets)| {
            let octet = match octets {
                Octets::One(o) => *o,
                Octets::Several(list) => *list.first()?,
            };
            let address = blocks.get(net)?.host(octet)?;
            Some(format!("{address} ({net})"))
        })
        .collect()
}

/// A Linux step's script: what it is, then the install, the configuration and the start.
fn shell(blocks: &Blocks, name: &str, m: &Machine, os: OsRef) -> String {
    let family = Family::of(os.os());
    let ports = ports(m);
    let wanted = family.map(|f| wanted(&ports, f)).unwrap_or_default();
    let mut lines = header(blocks, name, m, os);
    lines.insert(0, "#!/bin/sh".to_owned());
    if !ports.is_empty() {
        let list: Vec<String> = ports.iter().map(ToString::to_string).collect();
        lines.push(format!("# Services the project expects to answer: {}", list.join(", ")));
    }
    lines.push("set -eu".to_owned());
    lines.push(String::new());
    match family.filter(|_| !wanted.is_empty()) {
        None => lines.push("# Nothing to install for this machine's ports: add your own steps here.".to_owned()),
        Some(family) => {
            lines.push(install(&wanted, family));
            for p in wanted.iter().filter(|p| !p.configure.is_empty()) {
                lines.push(format!(
                    "\n# Port {}: listen on the project's networks, not only on localhost.\n{}",
                    p.port, p.configure
                ));
            }
            lines.push(String::new());
            lines.push(start(&wanted, family));
        }
    }
    let unknown: Vec<String> = ports
        .iter()
        .filter(|port| !PACKAGES.iter().any(|p| p.port == **port))
        .map(ToString::to_string)
        .collect();
    if !unknown.is_empty() {
        lines.push(format!("\n# To do: install what answers on port(s) {}.", unknown.join(", ")));
    }
    text(&lines)
}

/// Lines as a file's text, each ending with a line break.
fn text(lines: &[String]) -> String {
    let mut text = lines.iter().map(|l| l.trim_end_matches('\n')).collect::<Vec<_>>().join("\n");
    text.push('\n');
    text
}

/// A starter's first lines: what it is and the machine's addresses.
fn header(blocks: &Blocks, name: &str, m: &Machine, os: OsRef) -> Vec<String> {
    let mut lines = vec![format!(
        "# Starter set-up for `{name}` ({os}), written by Isoloom: edit it to build your project."
    )];
    lines.extend(addresses(blocks, m).iter().map(|a| format!("# Address: {a}")));
    lines
}

/// The TCP ports IIS answers on, once installed (443 once a certificate is bound).
const WEB_PORTS: [u16; 2] = [80, 443];

/// A Windows step's script: IIS for its web ports, a firewall rule for each other port, on its
/// own transport.
fn powershell(blocks: &Blocks, name: &str, m: &Machine, os: OsRef) -> String {
    let mut lines = header(blocks, name, m, os);
    lines.push("$ErrorActionPreference = 'Stop'".to_owned());
    lines.push(String::new());
    let ports = transports(m);
    let is_web = |(port, transport): &(u16, &str)| *transport == "TCP" && WEB_PORTS.contains(port);
    let web: Vec<String> = ports.iter().filter(|p| is_web(p)).map(|(port, _)| port.to_string()).collect();
    if !web.is_empty() {
        lines.push(format!("# A web server (IIS) for port {}.", web.join(" and ")));
        lines.push("Install-WindowsFeature -Name Web-Server -IncludeManagementTools".to_owned());
    }
    if web.iter().any(|p| p == "443") {
        lines.push("# Port 443: IIS answers HTTPS once a certificate is bound to its site.".to_owned());
    }
    for (p, transport) in ports.iter().filter(|p| !is_web(p)) {
        let lower = transport.to_lowercase();
        lines.push(format!("# To do: install what answers on port {p}/{lower}, then open it:"));
        lines.push(format!(
            "New-NetFirewallRule -DisplayName 'project-{p}-{lower}' -Direction Inbound -Protocol {transport} -LocalPort {p} -Action Allow"
        ));
    }
    if ports.is_empty() {
        lines.push("# Nothing to install for this machine's ports: add your own steps here.".to_owned());
    }
    text(&lines)
}

/// Where an Ansible step's playbook runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybookOn {
    /// On its machine, as root (`hosts: all`, `become`).
    Machine,
    /// From Isoloom's controller, over WinRM (`hosts: <machine>`; WinRM has no `become`).
    Controller,
}

/// The starter playbook that sets up `machine`, run `on` its machine or from the controller.
fn playbook(machine: &str, on: PlaybookOn) -> String {
    let (from, hosts, become_, ping) = match on {
        PlaybookOn::Machine => ("", "all", "  become: true\n", "ansible.builtin.ping"),
        PlaybookOn::Controller => (", run from Isoloom's controller over WinRM", machine, "", "ansible.windows.win_ping"),
    };
    format!(
        "# Starter playbook for `{machine}`{from}, written by Isoloom: edit it to build your project.\n- hosts: {hosts}\n{become_}  tasks:\n    - name: The machine answers\n      {ping}:\n"
    )
}

#[cfg(test)]
mod tests;
