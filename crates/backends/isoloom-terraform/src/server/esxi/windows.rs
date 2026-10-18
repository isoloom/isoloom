//! A Windows machine on ESXi: a VM from the OVA given for its OS in `images` (VMware Tools and
//! cloudbase-init installed, cloudbase-init reading VMware's guestinfo, then generalized with
//! sysprep), with the guest OS type ESXi tunes the VM for (`guestos`). Its set-up is Proxmox's:
//! one PowerShell script cloudbase-init runs at the first boot (the `isoloom` administrator,
//! its name, the project, its services' openings, its steps, and last WinRM over HTTP), from
//! the user data in `guestinfo`. Terraform's WinRM can't go through the router, so the
//! controller waits for its ready marker over WinRM before the playbooks, and `isoloom run`
//! delivers its secrets through an SSH tunnel by the router, as on Proxmox.

use isoloom_plan::Environment;

use crate::machine::cloudbase_init;

/// The guest OS type ESXi gives a VM of Windows `os` (VMware's names: Windows Server 2022 is
/// the one after 2019, 2025 the one after 2022).
pub(super) fn guestos(os: &str) -> &'static str {
    match os {
        "windows-server-2008r2" => "windows7srv-64",
        "windows-server-2012r2" => "windows8srv-64",
        "windows-server-2016" => "windows9srv-64",
        "windows-server-2019" => "windows2019srv-64",
        "windows-server-2025" => "windows2022srvnext-64",
        "windows-10" => "windows9-64",
        "windows-11" => "windows11-64",
        _ => "windows2019srvnext-64",
    }
}

/// What ESXi can't give a Windows machine yet: cloudbase-init's limits, as on Proxmox.
pub(super) fn refusal(env: &Environment) -> Option<String> {
    cloudbase_init::refusal(env, "ESXi")
}

#[cfg(test)]
mod tests {
    #[test]
    fn each_windows_gets_vmware_s_guest_os_type() {
        assert_eq!(super::guestos("windows-server-2019"), "windows2019srv-64");
        assert_eq!(super::guestos("windows-server-2022"), "windows2019srvnext-64");
        assert_eq!(super::guestos("windows-11"), "windows11-64");
    }
}
