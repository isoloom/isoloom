//! The one Ansible inventory, rendered from a planned [`Environment`] for every target that runs
//! playbooks: the controller's `/etc/isoloom/inventory.ini` on Vagrant, Proxmox and the clouds,
//! and `.isoloom/external/inventory.ini` on the `external` target.
//!
//! What is the same everywhere lives here: which machines are hosts ([`hosts`]), the `linux` and
//! `windows` groups, each host's `ansible_host` and its `host_vars`, `ansible_become` on Linux,
//! and the inventory groups (`provision[].groups`, then the spec's `groups:`, members limited to
//! the hosts). What differs per target is how a host is reached (users, keys, passwords, WinRM
//! or SSH on Windows), which the target says with an [`Access`], each backend its own
//! ([`Access::new`], from the shared pieces: [`winrm_vars`], [`WINRM_PATIENCE`],
//! [`WINDOWS_OVER_SSH`]).
//!
//! ```text
//! [linux]
//! <host> ansible_host=<address> <Access::host vars> <host_vars>
//!
//! [windows]
//! <host> ansible_host=<address> <Access::host vars> <host_vars>
//!
//! [linux:vars]
//! <Access::linux>
//! ansible_become=true
//!
//! [windows:vars]          when a Windows host exists and the access sets Windows variables
//! <Access::windows>
//!
//! [<group>]               each inventory group: its members that are hosts here
//! <host>
//! ```
//!
//! A backend calls `inventory::render(env, &access)` and writes the text where its
//! controller (or the user) runs Ansible; a header, if the file has one, is the caller's.

use std::fmt;

use indexmap::IndexMap;

use isoloom_plan::{Environment, MachinePlan, MachineShape};
use isoloom_spec::model::{Target, Winrm};

/// Variables, in the order they are written.
pub type Vars = Vec<(String, String)>;

/// The controller's own private key, which every Linux VM it provisions trusts.
pub const CONTROLLER_KEY: &str = isoloom_spec::linux_config_path!("/id_ed25519");

/// The controller's inventory, which its playbooks and checks read (as root).
pub const CONTROLLER_INVENTORY: &str = isoloom_spec::linux_config_path!("/inventory.ini");

/// One host of the inventory: a machine Ansible reaches on this target.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Host<'a> {
    /// The machine's name, the inventory hostname.
    pub name: &'a str,
    /// Its `ansible_host`: the existing host's address on `external`, else its first address.
    pub address: String,
    /// The machine as planned.
    pub machine: &'a MachinePlan,
}

/// How a target reaches its hosts.
#[non_exhaustive]
pub struct Access<'a> {
    /// `[linux:vars]`, before the `ansible_become=true` every Linux host gets.
    pub linux: Vars,
    /// `[windows:vars]`, written when a Windows host exists.
    pub windows: Vars,
    /// Each host's own connection variables, after its `ansible_host`.
    pub host: Box<dyn Fn(&Host<'_>) -> Vars + 'a>,
}

impl fmt::Debug for Access<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Access")
            .field("linux", &self.linux)
            .field("windows", &self.windows)
            .finish_non_exhaustive()
    }
}

impl<'a> Access<'a> {
    /// How a target reaches its hosts: `linux` and `windows` as its groups' variables, `host`
    /// each host's own. Each backend builds its own (the Vagrant boxes' account, the `isoloom`
    /// user of Isoloom's server and cloud VMs, an existing host's own SSH).
    #[must_use]
    pub fn new(linux: Vars, windows: Vars, host: impl Fn(&Host<'_>) -> Vars + 'a) -> Access<'a> {
        Access {
            linux,
            windows,
            host: Box::new(host),
        }
    }
}

/// `pairs` as [`Vars`], in their order.
#[must_use]
pub fn vars(pairs: &[(&str, &str)]) -> Vars {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
}

/// WinRM's certificate check off and its timeouts longer than Ansible's defaults: a domain
/// controller's promotion or a reboot keeps a call waiting. Every target reaching Windows over
/// WinRM sets them.
pub const WINRM_PATIENCE: [(&str, &str); 3] = [
    ("ansible_winrm_server_cert_validation", "ignore"),
    ("ansible_winrm_operation_timeout_sec", "400"),
    ("ansible_winrm_read_timeout_sec", "500"),
];

/// The WinRM port, scheme and transport of a Windows machine that answers `winrm` that way.
#[must_use]
pub fn winrm_vars(winrm: Winrm) -> Vars {
    match winrm {
        Winrm::Ssl => vars(&[("ansible_port", "5986"), ("ansible_winrm_scheme", "https"), ("ansible_winrm_transport", "ntlm")]),
        Winrm::Plaintext => vars(&[("ansible_port", "5985"), ("ansible_winrm_scheme", "http"), ("ansible_winrm_transport", "basic")]),
    }
}

/// A Windows host reached over OpenSSH, its commands run by PowerShell.
pub const WINDOWS_OVER_SSH: [(&str, &str); 2] = [("ansible_connection", "ssh"), ("ansible_shell_type", "powershell")];

/// The machines Ansible reaches on the environment's target, in spec order: on `external`, the
/// existing hosts (at their `external` address); elsewhere, the VMs (at their first address).
#[must_use]
pub fn hosts(env: &Environment) -> Vec<Host<'_>> {
    env.machines
        .iter()
        .filter_map(|(name, m)| {
            let address = if env.target == Target::External {
                m.external.as_ref()?.address.clone()
            } else if m.shape == MachineShape::Vm {
                m.addresses.values().next()?.to_string()
            } else {
                return None;
            };
            Some(Host { name, address, machine: m })
        })
        .collect()
}

/// The inventory of `env`, its hosts reached as `access` says.
#[must_use]
pub fn render(env: &Environment, access: &Access<'_>) -> String {
    let hosts = hosts(env);
    let (windows, linux): (Vec<&Host>, Vec<&Host>) = hosts.iter().partition(|h| h.machine.is_windows());
    let lines = |hs: &[&Host]| hs.iter().map(|h| host_line(h, access)).collect::<String>();
    let windows_vars = if windows.is_empty() || access.windows.is_empty() {
        String::new()
    } else {
        format!("\n[windows:vars]\n{}", group_vars(&access.windows))
    };
    let groups: String = env
        .inventory_groups
        .iter()
        .map(|(group, members)| {
            let here: Vec<&str> = members.iter().map(String::as_str).filter(|m| hosts.iter().any(|h| h.name == *m)).collect();
            format!("\n[{group}]\n{}\n", here.join("\n"))
        })
        .collect::<Vec<_>>()
        .concat();
    format!(
        "[linux]\n{}\n[windows]\n{}\n[linux:vars]\n{}ansible_become=true\n{windows_vars}{groups}",
        lines(&linux),
        lines(&windows),
        group_vars(&access.linux)
    )
}

/// `k=v` lines of a `[group:vars]` section (written as is: Ansible reads the rest of the line).
fn group_vars(vars: &Vars) -> String {
    vars.iter().map(|(k, v)| format!("{k}={v}\n")).collect::<Vec<_>>().concat()
}

/// A host's line: its name, `ansible_host`, the access's variables, then its `host_vars`.
fn host_line(h: &Host<'_>, access: &Access<'_>) -> String {
    let own: String = (access.host)(h)
        .iter()
        .map(|(k, v)| format!(" {k}={}", host_value(v)))
        .collect::<Vec<_>>()
        .concat();
    format!(
        "{} ansible_host={}{own}{}\n",
        h.name,
        host_value(&h.address),
        host_vars_suffix(&h.machine.host_vars)
    )
}

/// A value on a host line: Ansible splits the line like a shell, so a value with a space, a
/// quote or a comment character is single-quoted; a plain one is written as is.
#[must_use]
pub(crate) fn host_value(v: &str) -> String {
    let plain = !v.is_empty() && !v.chars().any(|c| c.is_whitespace() || matches!(c, '\'' | '"' | '\\' | '#' | ';'));
    if plain { v.to_owned() } else { single_quoted(v) }
}

/// Single-quoted for Ansible's shell-like split of a host line.
fn single_quoted(v: &str) -> String {
    format!("'{}'", v.replace('\'', "'\"'\"'"))
}

/// A value as a Python literal, which is how Ansible reads a host line's values: a string
/// quoted (so `"10"` stays a string), `True`, `False` and `None`, lists and dicts as Python
/// spells them.
#[must_use]
fn python_literal(v: &serde_json::Value) -> String {
    use serde_json::Value;
    match v {
        Value::Null => "None".into(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Number(n) => n.to_string(),
        Value::String(_) => v.to_string(),
        Value::Array(a) => format!("[{}]", a.iter().map(python_literal).collect::<Vec<_>>().join(", ")),
        Value::Object(o) => {
            let items: Vec<String> = o
                .iter()
                .map(|(k, item)| format!("{}: {}", Value::String(k.clone()), python_literal(item)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

/// The `host_vars` of one machine as ` key='value'` pairs, the way its inventory line ends.
#[must_use]
fn host_vars_suffix(host_vars: &IndexMap<String, serde_json::Value>) -> String {
    host_vars
        .iter()
        .map(|(k, v)| format!(" {k}={}", single_quoted(&python_literal(v))))
        .collect::<Vec<_>>()
        .concat()
}

#[cfg(test)]
mod tests;
