//! The one Ansible inventory, rendered from a planned [`Environment`] for every target that runs
//! playbooks: the controller's `/etc/isoloom/inventory.ini` on Vagrant, Proxmox and the clouds,
//! and `.isoloom/external/inventory.ini` on the `external` target.
//!
//! What is the same everywhere lives here: which machines are hosts ([`hosts`]), the `linux` and
//! `windows` groups, each host's `ansible_host` and its `host_vars`, `ansible_become` on Linux,
//! and the inventory groups (`provision[].groups`, then the spec's `groups:`, members limited to
//! the hosts). What differs per target is how a host is reached (users, keys, passwords, WinRM
//! or SSH on Windows), which the target says with an [`Access`]. The presets ([`Access::vagrant`],
//! [`Access::proxmox`], [`Access::cloud`], [`Access::external`]) are the targets' own.
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
//! A backend calls `inventory::render(env, &Access::vagrant())` and writes the text where its
//! controller (or the user) runs Ansible; a header, if the file has one, is the caller's.

use std::fmt;

use indexmap::IndexMap;

use isoloom_plan::{Environment, MachinePlan, MachineShape};
use isoloom_spec::model::{Target, Winrm};

/// Variables, in the order they are written.
pub type Vars = Vec<(String, String)>;

/// The controller's own private key, which every Linux VM it provisions trusts.
pub const CONTROLLER_KEY: &str = "/etc/isoloom/id_ed25519";

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

fn vars(pairs: &[(&str, &str)]) -> Vars {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
}

/// WinRM's certificate check off and its timeouts longer than Ansible's defaults: a domain
/// controller's promotion or a reboot keeps a call waiting.
const WINRM_PATIENCE: [(&str, &str); 3] = [
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

impl Access<'static> {
    /// Vagrant: the boxes' own `vagrant` account (password auth); WinRM set per box, so a lab
    /// can mix plain-HTTP and HTTPS Windows boxes, and a box reached over OpenSSH instead
    /// (`vm.image.winssh`) gets SSH with PowerShell on its line.
    #[must_use]
    pub fn vagrant() -> Access<'static> {
        let account = [("ansible_user", "vagrant"), ("ansible_password", "vagrant")];
        let mut windows = vars(&account);
        windows.extend(vars(&[("ansible_connection", "winrm")]));
        windows.extend(vars(&WINRM_PATIENCE));
        Access {
            linux: vars(&account),
            windows,
            host: Box::new(|h| {
                if h.machine.winssh {
                    vars(&WINDOWS_OVER_SSH)
                } else {
                    h.machine.winrm.map(winrm_vars).unwrap_or_default()
                }
            }),
        }
    }

    /// External: each existing host's SSH user, port and key as its `external:` gives them;
    /// Windows hosts over SSH with PowerShell (OpenSSH on Windows), not WinRM.
    #[must_use]
    pub fn external() -> Access<'static> {
        Access {
            linux: Vars::new(),
            windows: vars(&WINDOWS_OVER_SSH),
            host: Box::new(|h| external_vars(h.machine)),
        }
    }
}

/// A Windows host reached over OpenSSH, its commands run by PowerShell.
const WINDOWS_OVER_SSH: [(&str, &str); 2] = [("ansible_connection", "ssh"), ("ansible_shell_type", "powershell")];

/// How a Windows machine set up by Isoloom (on Proxmox and the clouds) is reached: as
/// `isoloom` with a generated password, over WinRM on plain HTTP.
fn isoloom_over_winrm(password: &str, transport: WinrmAuth) -> Vars {
    let mut windows = vars(&[("ansible_user", "isoloom"), ("ansible_password", password), ("ansible_connection", "winrm")]);
    windows.extend(match transport {
        WinrmAuth::Basic => winrm_vars(Winrm::Plaintext),
        WinrmAuth::Ntlm => vars(&[("ansible_port", "5985"), ("ansible_winrm_scheme", "http"), ("ansible_winrm_transport", "ntlm")]),
    });
    windows.extend(vars(&WINRM_PATIENCE));
    windows
}

/// How WinRM authenticates over plain HTTP: Basic where the image allows it, NTLM where it
/// doesn't (Azure's Windows images).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinrmAuth {
    /// Basic authentication (the user data allows it).
    Basic,
    /// NTLM.
    Ntlm,
}

impl<'a> Access<'a> {
    /// Proxmox: the `isoloom` user cloud-init creates on each Linux VM, with the controller's
    /// key; on a Windows VM, the `isoloom` administrator its set-up makes, whose password is
    /// `windows_password` (a Terraform expression, as the module writes it), over WinRM.
    #[must_use]
    pub fn proxmox(windows_password: &str) -> Access<'a> {
        Access {
            linux: vars(&[("ansible_ssh_private_key_file", CONTROLLER_KEY)]),
            windows: isoloom_over_winrm(windows_password, WinrmAuth::Basic),
            host: Box::new(|h| {
                if h.machine.is_windows() {
                    Vars::new()
                } else {
                    vars(&[("ansible_user", "isoloom")])
                }
            }),
        }
    }

    /// A cloud: each Linux VM as its image's user (`linux_user` of its OS) with the
    /// controller's key; Windows over plain-HTTP WinRM as `isoloom`, whose password is
    /// `windows_password` (a Terraform expression, as the module writes it), authenticated as
    /// the cloud's images allow (`auth`, as Terraform's own connection does).
    #[must_use]
    pub fn cloud(linux_user: impl Fn(&str) -> String + 'a, windows_password: &str, auth: WinrmAuth) -> Access<'a> {
        Access {
            linux: vars(&[("ansible_ssh_private_key_file", CONTROLLER_KEY)]),
            windows: isoloom_over_winrm(windows_password, auth),
            host: Box::new(move |h| match h.machine.os.as_deref() {
                Some(os) if !h.machine.is_windows() => vec![("ansible_user".to_owned(), linux_user(os))],
                _ => Vars::new(),
            }),
        }
    }
}

/// An existing host's `ansible_user`, `ansible_port` and key: the ones its `external:` sets.
fn external_vars(m: &MachinePlan) -> Vars {
    let Some(e) = &m.external else { return Vars::new() };
    let mut out = Vars::new();
    if let Some(u) = &e.user {
        out.push(("ansible_user".into(), u.clone()));
    }
    if let Some(p) = e.port {
        out.push(("ansible_port".into(), p.to_string()));
    }
    if let Some(k) = &e.key {
        out.push(("ansible_ssh_private_key_file".into(), k.clone()));
    }
    out
}

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
pub fn host_value(v: &str) -> String {
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
pub fn python_literal(v: &serde_json::Value) -> String {
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
pub fn host_vars_suffix(host_vars: &IndexMap<String, serde_json::Value>) -> String {
    host_vars
        .iter()
        .map(|(k, v)| format!(" {k}={}", single_quoted(&python_literal(v))))
        .collect::<Vec<_>>()
        .concat()
}

#[cfg(test)]
mod tests;
