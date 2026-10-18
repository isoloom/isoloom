//! A Windows machine on Proxmox. Proxmox has no Windows cloud image: each Windows OS a spec uses
//! is a template you make once (installed from Microsoft's ISO with the VirtIO drivers and
//! cloudbase-init, UEFI on a Q35 machine, its system disk on `virtio0`, then generalized with
//! sysprep), named by its VM id in a variable (`template_windows_server_2022`). Each machine is
//! a full clone of it, with a virtual TPM when it gets one, set up by cloudbase-init from its
//! configuration drive (`crate::machine::cloudbase_init`).

use isoloom_plan::{Environment, MachinePlan};
use isoloom_spec::images::is_windows;

use super::machine::Boot;
use crate::machine::vms;
use crate::module::ident;
use crate::module::variable::Variable;

/// The variable holding the VM id of `os`'s template.
fn template_variable(os: &str) -> String {
    format!("template_{}", ident(os))
}

/// One variable per Windows OS the machines use: its template's VM id.
pub(crate) fn variables(env: &Environment) -> Vec<Variable> {
    let mut oses: Vec<&str> = vms(env).filter_map(|(_, mp)| mp.os.as_deref()).filter(|o| is_windows(o)).collect();
    oses.sort_unstable();
    oses.dedup();
    oses.into_iter()
        .map(|o| {
            Variable::new(&template_variable(o), "number").describe(&format!(
                "The VM id of a `{o}` template: cloudbase-init and the VirtIO drivers installed, UEFI on Q35, its disk on virtio0, sysprepped"
            ))
        })
        .collect()
}

/// The OS type Proxmox tunes a Windows VM for.
fn ostype(os: &str) -> &'static str {
    match os {
        "windows-server-2008r2" => "win7",
        "windows-server-2012r2" => "win8",
        "windows-10" | "windows-server-2016" | "windows-server-2019" => "win10",
        _ => "win11",
    }
}

/// A Windows machine's VM boots a full clone of its OS's template.
pub(crate) fn boot(mp: &MachinePlan) -> Boot {
    let o = mp.os.as_deref().unwrap_or_default();
    Boot::Template {
        variable: template_variable(o),
        ostype: ostype(o),
        disk: mp.size.disk_gb,
    }
}
