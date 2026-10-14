//! The `proxmox` target: Terraform for the bpg/proxmox provider, in `.isoloom/proxmox/main.tf`,
//! rendered from the plan through the VM core.
//!
//! - The environment gets its own SDN simple zone, and a VNet per network. No SDN subnets: a
//!   subnet's gateway lives on the host bridge, which would route between networks.
//! - A router VM sits on the server's uplink bridge and on every network at its router address.
//!   It forwards between networks with the `reach` rules (nftables), and gives the machines the
//!   internet (NAT).
//! - Each machine is a VM from a cloud image, with its fixed address on each network and the
//!   router as its gateway. Its set-up is a cloud-init document (the core's cloud-init
//!   transport): the project written to /opt/isoloom, then its steps from the plan. After a
//!   `reboot` step, cloud-init restarts the machine and a unit runs the rest at the next boot.
//! - A Windows machine is a full clone of a template you give per OS, set up by cloudbase-init
//!   and reached by the controller over WinRM as `isoloom` (its `windows` module).
//! - A machine with a virtual TPM (`vm.tpm`, or Windows 11's own) has its state on the VM
//!   datastore (`tpm_state`).
//! - The connection, the node, the datastores and a `slot` (unique per environment on the
//!   server: SDN ids are short) are variables.
//! - Machines are reached through the router (`ssh -J isoloom@<address>`, the `isoloom` user
//!   with `ssh_public_key`): the `machines` output gives their addresses, and the check runners
//!   in `.isoloom/proxmox/checks/<position>.sh` are piped to them by `isoloom test proxmox`
//!   (the `checks` output says which runs where).

mod controller;
mod machine;
pub(crate) mod module;
mod outputs;
mod outside;
mod router;
mod windows;

pub use crate::vm::res;

use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::backend::{self, Backend, Files, RenderError};
use isoloom_render::hcl_builder::Module;
use isoloom_render::{GenerateError, GeneratedFile, OUTPUT_DIR, header};
use isoloom_spec::images::is_windows;
use isoloom_spec::model::Target;

use crate::vm::limits::Limits;
use crate::vm::module::project_lists;
use crate::vm::{checks, controller as core_controller, os, vms};

/// The target's folder under `.isoloom/`.
const DIR: &str = "proxmox";

/// What Proxmox can't host yet.
pub const LIMITS: Limits = Limits {
    arm64: Some("arm64 on Proxmox comes later (arm64 is uncommon on Proxmox hosts)"),
    gateways: Some("networks with a `gateway` machine on Proxmox come later"),
    playbook_checks: None,
    tc: None,
    ipv6: None,
};

/// Cloud images by OS name (Linux for now).
fn image_url(os: &str) -> Option<&'static str> {
    Some(match os {
        "debian-11" => "https://cloud.debian.org/images/cloud/bullseye/latest/debian-11-genericcloud-amd64.qcow2",
        "debian-12" => "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2",
        "debian-13" => "https://cloud.debian.org/images/cloud/trixie/latest/debian-13-genericcloud-amd64.qcow2",
        // End of life, still published. 14.04's cloud-init (0.7.5) is too old for the snippet.
        "ubuntu-16.04" => "https://cloud-images.ubuntu.com/xenial/current/xenial-server-cloudimg-amd64-disk1.img",
        "ubuntu-18.04" => "https://cloud-images.ubuntu.com/bionic/current/bionic-server-cloudimg-amd64.img",
        "ubuntu-20.04" => "https://cloud-images.ubuntu.com/focal/current/focal-server-cloudimg-amd64.img",
        "ubuntu-22.04" => "https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.img",
        "ubuntu-24.04" => "https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img",
        "rocky-9" => "https://dl.rockylinux.org/pub/rocky/9/images/x86_64/Rocky-9-GenericCloud-Base.latest.x86_64.qcow2",
        "almalinux-9" => "https://repo.almalinux.org/almalinux/9/cloud/x86_64/images/AlmaLinux-9-GenericCloud-latest.x86_64.qcow2",
        "centos-7" => "https://cloud.centos.org/centos/7/images/CentOS-7-x86_64-GenericCloud.qcow2",
        _ => return None,
    })
}

/// Why Proxmox can't run this environment, if it can't: its limits, then a Linux machine with
/// no cloud image, then what a Windows machine can't have here yet.
pub fn refusal(env: &Environment) -> Option<String> {
    LIMITS.refusal(env).or_else(|| {
        vms(env)
            .find_map(|(name, mp)| {
                let os = os(mp).unwrap_or_default();
                (!is_windows(os) && image_url(os).is_none())
                    .then(|| format!("machine `{name}`: no Proxmox image for `{os}` yet (Debian, Ubuntu, Rocky, AlmaLinux, CentOS 7, Windows from a template)"))
            })
            .or_else(|| windows::refusal(env))
    })
}

/// The `proxmox` backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProxmoxBackend;

impl Backend for ProxmoxBackend {
    fn target(&self) -> Target {
        Target::Proxmox
    }

    /// Environment-level provisioning (a controller VM) and the checks, from the VMs.
    fn capabilities(&self) -> Capabilities {
        Capabilities::of([
            Feature::EnvironmentProvisioning,
            Feature::PlaybookChecks,
            Feature::ExecWithoutContainer,
            // Gateway networks are refused by the target's own limit (`LIMITS.gateways`), with
            // its own reason: the plan's requirement names hybrid.
            Feature::Gateways,
            Feature::MachineAliases,
            Feature::Ulimits,
            Feature::TrafficControl,
            Feature::Ipv6,
            // An existing bridge (`external`), or the uplink or another bridge (`bridged`).
            Feature::ExternalNetworks,
            Feature::BridgedNetworks,
            Feature::Tpm,
            Feature::MachineProcess,
        ])
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        backend::check(Target::Proxmox, env, &self.capabilities())?;
        if let Some(what) = refusal(env) {
            return Err(unsupported(what));
        }
        let main = GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/main.tf"),
            contents: main(env).map_err(|e| unsupported(format!("the module couldn't be written: {e}")))?,
        };
        let mut files = vec![main];
        files.extend(checks::runners(env, DIR));
        Ok(files)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        isoloom_plan::refusal(&env.requirements, &self.capabilities())
            .map(|r| r.to_string())
            .or_else(|| refusal(env))
    }
}

fn unsupported(what: String) -> RenderError {
    RenderError::Generate(GenerateError::Unsupported { target: Target::Proxmox, what })
}

const USAGE: &str = "# Start:  terraform -chdir=.isoloom/proxmox init && terraform -chdir=.isoloom/proxmox apply -var proxmox_endpoint=https://<server>:8006/ -var proxmox_api_token=<user@realm!name=secret>\n# Stop:   terraform -chdir=.isoloom/proxmox destroy (same variables)\n";

/// The module.
fn main(env: &Environment) -> Result<String, isoloom_render::hcl_builder::Error> {
    let mut m = Module::new(format!("{}{USAGE}", header("#")));
    module::blocks(env, &mut m);
    router::blocks(env, &mut m);
    for (name, _) in vms(env) {
        machine::blocks(env, name, &mut m);
    }
    if core_controller::needed(env) {
        controller::blocks(env, &mut m);
    }
    outputs::blocks(env, &mut m);
    m.fragment(&project_lists(false));
    m.render()
}
