//! The matrix's graphical consoles: what each VM target gives a Linux VM and a Windows VM that
//! opens Remote Desktop's port, read from the run manifest the target writes for a small lab
//! with both (`machines[].consoles`), so it says no more than the generators do.

use isoloom_manifest::{ConsoleProtocol, ConsoleSource, Manifest, Provider};
use isoloom_plan::Lab;
use isoloom_spec::model::Target;

use crate::decision::Column;

/// The environment each column generates for a Linux VM: one alone.
const LINUX: &str = "version: 1
name: consoles
networks: { lab: { cidr: 10.20.0.0/24 } }
machines:
  linux: { networks: { lab: 10 }, os: debian@12 }
";

/// The environment for a Windows VM: one opening 3389/tcp, beside a Linux one (what a tunnel goes
/// through).
const WINDOWS: &str = "version: 1
name: consoles
networks: { lab: { cidr: 10.20.0.0/24 } }
machines:
  linux: { networks: { lab: 10 }, os: debian@12 }
  windows: { networks: { lab: 20 }, os: windows-server@2022, ports: { rdp: { port: 3389 } } }
";

/// What one column gives each kind of VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConsoleRow {
    /// The column.
    pub(crate) column: Column,
    /// A Linux VM's console, in a few words.
    pub(crate) linux: String,
    /// A Windows VM's that opens Remote Desktop's port.
    pub(crate) windows: String,
}

/// A row per column whose target runs VMs and writes a run manifest; `–` where the column
/// doesn't take the kind of VM (a cloud whose images have no Windows).
pub(crate) fn rows(columns: &[Column]) -> Vec<ConsoleRow> {
    let (linux, windows) = (isoloom_spec::parse(LINUX).map(Lab::new).ok(), isoloom_spec::parse(WINDOWS).map(Lab::new).ok());
    let shown = |c: &&Column| matches!(c.target, Target::Vagrant | Target::Proxmox | Target::Esxi | Target::CloudVm);
    let of = |lab: Option<&Lab>, c: &Column, machine: &str| lab.and_then(|lab| manifest_of(lab, c)).map_or_else(|| "–".to_owned(), |m| said(&m, machine, c));
    columns
        .iter()
        .filter(shown)
        .map(|c| ConsoleRow {
            column: *c,
            linux: of(linux.as_ref(), c, "linux"),
            windows: of(windows.as_ref(), c, "windows"),
        })
        .collect()
}

/// The run manifest `column` writes for `lab`, when it generates that environment.
fn manifest_of(lab: &Lab, column: &Column) -> Option<Manifest> {
    let files = crate::generate(lab, column.target).ok()?;
    let path = match (column.target, column.variant) {
        (Target::CloudVm, Some(cloud)) => format!(".isoloom/cloud-vm/{cloud}/run.json"),
        (target, _) => format!(".isoloom/{}/run.json", target.id()),
    };
    let file = files.iter().find(|f| f.path == path)?;
    isoloom_manifest::parse(&file.contents).ok()
}

/// What the manifest says of `machine`'s console under the column's provider, in a few words.
fn said(m: &Manifest, machine: &str, column: &Column) -> String {
    let provider = (column.target == Target::Vagrant)
        .then_some(column.variant)
        .flatten()
        .and_then(|p| serde_json::from_value::<Provider>(serde_json::Value::String(p.to_owned())).ok());
    let Some(console) = m.machines.iter().find(|x| x.name.as_str() == machine).and_then(|x| x.console(provider)) else {
        return "–".to_owned();
    };
    let how = match (&console.protocol, &console.source) {
        (ConsoleProtocol::None, _) | (_, None) => return format!("none: {}", console.note.as_deref().unwrap_or_default()),
        (_, Some(ConsoleSource::Libvirt {})) => "VNC, libvirt's display",
        (_, Some(ConsoleSource::QemuMonitor {})) => "VNC, QEMU's display",
        (_, Some(ConsoleSource::Runner {})) => "VNC, the hypervisor's server (its port and password the runner's, per run)",
        (_, Some(ConsoleSource::Vrde {})) => "RDP, VirtualBox's remote display (with the Oracle extension pack)",
        (_, Some(ConsoleSource::ProxmoxApi { .. })) => "VNC, Proxmox's console through its API",
        (ConsoleProtocol::Rdp, Some(ConsoleSource::SshTunnel { .. })) => "RDP, its own Remote Desktop through an SSH tunnel",
        (_, Some(ConsoleSource::SshTunnel { .. })) => "VNC through an SSH tunnel",
    };
    how.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_vm_target_says_what_console_a_vm_gets() {
        let rows = rows(&crate::decision::columns());
        let row = |id: &str| rows.iter().find(|r| r.column.id() == id).unwrap_or_else(|| panic!("{id}"));
        assert_eq!(row("vagrant/libvirt").linux, "VNC, libvirt's display");
        assert_eq!(row("vagrant/qemu").windows, "VNC, QEMU's display");
        assert!(row("vagrant/virtualbox").linux.starts_with("RDP, VirtualBox's"));
        assert_eq!(row("proxmox").linux, "VNC, Proxmox's console through its API");
        assert!(row("esxi").windows.starts_with("none: "));
        assert!(row("cloud-vm/aws").linux.starts_with("none: a Linux cloud VM"));
        assert_eq!(row("cloud-vm/aws").windows, "RDP, its own Remote Desktop through an SSH tunnel");
        assert!(
            row("cloud-vm/linode").linux.starts_with("none: a Linux cloud VM"),
            "every cloud takes a Linux VM"
        );
        assert!(!rows.iter().any(|r| r.column.target == Target::Docker), "no VM, no console");
    }
}
