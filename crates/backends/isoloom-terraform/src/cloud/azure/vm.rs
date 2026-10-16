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
mod private_dns;
mod routing;

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{Expression, Module, reference};

use super::{provider, variables};
use crate::cloud::vm::rules::{FIRST_THREE, Images, Mtu, Networks, PLAYBOOK_CHECKS, Rules, Subnets, Volumes};
use crate::cloud::vm::{
    self, Driver, gateways,
    gpus::GpuSizes,
    outputs::{self, Reach},
    own_image, windows,
};
use crate::cloud::{Cloud, operator};
use crate::limits::Limits;
use crate::machine::controller;
use crate::module::{Usage, ident, providers};

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
    subnets: Some(Subnets {
        called: "subnets",
        longest: 29,
    }),
    kept: Some(FIRST_THREE),
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

/// The VM size for a machine: its GPUs' size when it asks for GPUs, else by its memory and its
/// interfaces (one per network).
fn machine_size(mp: &MachinePlan, memory_mb: u32) -> &'static str {
    RULES.gpus.of(mp).unwrap_or(size(memory_mb, mp.addresses.len()))
}

/// The sizes Isoloom picks, cheapest first: (size, memory in MB, network interfaces at most).
///
/// Burstable Bsv2, the current series replacing B-series v1 (`Standard_B1s`, `B2s` and the rest:
/// retiring on 15 November 2028, and since July 2026 not deployable from new subscriptions):
/// <https://learn.microsoft.com/azure/virtual-machines/sizes/lifecycle/retirements-and-capacity-restrictions>
/// and <https://learn.microsoft.com/azure/virtual-machines/sizes/lifecycle/retirement/retired-sizes-modernization-guide>.
/// Bsv2 takes two interfaces up to 8 vCPUs
/// (<https://learn.microsoft.com/azure/virtual-machines/sizes/general-purpose/bsv2-series>), so a
/// machine on more networks gets a Dasv5 size: 4 interfaces from 8 vCPUs, 8 from 16
/// (<https://learn.microsoft.com/azure/virtual-machines/sizes/general-purpose/dasv5-series>).
/// Both series boot generation 1 and 2 images on SCSI disks, as every image Isoloom names does.
const SIZES: [(&str, u32, usize); 8] = [
    ("Standard_B2ts_v2", 1024, 2),
    ("Standard_B2ls_v2", 4096, 2),
    ("Standard_B2s_v2", 8192, 2),
    ("Standard_B4s_v2", 16384, 2),
    ("Standard_B8s_v2", 32768, 2),
    ("Standard_D8as_v5", 32768, 4),
    ("Standard_D16as_v5", 65536, 8),
    ("Standard_D32as_v5", 131_072, 8),
];

/// The network interfaces an Azure VM of [`SIZES`] takes at most.
const MOST_INTERFACES: usize = 8;

/// The cheapest size with `memory_mb` and `interfaces`; the largest when none has enough.
fn size(memory_mb: u32, interfaces: usize) -> &'static str {
    let [.., largest] = SIZES;
    SIZES
        .iter()
        .find(|(_, memory, most)| *memory >= memory_mb && *most >= interfaces)
        .unwrap_or(&largest)
        .0
}

/// Why Azure can't run this environment, if it can't: its features, its address rules, its
/// images, then what no cloud does yet for a Windows machine.
fn refusal(env: &Environment) -> Option<String> {
    let spec = env.spec();
    let machine = || {
        spec.machines.iter().find_map(|(name, m)| {
            m.vm.as_ref()?;
            RULES.image(name, m).or_else(|| windows::refusal(name, m)).or_else(|| RULES.kept(env, name, m))
        })
    };
    RULES
        .features(env)
        .or_else(|| RULES.subnets(env))
        .or_else(|| interfaces(env))
        .or_else(machine)
        .or_else(|| windows::udp_redirect(spec))
}

/// Why a VM can't have an interface on each of its networks, if one can't: a machine, or the
/// controller (on every network), on more than [`MOST_INTERFACES`].
fn interfaces(env: &Environment) -> Option<String> {
    let most = MOST_INTERFACES;
    let machine = env.machines.iter().find(|(_, mp)| mp.addresses.len() > most);
    if let Some((name, _)) = machine {
        return Some(format!(
            "machine `{name}`: an Azure VM takes {most} network interfaces at most, one per network"
        ));
    }
    (controller::needed(env) && env.networks.len() > most)
        .then(|| format!("the controller joins every network: an Azure VM takes {most} network interfaces at most"))
}

/// Its `cloud-vm` driver.
pub(crate) const DRIVER: Driver = Driver {
    usage: |folder| Usage::with_key(folder, operator::ALLOWED),
    requires: (&[providers::AZURERM, providers::LOCAL], &[providers::RANDOM]),
    variables: || variables().into(),
    provider,
    refusal,
    write,
};

/// The module's blocks after the providers, from the plan.
fn write(env: &Environment, m: &mut Module) {
    let env = &*gateways::relocated(env);
    network::blocks(env, m);
    routing::blocks(env, m);
    private_dns::write(env, m);
    vm::credentials(env, m);
    MACHINES.write(env, m);
    let reach = Reach {
        public_ip: &|n| public_ip(&ident(n)),
        user: &|_| USER.to_owned(),
        controller: Some((public_ip("isoloom_controller"), USER)),
    };
    outputs::write(env, &reach, m);
}

/// The address Terraform reaches a machine at (`id`: its resource name).
fn public_ip(id: &str) -> Expression {
    reference(&format!("azurerm_public_ip.{id}.ip_address"))
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;

    #[test]
    fn a_machine_gets_the_cheapest_current_size_with_its_memory_and_interfaces() {
        assert_eq!(size(512, 1), "Standard_B2ts_v2");
        assert_eq!(size(2048, 2), "Standard_B2ls_v2");
        assert_eq!(size(8192, 1), "Standard_B2s_v2");
        assert_eq!(size(1024, 3), "Standard_D8as_v5");
        assert_eq!(size(40_000, 5), "Standard_D16as_v5");
        assert_eq!(size(1_000_000, 1), "Standard_D32as_v5");
        assert!(
            SIZES
                .iter()
                .all(|(name, _, _)| name.starts_with("Standard_B") && name.ends_with("_v2") || name.ends_with("as_v5"))
        );
    }

    #[test]
    fn a_machine_on_more_networks_than_a_vm_takes_interfaces_is_refused() {
        let nets: Vec<String> = (1..=9).map(|n| format!("n{n}: {{ cidr: 10.0.{n}.0/24 }}")).collect();
        let on: Vec<String> = (1..=9).map(|n| format!("n{n}: 10")).collect();
        let yaml = format!(
            "version: 1\nname: t\nnetworks: {{ {} }}\nmachines:\n  web: {{ networks: {{ {} }}, os: debian@12 }}\n",
            nets.join(", "),
            on.join(", ")
        );
        let env = crate::testing::planned(&yaml, Target::CloudVm);
        let why = interfaces(&env).unwrap_or_default();
        assert!(why.contains("machine `web`") && why.contains("8 network interfaces"), "{why}");
    }
}
