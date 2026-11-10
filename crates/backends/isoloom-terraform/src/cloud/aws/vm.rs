//! AWS's `cloud-vm` module, `.isoloom/cloud-vm/aws/main.tf`, rendered from the plan through the
//! VM core.
//!
//! - One VPC holding every network as a subnet with its exact range, so every address of the
//!   spec is kept. The VPC routes between subnets (the machines' `Routes` and `Trunks` steps are
//!   left out); security groups do what the router does on other targets: a network's machines
//!   reach each other, `reach` rules open one network to another (on their ports), the rest is
//!   closed.
//! - Each machine is an instance at its address that sets itself up from its user data (the
//!   core's, `crate::cloud::vm::hooks::user_data`), once the runner delivered the project
//!   over SSH. A machine on several networks gets a network interface on each
//!   (source/destination check off, so replies leave from the right one), brought up by MAC at
//!   every boot, and an Elastic IP on the first.
//! - Published services: open to `allowed_cidr` on the instance's public address; a published
//!   port that differs from the service's own is redirected inside the machine.
//! - Windows (Amazon's Server images): an `isoloom` administrator with a generated password and
//!   WinRM at first boot, then the core's WinRM set-up.
//! - Environment-level `provision:`: the core's controller, a Debian instance on every network.

mod addresses;
mod machines;
mod network;
mod outputs;
mod private_dns;
mod routing;

use crate::hcl_builder::{Block, BlockBuilder, Module, string};
use isoloom_plan::{Environment, MachinePlan, Vm};
use isoloom_spec::net::Cidr;
use isoloom_spec::windows::Tpm;

use super::{INSTANCE_TYPES, ami, ami_filter, zones};
use crate::cloud::Cloud;
use crate::cloud::vm::rules::{FIRST_THREE, Images, Mtu, Need, Networks, OsImages, PLAYBOOK_CHECKS, Rules, SECURITY_GROUPS_TC, Size, Sizes, Subnets, Volumes};
use crate::cloud::vm::{self, Driver, disks, gateways, gpus::GpuSizes, nested, own_image, windows};
use crate::limits::Limits;
use crate::machine::controller;
use crate::module::ident;
use crate::module::providers;

/// What AWS takes.
const RULES: Rules = Rules {
    cloud: Cloud::Aws,
    networks: Networks::Several,
    limits: Limits {
        arm64: Some("Isoloom generates no arm64 VM on AWS"),
        gateways: None,
        playbook_checks: Some(PLAYBOOK_CHECKS),
        tc: Some(SECURITY_GROUPS_TC),
        ipv6: Some("an AWS VPC takes an Amazon-provided (or your own registered) IPv6 block, never one the spec picks"),
    },
    // NVIDIA T4s (G4dn). New accounts have no quota for them until they ask (Service Quotas,
    // "Running On-Demand G and VT instances").
    gpus: GpuSizes {
        gpu: "NVIDIA T4",
        sizes: &[(1, "g4dn.xlarge"), (4, "g4dn.12xlarge"), (8, "g4dn.metal")],
        none: "",
    },
    nests: true,
    need,
    sizes: Some(INSTANCE_TYPES),
    nesting_sizes: Some(NESTING),
    mtu: Mtu::Fixed,
    picks_ipv6: false,
    volumes: Volumes {
        smallest_gb: 1,
        most: disks::AWS_MAX_DISKS,
    },
    tpm: true,
    subnets: Some(Subnets {
        called: "subnets",
        longest: 28,
    }),
    kept: Some(FIRST_THREE),
    images: Images {
        table: &IMAGES,
        own: own_image::aws,
    },
};

/// How the module writes its machines.
const MACHINES: vm::Machines = vm::Machines {
    linux: machines::linux,
    windows: Some(machines::windows),
    controller: machines::controller,
};

/// The instance types whose CPUs nest (8th-generation Intel), smallest first, as
/// [`INSTANCE_TYPES`].
const NESTING: Sizes = Sizes::ordered(&[
    Size::new("m8i.large", 2, 8192, 3).with_addresses(10),
    Size::new("m8i.xlarge", 4, 16384, 4).with_addresses(15),
    Size::new("m8i.2xlarge", 8, 32768, 4).with_addresses(15),
]);

/// What a machine asks of its instance type ([`Need::of`]), and the addresses its busiest
/// interface holds.
fn need(mp: &MachinePlan) -> Need {
    Need::of(mp).with_addresses(addresses::busiest(mp))
}

/// A machine's instance type: its GPUs', a nesting one, else the smallest that takes it.
fn machine_type(mp: &MachinePlan) -> &'static str {
    RULES.size_of(mp)
}

/// The instance's nested virtualization switch, when the machine runs VMs of its own.
fn nesting(b: BlockBuilder, mp: &MachinePlan) -> BlockBuilder {
    if !nested::wanted(mp) {
        return b;
    }
    b.add_block(
        Block::builder("cpu_options")
            .add_attribute(("nested_virtualization", string("enabled")))
            .build(),
    )
}

/// An OS's image on AWS: how its AMI is looked up, and what it needs.
#[derive(Debug, Clone, Copy)]
struct Image {
    /// The AMI's owner (an account, or `aws-marketplace`).
    owner: &'static str,
    /// The AMI's name filter (the latest match is taken).
    filter: &'static str,
    /// The image's SSH user.
    user: &'static str,
    /// The Marketplace product the AMI must belong to, when its owner is the Marketplace (any
    /// seller lists there: the name alone could match a look-alike).
    product_code: Option<&'static str>,
    /// The smallest root disk the image takes, in GB (its snapshot's size), when it is more than
    /// the default.
    min_disk_gb: Option<u32>,
}

impl Image {
    /// An image looked up by owner and name only, with no disk of its own.
    const fn new(owner: &'static str, filter: &'static str, user: &'static str) -> Image {
        Image {
            owner,
            filter,
            user,
            product_code: None,
            min_disk_gb: None,
        }
    }
}

/// The OSes AWS has images for.
const IMAGES: OsImages<Image> = OsImages(&[
    ("debian-12", Image::new("136693071363", "debian-12-amd64-*", "admin")),
    ("debian-13", Image::new("136693071363", "debian-13-amd64-*", "admin")),
    (
        "ubuntu-22.04",
        Image::new("099720109477", "ubuntu/images/hvm-ssd*/ubuntu-jammy-22.04-amd64-server-*", "ubuntu"),
    ),
    (
        "ubuntu-24.04",
        Image::new("099720109477", "ubuntu/images/hvm-ssd*/ubuntu-noble-24.04-amd64-server-*", "ubuntu"),
    ),
    // Amazon's Windows Server images (license included, billed per hour).
    (
        "windows-server-2016",
        Image::new("801119661308", "Windows_Server-2016-English-Full-Base-*", crate::machine::USER),
    ),
    (
        "windows-server-2019",
        Image::new("801119661308", "Windows_Server-2019-English-Full-Base-*", crate::machine::USER),
    ),
    (
        "windows-server-2022",
        Image::new("801119661308", "Windows_Server-2022-English-Full-Base-*", crate::machine::USER),
    ),
    (
        "windows-server-2025",
        Image::new("801119661308", "Windows_Server-2025-English-Full-Base-*", crate::machine::USER),
    ),
    // Kali's own image, on the AWS Marketplace (free; an account may have to accept its terms
    // there once), pinned to its product; its snapshot is 25 GB.
    (
        "kali",
        Image {
            product_code: Some("7lgvy7mt78lgoi4lant0znp5h"),
            min_disk_gb: Some(25),
            ..Image::new("aws-marketplace", "debian-kali-last-snapshot-amd64-*", "kali")
        },
    ),
]);

/// An OS's image, when AWS has one.
fn image(os: &str) -> Option<Image> {
    IMAGES.of(os)
}

/// The smallest root disk an OS's image takes, in GB (0 when the default is enough).
pub(super) fn image_min_disk_gb(os: &str) -> u32 {
    image(os).and_then(|i| i.min_disk_gb).unwrap_or(0)
}

/// The SSH user of an OS's image (`admin` for an OS without one).
pub(super) fn image_user(os: &str) -> &'static str {
    image(os).map_or("admin", |i| i.user)
}

/// Why AWS can't run this environment, if it can't: the features every cloud checks
/// ([`Rules::features`](crate::cloud::vm::rules::Rules::features)), then its address rules and
/// images (a machine's own AMI stands in for its OS's).
fn refusal(env: &Environment) -> Option<String> {
    RULES
        .features(env)
        .or_else(|| networks_refusal(env))
        .or_else(|| routing::refusal(env))
        .or_else(|| machines_refusal(env))
}

fn networks_refusal(env: &Environment) -> Option<String> {
    let nets: Vec<Cidr> = env.networks.values().map(|n| n.cidr.cidr()).collect();
    let family = |c: &Cidr| c.base() >> 24;
    let mixed = || nets.iter().any(|c| family(c) != nets.first().map_or(0, family));
    RULES
        .subnets(env)
        .or_else(|| mixed().then(|| "AWS can't mix private ranges (10/8, 172.16/12, 192.168/16) in one VPC".to_owned()))
        .or_else(|| windows::udp_redirect(env))
}

/// Why one of the VMs can't run on AWS, if one can't, the first in spec order.
fn machines_refusal(env: &Environment) -> Option<String> {
    env.machines.iter().filter_map(|(name, mp)| Vm::of(name, mp)).find_map(|vm| {
        let name = vm.name;
        RULES
            .image(vm)
            .or_else(|| windows::refusal(env, vm))
            .or_else(|| {
                (vm.plan.is_windows() && vm.plan.tpm == Some(Tpm::Required)).then(|| {
                    format!("machine `{name}`: a TPM on a Windows VM on AWS needs Amazon's NitroTPM images (TPM-Windows_Server-*), which Isoloom doesn't use")
                })
            })
            .or_else(|| addresses::refusal(name, vm.plan))
            .or_else(|| RULES.kept(env, vm))
    })
}

/// Its `cloud-vm` driver.
pub(crate) const DRIVER: Driver = Driver {
    base: super::BASE,
    requires: (&[providers::AWS], &[providers::RANDOM, providers::TIME, providers::LOCAL]),
    write,
    refusal,
};

/// The module's blocks after the providers, from the plan.
fn write(env: &Environment, m: &mut Module) -> crate::Result<()> {
    let env = &*gateways::relocated(env);
    m.block(zones());
    network::blocks(env, m);
    private_dns::write(env, m);
    crate::machine::credentials(env, m);
    m.blocks(amis(env));
    MACHINES.write(env, m)?;
    outputs::blocks(env, m);
    Ok(())
}

/// An AMI lookup per OS (the controller's among them).
fn amis(env: &Environment) -> Vec<Block> {
    let mut oses: Vec<&str> = env
        .vms()
        .filter(|vm| own_image::of(vm.plan, own_image::aws).is_none())
        .map(|vm| vm.os)
        .collect();
    if controller::needed(env) {
        oses.push(controller::OS);
    }
    oses.sort_unstable();
    oses.dedup();
    oses.into_iter().filter_map(ami_lookup).collect()
}

/// The AMI lookup of an OS from the image table (`data.aws_ami.<os>`), when AWS has an image of
/// it: the `cloud-vm` machines' and the `cloud-docker` host's.
pub(super) fn ami_lookup(os: &str) -> Option<Block> {
    let i = image(os)?;
    let lookup = ami(&ident(os), i.owner, i.filter).add_block(ami_filter("architecture", "x86_64"));
    Some(match i.product_code {
        Some(code) => lookup.add_block(ami_filter("product-code", code)).build(),
        None => lookup.build(),
    })
}
