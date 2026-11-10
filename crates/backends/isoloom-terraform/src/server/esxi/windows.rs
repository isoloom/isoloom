//! A Windows machine on ESXi: a VM from the OVA given for its OS in `images` (VMware Tools and
//! cloudbase-init installed, cloudbase-init reading VMware's guestinfo, then generalized with
//! sysprep), with the guest OS type ESXi tunes the VM for (`isoloom_render::esxi::guest_os`). Its set-up is Proxmox's:
//! one PowerShell script cloudbase-init runs at the first boot (the `isoloom` administrator,
//! its name, the project, its services' openings, its steps, and last WinRM over HTTP), from
//! the user data in `guestinfo`. Terraform's WinRM can't go through the router, so the
//! controller waits for its ready marker over WinRM before the playbooks, and the runner
//! delivers its secrets through an SSH tunnel by the router, as on Proxmox.

use isoloom_plan::Environment;

use crate::machine::cloudbase_init;

/// What ESXi can't give a Windows machine: cloudbase-init's limits, as on Proxmox.
pub(super) fn refusal(env: &Environment) -> Option<String> {
    cloudbase_init::refusal(env, "ESXi")
}
