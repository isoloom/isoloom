//! Azure's `cloud-vm` module, `.isoloom/cloud-vm/azure/main.tf`, rendered from the plan through
//! the VM core: the AWS model on the `azurerm` provider.
//!
//! - One resource group and virtual network holding every network as a subnet with its exact
//!   range. The virtual network routes between subnets (the machines' `Routes` and `Trunks`
//!   steps are left out); a network security group per machine does what the router does on
//!   other targets: a machine's networks reach it, `reach` rules open one network to another
//!   (on their ports), the rest is closed.
//! - Each machine is a VM at its address with a static public IP that sets itself up from its
//!   user data (`crate::cloud::vm::hooks::user_data`), once the runner delivered the
//!   project over SSH as `isoloom` (Azure creates the administrator; it reserves `admin`). A
//!   machine on several networks gets an interface on each (forwarding on, so replies leave from
//!   the right one), brought up by MAC at every boot.
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

pub(super) use machines::{os_disk, source_image};

use crate::hcl_builder::{Expression, Module, reference};
use isoloom_plan::Environment;

use crate::cloud::Cloud;
use crate::cloud::vm::rules::{FIRST_THREE, Images, Mtu, Need, Networks, OsImages, PLAYBOOK_CHECKS, Rules, Size, Sizes, Subnets, Volumes};
use crate::cloud::vm::{self, Driver, gateways, gpus::GpuSizes, own_image, windows};
use crate::limits::Limits;
use crate::module::providers;

/// Azure creates the administrator with the VM: a name it never reserves, on every machine.
pub(super) use crate::machine::USER;

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
    need: Need::of,
    sizes: Some(SIZES),
    nesting_sizes: None,
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
        table: &IMAGES,
        own: own_image::azure,
    },
};

/// How the module writes its machines.
const MACHINES: vm::Machines = vm::Machines {
    linux: machines::linux,
    windows: Some(machines::windows),
    controller: machines::controller,
};

/// The Azure Marketplace image of each OS: (publisher, offer, sku).
const IMAGES: OsImages<(&str, &str, &str)> = OsImages(&[
    ("debian-12", ("Debian", "debian-12", "12-gen2")),
    ("debian-13", ("Debian", "debian-13", "13-gen2")),
    ("ubuntu-22.04", ("Canonical", "0001-com-ubuntu-server-jammy", "22_04-lts-gen2")),
    ("ubuntu-24.04", ("Canonical", "ubuntu-24_04-lts", "server-gen1")),
    ("windows-server-2016", ("MicrosoftWindowsServer", "WindowsServer", "2016-datacenter-gensecond")),
    ("windows-server-2019", ("MicrosoftWindowsServer", "WindowsServer", "2019-datacenter-gensecond")),
    (
        "windows-server-2022",
        ("MicrosoftWindowsServer", "WindowsServer", "2022-datacenter-azure-edition"),
    ),
    (
        "windows-server-2025",
        ("MicrosoftWindowsServer", "WindowsServer", "2025-datacenter-azure-edition"),
    ),
]);

/// An OS's image, when there is one.
fn image(os: &str) -> Option<(&'static str, &'static str, &'static str)> {
    IMAGES.of(os)
}

/// The sizes Isoloom picks, cheapest first, each with its vCPUs, memory and network interfaces
/// at most.
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
const SIZES: Sizes = Sizes::ordered(&[
    Size::new("Standard_B2ts_v2", 2, 1024, 2),
    Size::new("Standard_B2ls_v2", 2, 4096, 2),
    Size::new("Standard_B2s_v2", 2, 8192, 2),
    Size::new("Standard_B4s_v2", 4, 16384, 2),
    Size::new("Standard_B8s_v2", 8, 32768, 2),
    Size::new("Standard_D8as_v5", 8, 32768, 4),
    Size::new("Standard_D16as_v5", 16, 65536, 8),
    Size::new("Standard_D32as_v5", 32, 131_072, 8),
]);

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
        .or_else(machine)
        .or_else(|| windows::udp_redirect(spec))
}

/// Its `cloud-vm` driver.
pub(crate) const DRIVER: Driver = Driver {
    base: super::BASE,
    requires: (&[providers::AZURERM], &[providers::RANDOM, providers::LOCAL]),
    refusal,
    write,
};

/// The module's blocks after the providers, from the plan.
fn write(env: &Environment, m: &mut Module) -> crate::Result<()> {
    let env = &*gateways::relocated(env);
    network::blocks(env, m);
    routing::blocks(env, m);
    private_dns::write(env, m);
    MACHINES.write_with_outputs(env, super::BASE.user, public_ip, m)
}

/// The address Terraform reaches a machine at (`id`: its resource name).
fn public_ip(id: &str) -> Expression {
    reference(&format!("azurerm_public_ip.{id}.ip_address"))
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;

    /// The size of a machine of `cpus` and `memory_mb` on `networks` networks.
    fn size(cpus: u32, memory_mb: u32, networks: usize) -> &'static str {
        SIZES.fit(&Need::new(cpus, memory_mb, 20).on(networks))
    }

    #[test]
    fn a_machine_gets_the_cheapest_current_size_with_its_cpus_memory_and_interfaces() {
        assert_eq!(size(1, 512, 1), "Standard_B2ts_v2");
        assert_eq!(size(1, 2048, 2), "Standard_B2ls_v2");
        assert_eq!(size(1, 8192, 1), "Standard_B2s_v2");
        assert_eq!(size(1, 1024, 3), "Standard_D8as_v5");
        assert_eq!(size(1, 40_000, 5), "Standard_D16as_v5");
        assert_eq!(size(4, 1024, 1), "Standard_B4s_v2");
        assert_eq!(size(16, 1024, 1), "Standard_D16as_v5");
        assert!(
            SIZES
                .names()
                .all(|name| name.starts_with("Standard_B") && name.ends_with("_v2") || name.ends_with("as_v5"))
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
        let why = refusal(&env).unwrap_or_default();
        assert!(
            why.contains("machine `web` is on 9 networks") && why.contains("takes 8 network interfaces"),
            "{why}"
        );
    }
}
