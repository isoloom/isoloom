//! The Terraform VM core: what every Terraform target that runs one VM per machine shares, built
//! once on the plan ([`Environment`]) with the HCL builder. Proxmox and every cloud of
//! `cloud-vm` (AWS, Azure, Google Cloud, DigitalOcean, Linode, Oracle Cloud) render through it.
//!
//! A target brings its own resources (networks, instances, images, firewall rules) and picks a
//! transport for the machines' set-up; the core brings the rest:
//!
//! - [`linux`]: a Linux machine set up over SSH by Terraform (`remote-exec`): the connection,
//!   the project's archive, the inputs file, then the machine's [`Step`]s from the plan, each
//!   written once by `isoloom_render::steps` and run as root. A `reboot` step splits the steps:
//!   Terraform restarts the machine between two `remote-exec` provisioners. A target adds its
//!   own commands through [`linux::Hooks`] (extra interfaces found by MAC, the auto-stop after a
//!   restart, published-port redirects) and leaves out the steps its platform does itself.
//! - [`cloud_init`]: the same steps as a cloud-init document (Proxmox): the `Prelude` as
//!   `bootcmd`, the others as `runcmd`, and after a `reboot` step a unit running the rest at the
//!   next boot.
//! - [`windows`]: a Windows machine over WinRM: its name, then one PowerShell script made of the
//!   steps' Windows renderings.
//! - [`controller`]: the controller VM's part (its key, its set-up, the playbooks) and the
//!   inventory.
//! - [`checks`]: the check runners and the `checks` output.
//! - [`cloud`]: what every cloud module repeats: the operator's variables, the environment's id
//!   and names, the tags, the auto-stop, extra interfaces brought up by MAC, published-port
//!   redirects, the Windows password and rules, and the outputs ([`cloud::outputs`], given how
//!   the cloud reaches its machines).
//! - [`module`]: the variables, the project lists and the outputs every module has.
//! - [`provisioner`]: one `terraform_data` resource running files and commands on a machine.
//!
//! To move a cloud onto the core: implement `isoloom_render::backend::Backend` for it (target
//! `cloud-vm`, `cloud_vm::render_cloud` and `cloud_vm::refuse_cloud` do the rest) and add it to
//! `cloud_vm::CLOUDS`; write its resources with `isoloom_render::hcl_builder`, and for each
//! machine call [`linux::provisioner`] (or [`windows::blocks`], with its [`windows::Platform`])
//! with how Terraform reaches it; its refusals come from [`limits`], computed from the plan,
//! then its own rules (images, reserved addresses).

pub mod checks;
pub mod cloud;
pub mod cloud_init;
pub mod controller;
pub mod limits;
pub mod linux;
pub mod module;
pub mod provisioner;
pub mod versions;
pub mod windows;

#[cfg(test)]
mod tests;

use isoloom_plan::{Environment, MachinePlan, MachineShape, Step};
use isoloom_render::steps;
use isoloom_spec::shell::quote;

/// A Terraform resource name from a machine, network or OS name.
pub fn res(s: &str) -> String {
    s.replace(['-', '.'], "_")
}

/// Whether the machine runs Windows.
pub fn is_windows(mp: &MachinePlan) -> bool {
    matches!(&mp.image, Some(isoloom_plan::Image::Vm { os }) if isoloom_spec::images::is_windows(os))
}

/// The machine's OS id (`debian-12`...), when it is a VM.
pub fn os(mp: &MachinePlan) -> Option<&str> {
    match &mp.image {
        Some(isoloom_plan::Image::Vm { os }) => Some(os),
        _ => None,
    }
}

/// The VMs of the environment, in start order.
pub fn vms(env: &Environment) -> impl Iterator<Item = (&str, &MachinePlan)> {
    env.start_order
        .iter()
        .filter_map(|n| env.machines.get_key_value(n))
        .filter(|(_, mp)| mp.shape == MachineShape::Vm)
        .map(|(n, mp)| (n.as_str(), mp))
}

/// Whether a step sets up something a restart undoes (interfaces and routes are not kept): a
/// transport runs it again after each `reboot` step.
pub fn at_every_boot(step: &Step) -> bool {
    matches!(step, Step::Trunks | Step::Routes { .. })
}

/// A step's Linux script as one command: `sh -c '<script>'` (as root: cloud-init), or with
/// `sudo` for a user that isn't root (`remote-exec`). `None` when the step has nothing to do.
pub fn step_command(env: &Environment, machine: &str, step: &Step, sudo: bool) -> Option<String> {
    steps::linux(env, machine, step).map(|sh| as_root(&sh, sudo))
}

/// A root script as one command.
pub fn as_root(script: &str, sudo: bool) -> String {
    format!("{}sh -c {}", if sudo { "sudo " } else { "" }, quote(script.trim_end_matches('\n')))
}
