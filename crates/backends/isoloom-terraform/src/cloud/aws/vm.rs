//! AWS's `cloud-vm` module, `.isoloom/cloud-vm/aws/main.tf`, rendered from the plan through the
//! VM core.
//!
//! - One VPC holding every network as a subnet with its exact range, so every address of the
//!   spec is kept. The VPC routes between subnets (the machines' `Routes` and `Trunks` steps are
//!   left out); security groups do what the router does on other targets: a network's machines
//!   reach each other, `reach` rules open one network to another (on their ports), the rest is
//!   closed.
//! - Each machine is an instance at its address, set up over SSH from where Terraform runs (the
//!   core's `remote-exec` transport). A machine on several networks gets a network interface on
//!   each (source/destination check off, so replies leave from the right one), brought up by MAC
//!   at every boot, and an Elastic IP on the first.
//! - Published services: open to `allowed_cidr` on the instance's public address; a published
//!   port that differs from the service's own is redirected inside the machine.
//! - Windows (Amazon's Server images): an `isoloom` administrator with a generated password and
//!   WinRM at first boot, then the core's WinRM set-up.
//! - Environment-level `provision:`: the core's controller, a Debian instance on every network.

mod addresses;
mod machines;
mod network;
mod outputs;
mod routing;

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::GeneratedFile;
use isoloom_render::hcl_builder::{Block, BlockBuilder, string};
use isoloom_spec::images::is_windows;
use isoloom_spec::model::Target;
use isoloom_spec::validate::Cidr;

use super::{ami, ami_filter, instance_type, provider, region, zones};
use crate::Result;
use crate::cloud::vm::gpus::GpuSizes;
use crate::cloud::vm::rules::{Images, Mtu, Networks, PLAYBOOK_CHECKS, Rules, Volumes, kept_address};
use crate::cloud::vm::{self, disks, gateways, nested, own_image, windows};
use crate::cloud::{Cloud, instance, operator};
use crate::limits::Limits;
use crate::machine::{controller, vms};
use crate::module::providers;
use crate::module::{self, ProjectCopy, Usage, ident};

/// What AWS takes.
const RULES: Rules = Rules {
    cloud: Cloud::Aws,
    networks: Networks::Several,
    limits: Limits {
        arm64: Some("arm64 on AWS comes later (Graviton instances and arm64 AMIs)"),
        gateways: None,
        playbook_checks: Some(PLAYBOOK_CHECKS),
        tc: Some("link impairment (`tc`) needs a router in the path; the cloud's security groups have none"),
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
    mtu: Mtu::Fixed,
    picks_ipv6: false,
    volumes: Volumes {
        smallest_gb: 1,
        most: disks::AWS_MAX_DISKS,
    },
    tpm: true,
    images: Images {
        label: "AWS",
        has: |os| image(os).is_some(),
        list: "Debian 12/13, Ubuntu 22.04/24.04, Windows Server 2016 to 2025",
        own: own_image::aws,
    },
};

/// How the module writes its machines.
const MACHINES: vm::Machines = vm::Machines {
    linux: machines::linux,
    windows: machines::windows,
    controller: machines::controller,
};

/// The instance types whose CPUs nest (8th-generation Intel), by memory in MB.
const NESTING: &[(u32, &str)] = &[(8192, "m8i.large"), (16384, "m8i.xlarge"), (32768, "m8i.2xlarge")];

/// A machine's instance type: its GPUs', a nesting one, else by its memory.
fn machine_type(mp: &MachinePlan, memory_mb: u32) -> &'static str {
    RULES
        .gpus
        .of(mp)
        .or_else(|| nested::size(mp, memory_mb, NESTING))
        .unwrap_or_else(|| addresses::sized(instance_type(memory_mb), mp))
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

/// The AMI lookup and SSH user for an OS name: (owner, name filter, ssh user).
fn image(os: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match os {
        "debian-12" => ("136693071363", "debian-12-amd64-*", "admin"),
        "debian-13" => ("136693071363", "debian-13-amd64-*", "admin"),
        "ubuntu-22.04" => ("099720109477", "ubuntu/images/hvm-ssd*/ubuntu-jammy-22.04-amd64-server-*", "ubuntu"),
        "ubuntu-24.04" => ("099720109477", "ubuntu/images/hvm-ssd*/ubuntu-noble-24.04-amd64-server-*", "ubuntu"),
        // Amazon's Windows Server images (license included, billed per hour).
        "windows-server-2016" => ("801119661308", "Windows_Server-2016-English-Full-Base-*", "isoloom"),
        "windows-server-2019" => ("801119661308", "Windows_Server-2019-English-Full-Base-*", "isoloom"),
        "windows-server-2022" => ("801119661308", "Windows_Server-2022-English-Full-Base-*", "isoloom"),
        "windows-server-2025" => ("801119661308", "Windows_Server-2025-English-Full-Base-*", "isoloom"),
        _ => return None,
    })
}

/// The SSH user of an OS's image (`admin` for an OS without one).
fn image_user(os: &str) -> &'static str {
    image(os).map_or("admin", |(_, _, user)| user)
}

/// Why AWS can't run this environment, if it can't: its limits, then its address rules and
/// images (a machine's own AMI stands in for its OS's), then its other features.
pub(crate) fn refusal(env: &Environment) -> Option<String> {
    RULES
        .limits
        .refusal(env)
        .or_else(|| networks_refusal(env))
        .or_else(|| routing::refusal(env))
        .or_else(|| RULES.mtu(env))
        .or_else(|| machines_refusal(env))
        .or_else(|| RULES.gpus(env))
        .or_else(|| RULES.nesting(env))
        .or_else(|| RULES.volumes(env))
}

fn networks_refusal(env: &Environment) -> Option<String> {
    let nets: Vec<Cidr> = env.networks.values().filter_map(|n| Cidr::parse(&n.cidr)).collect();
    if nets.iter().any(|c| c.len > 28) {
        return Some("AWS subnets are /28 or larger".to_owned());
    }
    let family = |c: &Cidr| c.base >> 24;
    if nets.iter().any(|c| family(c) != nets.first().map_or(0, family)) {
        return Some("AWS can't mix private ranges (10/8, 172.16/12, 192.168/16) in one VPC".to_owned());
    }
    windows::udp_redirect(env.spec())
}

fn machines_refusal(env: &Environment) -> Option<String> {
    env.spec().machines.iter().find_map(|(name, m)| {
        let vm = m.vm.as_ref()?;
        RULES
            .images
            .missing(name, &vm.os, own_image::of_machine(m, RULES.images.own))
            .or_else(|| windows::refusal(name, m))
            .or_else(|| {
                (is_windows(&vm.os) && vm.tpm == Some(true))
                    .then(|| format!("machine `{name}`: a TPM on a Windows VM on AWS needs Amazon's NitroTPM images (TPM-Windows_Server-*), which come later"))
            })
            .or_else(|| env.machines.get(name).and_then(|mp| addresses::refusal(name, mp)))
            .or_else(|| {
                kept_address(env, name, m, 0..=3).map(|net| format!("machine `{name}`: AWS keeps the first addresses of a subnet (.1 to .3) on `{net}`"))
            })
    })
}

/// The module.
pub(crate) fn module(env: &Environment) -> Result<GeneratedFile> {
    let env = &*gateways::relocated(env);
    let folder = format!("{}/{}", Target::CloudVm.id(), Cloud::Aws.id());
    let mut m = module::start(&Usage::with_key(&folder, operator::ALLOWED));
    m.block(providers::terraform(&vm::providers(
        env,
        &[providers::AWS, providers::LOCAL],
        &[providers::RANDOM, providers::TIME],
    )));
    m.block(region());
    m.blocks(operator::variables(env));
    m.block(provider(instance::tags(env)));
    m.blocks(instance::id_and_locals(env, Vec::new()));
    m.block(zones());
    network::blocks(env, &mut m);
    vm::private_dns::aws(env, &mut m);
    vm::credentials(env, &mut m);
    m.blocks(amis(env));
    MACHINES.write(env, &mut m);
    outputs::blocks(env, &mut m);
    module::file(Target::CloudVm, &folder, m, ProjectCopy::Archive)
}

/// An AMI lookup per OS (the controller's among them).
fn amis(env: &Environment) -> Vec<Block> {
    let mut oses: Vec<&str> = vms(env)
        .filter(|(_, mp)| own_image::of(mp, own_image::aws).is_none())
        .filter_map(|(_, mp)| mp.os.as_deref())
        .collect();
    if controller::needed(env) {
        oses.push(controller::OS);
    }
    oses.sort_unstable();
    oses.dedup();
    oses.into_iter()
        .filter_map(|o| image(o).map(|(owner, filter, _)| (o, owner, filter)))
        .map(|(o, owner, filter)| ami(&ident(o), owner, filter).add_block(ami_filter("architecture", "x86_64")).build())
        .collect()
}
