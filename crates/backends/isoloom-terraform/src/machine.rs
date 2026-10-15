//! The lab's VMs and how a module sets them up, built once on the plan ([`Environment`]) with
//! the HCL builder. Every VM target renders through it: Proxmox, ESXi, and each cloud of
//! `cloud-vm`. A target brings its own resources (networks, instances, images, firewall
//! rules) and picks a transport for the machines' set-up; this module brings the rest:
//!
//! - [`linux`]: a Linux machine set up over SSH by Terraform (`remote-exec`): the connection,
//!   the project's archive, the inputs file, then the machine's [`Step`]s from the plan, each
//!   written once by `isoloom_render::steps` and run as root. A `reboot` step splits the steps:
//!   Terraform restarts the machine between two `remote-exec` provisioners. A target adds its
//!   own commands through [`linux::Hooks`] and leaves out the steps its platform does itself.
//! - [`cloud_init`]: the same steps as a cloud-init document (Proxmox, ESXi): the `Prelude` as
//!   `bootcmd`, the others as `runcmd`, and after a `reboot` step a unit running the rest at
//!   the next boot.
//! - [`windows`]: a Windows machine over WinRM: its name, then one PowerShell script made of
//!   the steps' Windows renderings.
//! - [`cloudbase_init`]: a Windows machine set up at its first boot by cloudbase-init, from one
//!   PowerShell script (Proxmox, ESXi).
//! - [`controller`]: the controller VM's part (its key, its set-up, the playbooks) and the
//!   inventory.
//! - [`provisioner`]: one `terraform_data` resource running files and commands on a machine.

pub(crate) mod cloud_init;
pub(crate) mod cloudbase_init;
pub(crate) mod controller;
pub(crate) mod linux;
pub(crate) mod provisioner;
pub(crate) mod windows;

#[cfg(test)]
mod tests;

use isoloom_plan::{Environment, Feature, MachinePlan, MachineShape, Step};
use isoloom_render::hcl_builder::Expression;
use isoloom_render::steps;
use isoloom_spec::shell::quote;

/// What every VM target renders through this core; a target refuses one on its own, with its
/// own reason, where its platform can't give it.
///
/// - Environment-level provisioning: the [`controller`] runs the playbooks; playbook checks run
///   there too, `exec` checks on the machine.
/// - The set-up steps: machine aliases, ulimits, a machine's own process, tmpfs modes, its
///   further addresses (on the interface of its primary address), IPv6, link impairment (`tc`)
///   and the networks a machine routes (`gateway:`, refused by a target's limits where its
///   network can't).
/// - Secrets, owners included, delivered by `isoloom run` over the machine's own connection (SSH,
///   or WinRM on Windows).
/// - The private zone's names: the platform's DNS where it has one, else each VM's hosts file
///   under the zone's domain, its search domain.
/// - A network's MTU, where the platform sets one, and data disks (raw ones left unformatted).
pub(crate) const FEATURES: [Feature; 17] = [
    Feature::EnvironmentProvisioning,
    Feature::PlaybookChecks,
    Feature::ExecWithoutContainer,
    Feature::MachineAliases,
    Feature::Ulimits,
    Feature::MachineProcess,
    Feature::TmpfsModes,
    Feature::VmMoreAddresses,
    Feature::Ipv6,
    Feature::TrafficControl,
    Feature::Gateways,
    Feature::Secrets,
    Feature::SecretOwners,
    Feature::PrivateDns,
    Feature::NetworkMtu,
    Feature::DataDisks,
    Feature::RawDisks,
];

/// The VMs of the environment, in start order.
pub(crate) fn vms(env: &Environment) -> impl Iterator<Item = (&str, &MachinePlan)> {
    env.start_order
        .iter()
        .filter_map(|n| env.machines.get_key_value(n))
        .filter(|(_, mp)| mp.shape == MachineShape::Vm)
        .map(|(n, mp)| (n.as_str(), mp))
}

/// Whether a step sets up something a restart undoes (interfaces and routes are not kept): a
/// transport runs it again after each `reboot` step.
pub(crate) fn at_every_boot(step: &Step) -> bool {
    matches!(step, Step::Trunks | Step::MoreAddresses { .. } | Step::Routes { .. } | Step::Ipv6 { .. })
}

/// A machine as Terraform reaches it to set it up, whatever the transport.
#[derive(Debug, Clone)]
pub(crate) struct Remote<'a> {
    /// The plan.
    pub(crate) env: &'a Environment,
    /// The machine's name.
    pub(crate) name: &'a str,
    /// Its plan.
    pub(crate) plan: &'a MachinePlan,
    /// What replaces its set-up when it changes (the instance's id).
    pub(crate) replace_on: Expression,
    /// Its address, from where Terraform runs.
    pub(crate) host: Expression,
}

/// How a root script runs on a machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shell {
    /// As root already (cloud-init's commands).
    Root,
    /// As a user that isn't root, through `sudo` (Terraform's `remote-exec` over SSH).
    Sudo,
}

impl Shell {
    /// A root script as one command: `sh -c '<script>'`, through `sudo` when it must.
    pub(crate) fn command(self, script: &str) -> String {
        let sudo = match self {
            Shell::Root => "",
            Shell::Sudo => "sudo ",
        };
        format!("{sudo}sh -c {}", quote(script.trim_end_matches('\n')))
    }

    /// A step's Linux script as one command. `None` when the step has nothing to do.
    pub(crate) fn step(self, env: &Environment, machine: &str, step: &Step) -> Option<String> {
        steps::linux(env, machine, step).map(|sh| self.command(&sh))
    }
}
