//! Azure's `cloud-vm` module, `.isoloom/cloud-vm/azure/main.tf`, rendered from the plan through
//! the VM core: the AWS model on the `azurerm` provider.
//!
//! - One resource group and virtual network holding every network as a subnet with its exact
//!   range. The virtual network routes between subnets (the machines' `Routes` and `Trunks`
//!   steps are left out); a network security group per machine does what the router does on
//!   other targets: a machine's networks reach it, `reach` rules open one network to another
//!   (on their ports), the rest is closed.
//! - Each machine is a VM at its address with a static public IP, set up over SSH as `isoloom`
//!   (Azure creates the administrator; it reserves `admin`) through the core's `remote-exec`
//!   transport. A machine on several networks gets an interface on each (forwarding on, so
//!   replies leave from the right one), brought up by MAC at every boot.
//! - A network a machine routes (`gateway:`): a route table per network sends it through that
//!   machine's address (`routing`), the machine at the router's address (Azure keeps .1).
//! - Windows (Azure's Server images): the `isoloom` administrator with a generated password,
//!   its computer name and a WinRM listener set at creation, then the core's WinRM set-up
//!   (NTLM); the auto-stop is a scheduled task, as Azure runs no user data on Windows.
//! - Environment-level `provision:`: the core's controller, a Debian VM on every network.

mod machines;
mod network;
mod routing;

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::GeneratedFile;
use isoloom_render::hcl_builder::{Expression, reference};
use isoloom_spec::model::Target;
use isoloom_spec::validate::Cidr;

use super::{base, variables};
use crate::Result;
use crate::cloud::vm::gpus::GpuSizes;
use crate::cloud::vm::outputs::{self, Reach};
use crate::cloud::vm::rules::{Images, Mtu, Networks, PLAYBOOK_CHECKS, Rules, Volumes, kept_address};
use crate::cloud::vm::{self, gateways, own_image, windows};
use crate::cloud::{Cloud, operator};
use crate::limits::Limits;
use crate::module::{self, ProjectCopy, Usage, ident, providers};

/// Azure creates the administrator with the VM: a name it never reserves, on every machine.
const USER: &str = "isoloom";

/// What Azure takes.
const RULES: Rules = Rules {
    cloud: Cloud::Azure,
    networks: Networks::Several,
    limits: Limits {
        arm64: Some("arm64 on Azure comes later (Arm-based sizes and arm64 images)"),
        gateways: None,
        playbook_checks: Some(PLAYBOOK_CHECKS),
        tc: Some("link impairment (`tc`) needs a router in the path; the cloud's security groups have none"),
        ipv6: None,
    },
    // NVIDIA T4s (NCasT4_v3). Subscriptions get no quota for them until they ask.
    gpus: GpuSizes {
        gpu: "NVIDIA T4",
        sizes: &[(1, "Standard_NC4as_T4_v3"), (4, "Standard_NC64as_T4_v3")],
        none: "",
    },
    nests: false,
    mtu: Mtu::Fixed,
    picks_ipv6: false,
    volumes: Volumes { smallest_gb: 1, most: 2 },
    tpm: true,
    images: Images {
        label: "Azure",
        has: |os| image(os).is_some(),
        list: "Debian 12/13, Ubuntu 22.04/24.04, Windows Server 2016 to 2025",
        own: own_image::azure,
    },
};

/// How the module writes its machines.
const MACHINES: vm::Machines = vm::Machines {
    linux: machines::linux,
    windows: machines::windows,
    controller: machines::controller,
};

/// The Azure Marketplace image for an OS: (publisher, offer, sku).
fn image(os: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match os {
        "debian-12" => ("Debian", "debian-12", "12-gen2"),
        "debian-13" => ("Debian", "debian-13", "13-gen2"),
        "ubuntu-22.04" => ("Canonical", "0001-com-ubuntu-server-jammy", "22_04-lts-gen2"),
        "ubuntu-24.04" => ("Canonical", "ubuntu-24_04-lts", "server-gen1"),
        "windows-server-2016" => ("MicrosoftWindowsServer", "WindowsServer", "2016-datacenter-gensecond"),
        "windows-server-2019" => ("MicrosoftWindowsServer", "WindowsServer", "2019-datacenter-gensecond"),
        "windows-server-2022" => ("MicrosoftWindowsServer", "WindowsServer", "2022-datacenter-azure-edition"),
        "windows-server-2025" => ("MicrosoftWindowsServer", "WindowsServer", "2025-datacenter-azure-edition"),
        _ => return None,
    })
}

/// The VM size for a machine: its GPUs' size when it asks for GPUs, else by its memory.
fn machine_size(mp: &MachinePlan, memory_mb: u32) -> &'static str {
    RULES.gpus.of(mp).unwrap_or(size(memory_mb))
}

/// The VM size for a machine's memory: B-series burstable, which covers the lab sizes.
fn size(memory_mb: u32) -> &'static str {
    match memory_mb {
        0..=1024 => "Standard_B1s",
        1025..=2048 => "Standard_B1ms",
        2049..=4096 => "Standard_B2s",
        4097..=8192 => "Standard_B2ms",
        _ => "Standard_B4ms",
    }
}

/// Why Azure can't run this environment, if it can't: its features, its address rules, its
/// images, then what no cloud does yet for a Windows machine.
pub(crate) fn refusal(env: &Environment) -> Option<String> {
    let spec = env.spec();
    if let Some(why) = RULES.features(env) {
        return Some(why);
    }
    if env.networks.values().filter_map(|n| Cidr::parse(&n.cidr)).any(|c| c.len > 29) {
        return Some("Azure subnets are /29 or larger".to_owned());
    }
    let machine = spec.machines.iter().find_map(|(name, m)| {
        let vm = m.vm.as_ref()?;
        RULES
            .images
            .missing(name, &vm.os, own_image::of_machine(m, RULES.images.own))
            .or_else(|| windows::refusal(name, m))
            .or_else(|| {
                kept_address(env, name, m, 0..=3).map(|net| format!("machine `{name}`: Azure keeps the first addresses of a subnet (.1 to .3) on `{net}`"))
            })
    });
    machine.or_else(|| windows::udp_redirect(spec))
}

/// The module.
pub(crate) fn module(env: &Environment) -> Result<GeneratedFile> {
    let env = &*gateways::relocated(env);
    let folder = format!("{}/{}", Target::CloudVm.id(), Cloud::Azure.id());
    let mut m = module::start(&Usage::with_key(&folder, operator::ALLOWED));
    m.block(providers::terraform(&vm::providers(
        env,
        &[providers::AZURERM, providers::LOCAL],
        &[providers::RANDOM],
    )));
    m.blocks(variables());
    m.blocks(operator::variables(env));
    m.blocks(base(env));
    network::blocks(env, &mut m);
    routing::blocks(env, &mut m);
    vm::private_dns::azure(env, &mut m);
    vm::credentials(env, &mut m);
    MACHINES.write(env, &mut m);
    let reach = Reach {
        public_ip: &|n| public_ip(&ident(n)),
        user: &|_| USER.to_owned(),
        controller: Some((public_ip("isoloom_controller"), USER)),
    };
    outputs::write(env, &reach, &mut m);
    module::file(Target::CloudVm, &folder, m, ProjectCopy::Archive)
}

/// The address Terraform reaches a machine at (`id`: its resource name).
fn public_ip(id: &str) -> Expression {
    reference(&format!("azurerm_public_ip.{id}.ip_address"))
}
