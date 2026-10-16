//! Oracle Cloud's `cloud-vm` module, `.isoloom/cloud-vm/oci/main.tf`, rendered from the plan
//! through the VM core. One compute instance per machine on one VCN, a subnet per network; OCI
//! pins a VNIC's private IP, so every machine keeps its spec address, with a VNIC on each of its
//! networks. Each network has its own security list (`network`), and a network a machine routes
//! goes through it. Linux labs: the module refuses Windows and a controller.
//!
//! A dual-stack network carries the spec's IPv6 block as its subnet's prefix, and each VNIC on it
//! an address from that prefix (`ipv6`). An IPv6-only network is refused by the plan: machines
//! are set up over their public IPv4 address, and a private prefix never reaches the internet.
//!
//! Each machine's published ports open on its own VNIC only, through a network security group
//! of its own; the subnet's security list holds what every machine shares (the VCN, SSH).
//!
//! The image is looked up, not given: a `data "oci_core_images"` finds the newest Canonical
//! Ubuntu for the shape, as the `cloud-docker` module does. Oracle Cloud ships no Debian platform
//! image, so Debian labs run on Ubuntu too; the SSH login is `ubuntu` either way.

mod addresses;
mod ipv6;
mod network;
mod private_dns;
mod published;

use std::net::Ipv4Addr;

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, attr, boolean, call, index, list, number, object, reference, string};
use isoloom_spec::{DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::{SHAPE, availability_domain, availability_domains, owned, provider, ubuntu_images};
use crate::cloud::instance::name_with;
use crate::cloud::vm::rules::{FIREWALLS_TC, Images, Mtu, Networks, PLAYBOOK_CHECKS, Rules, Volumes};
use crate::cloud::vm::{self, Driver, disks, gateways, gpus::GpuSizes, hooks, interfaces, nested, outputs, own_image};
use crate::cloud::{Cloud, auto_stop};
use crate::limits::Limits;
use crate::machine::{Remote, linux, vms};
use crate::module::variable::Variable;
use crate::module::{Usage, ident, providers, resource};

/// What Oracle Cloud takes: Ubuntu, and Debian on the newest Ubuntu LTS (it has no Debian image).
/// Any other OS needs an image of its own (`vm.image.oci`) rather than running Ubuntu in its place.
const RULES: Rules = Rules {
    cloud: Cloud::Oci,
    networks: Networks::Several,
    limits: Limits {
        arm64: Some("arm64 on Oracle Cloud comes later (Ampere shapes and arm64 images)"),
        // The VCN routes a network through its gateway machine (`network`).
        gateways: None,
        playbook_checks: Some(PLAYBOOK_CHECKS),
        tc: Some(FIREWALLS_TC),
        // The VCN takes the spec's blocks as its private IPv6 prefixes (`network`).
        ipv6: None,
    },
    gpus: GpuSizes {
        gpu: "",
        sizes: &[],
        none: "GPUs on Oracle Cloud come later (its GPU shapes are fixed, not the flexible shape every machine has, and take images built for them)",
    },
    // The AMD shape exposes its virtualization instructions when asked (`platform_config`).
    nests: true,
    mtu: Mtu::Fixed,
    // A VNIC on an `auto` network takes the address Oracle Cloud picks (`ipv6`).
    picks_ipv6: true,
    volumes: Volumes { smallest_gb: 50, most: 15 },
    tpm: true,
    subnets: None,
    kept: None,
    images: Images {
        label: "Oracle Cloud",
        has: |os| matches!(os, "debian-12" | "debian-13" | "ubuntu-22.04" | "ubuntu-24.04"),
        list: "Ubuntu 22.04/24.04, Debian 12/13 on Ubuntu 24.04",
        own: own_image::oci,
    },
};

/// The SSH user of every instance (Ubuntu's).
const USER: &str = "ubuntu";

/// Why Oracle Cloud can't run this environment, if it can't: its rules, then what its VCN can't
/// hold.
fn refusal(env: &Environment) -> Option<String> {
    RULES.linux_refusal(env).or_else(|| network::refusal(env)).or_else(|| ipv6::refusal(env))
}

/// The Ubuntu version an OS runs on: Oracle Cloud has no Debian image, so Debian labs fall back
/// to the newest Ubuntu LTS, as cloud-docker does.
fn ubuntu(os: &str) -> &'static str {
    match os {
        "ubuntu-22.04" => "22.04",
        _ => "24.04",
    }
}

/// The `oci_core_images` data source of an Ubuntu version.
fn image_source(version: &str) -> String {
    format!("ubuntu_{}", version.replace('.', ""))
}

/// The shape's sizing for a machine's memory: whole GB (at least 1), one OCPU per 8 GB.
fn sizing(memory_mb: u32) -> (u32, u32) {
    let gb = memory_mb.div_ceil(1024).max(1);
    ((gb / 8).max(1), gb)
}

/// Its `cloud-vm` driver: its module takes the compartment it goes in, and authenticates with
/// the OCI CLI's configuration.
pub(crate) const DRIVER: Driver = Driver {
    usage: |folder| Usage {
        after: "# Auth:   the oci provider reads ~/.oci/config; region defaults, override with -var region=<id>.\n",
        ..Usage::with_key(folder, "-var compartment_id=<compartment ocid> -var allowed_cidr=<your IP>/32 \\\n#           ")
    },
    requires: (&[providers::OCI, providers::LOCAL], &[]),
    variables: || {
        vec![
            Variable::new("compartment_id", "string")
                .describe("The compartment the environment goes in")
                .block(),
            Variable::new("region", "string")
                .default(string("eu-paris-1"))
                .describe("The OCI region to launch in")
                .block(),
        ]
    },
    provider,
    refusal,
    write,
};

/// The module after its providers: the images, the VCN, its security list and subnet, each machine, the outputs.
fn write(env: &Environment, m: &mut Module) {
    let env = &*gateways::relocated(env);
    m.commented(
        "The compartment's availability domains; the instances land in the first.",
        availability_domains(),
    );
    images(env, m);
    network::blocks(env, m);
    private_dns::write(env, m);
    for (vm, at) in vm::at_first_address(env) {
        machine(env, vm, at, m);
    }
    network::gateway_routes(env, m);
    outputs::write_as(env, USER, &public_ip, m);
}

/// An instance's public address.
fn public_ip(name: &str) -> Expression {
    reference(&format!("oci_core_instance.{}.public_ip", ident(name)))
}

/// The newest Canonical Ubuntu for the shape, one lookup per version the machines use.
fn images(env: &Environment, m: &mut Module) {
    let mut versions: Vec<&str> = vms(env)
        .filter(|(_, mp)| own_image::of(mp, own_image::oci).is_none())
        .map(|(_, mp)| ubuntu(mp.os.as_deref().unwrap_or_default()))
        .collect();
    versions.sort_unstable();
    versions.dedup();
    for v in versions {
        m.commented(
            &format!("Ubuntu {v}: Oracle Cloud has no Debian image of its own."),
            ubuntu_images(&image_source(v), v),
        );
    }
}

/// Machine `name`'s instance at its fixed address, its VNICs and further addresses, and its
/// set-up over SSH.
fn machine(env: &Environment, (name, mp): (&str, &MachinePlan), (net, address): (&str, Ipv4Addr), m: &mut Module) {
    let id = ident(name);
    let group = published::group(name, mp, m);
    m.commented(
        &format!("Machine `{name}`, at its fixed address {address}."),
        instance(env, (name, mp), (net, &address.to_string()), group),
    );
    vnics(env, (name, mp), m);
    addresses::blocks((name, mp), m);
    let mac = |n: &str| call("lower", [reference(&format!("data.oci_core_vnic.{id}_{}.mac_address", ident(n)))]);
    let mut at_boot = interfaces::up(env, &mp.addresses, &mac);
    at_boot.extend(ipv6::at_boot(env, name, mp));
    at_boot.extend(gateways::at_boot(mp));
    let machine = linux::Machine {
        remote: Remote {
            env,
            name,
            plan: mp,
            replace_on: reference(&format!("oci_core_instance.{id}.id")),
            host: public_ip(name),
        },
        user: USER.to_owned(),
        hooks: hooks::of(mp, at_boot),
    };
    m.block(linux::provisioner(machine).block());
}

/// Machine `name`'s instance, its first VNIC on network `net` at `address` (with its published
/// ports' `group`).
fn instance(env: &Environment, (name, mp): (&str, &MachinePlan), (net, address): (&str, &str), group: Option<Expression>) -> Block {
    // A gateway forwards traffic that isn't its own; a machine on several networks answers from
    // the interface its default route leaves by.
    let forwards = mp.is_gateway || mp.addresses.len() > 1;
    let (ocpus, memory_gb) = sizing(mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB));
    let shape = Block::builder("shape_config")
        .add_attribute(("ocpus", number(u64::from(ocpus))))
        .add_attribute(("memory_in_gbs", number(u64::from(memory_gb))))
        .build();
    let images = reference(&format!(
        "data.oci_core_images.{}.images",
        image_source(ubuntu(mp.os.as_deref().unwrap_or_default()))
    ));
    // OCI boot volumes start at 50 GB.
    let boot = mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB).max(50);
    let source = Block::builder("source_details")
        .add_attribute(("source_type", string("image")))
        .add_attribute((
            "source_id",
            own_image::of(mp, own_image::oci).map_or_else(|| attr(index(images, 0), "id"), string),
        ))
        .add_attribute(("boot_volume_size_in_gbs", number(u64::from(boot))))
        .build();
    let vnic = ipv6::vnic(env, mp, net, primary_vnic(name, net, address, forwards, group)).build();
    let metadata = object([
        ("ssh_authorized_keys", reference("var.ssh_public_key")),
        (
            "user_data",
            call("base64encode", [auto_stop::if_set(auto_stop::script(), string("#!/bin/sh\n"))]),
        ),
    ]);
    owned(resource("oci_core_instance", &ident(name)))
        .add_attribute(("availability_domain", availability_domain()))
        .add_attribute(("display_name", name_with(&format!("-{name}"))))
        .add_attribute(("shape", string(SHAPE)))
        .add_block(shape)
        .add_blocks(platform(mp))
        .add_block(source)
        .add_block(vnic)
        .add_blocks(disks::oci(name, &mp.disks))
        .add_attribute(("metadata", metadata))
        .build()
}

/// The instance's first VNIC, on its first network at its address, with its published ports'
/// group when it has one.
fn primary_vnic(name: &str, net: &str, address: &str, forwards: bool, group: Option<Expression>) -> BlockBuilder {
    let mut vnic = Block::builder("create_vnic_details")
        .add_attribute(("subnet_id", reference(&format!("oci_core_subnet.{}.id", ident(net)))))
        .add_attribute(("private_ip", string(address)))
        .add_attribute(("assign_public_ip", boolean(true)))
        .add_attribute(("hostname_label", string(name)))
        .add_attribute(("skip_source_dest_check", boolean(forwards)));
    if let Some(group) = group {
        vnic = vnic.add_attribute(("nsg_ids", list([group])));
    }
    vnic
}

/// A VNIC on each network after the first, at the machine's address there (forwarding, as the
/// first), and its MAC address, which finds it at boot.
fn vnics(env: &Environment, (name, mp): (&str, &MachinePlan), m: &mut Module) {
    let id = ident(name);
    for (net, address) in mp.addresses.iter().skip(1) {
        let nic = format!("{id}_{}", ident(net));
        let details = Block::builder("create_vnic_details")
            .add_attribute(("subnet_id", reference(&format!("oci_core_subnet.{}.id", ident(net)))))
            .add_attribute(("private_ip", string(address.to_string())))
            .add_attribute(("assign_public_ip", boolean(false)))
            .add_attribute(("skip_source_dest_check", boolean(true)));
        let details = ipv6::vnic(env, mp, net, details).build();
        m.block(
            resource("oci_core_vnic_attachment", &nic)
                .add_attribute(("instance_id", reference(&format!("oci_core_instance.{id}.id"))))
                .add_attribute(("display_name", name_with(&format!("-{name}-{net}"))))
                .add_block(details)
                .build(),
        );
        m.block(
            Block::builder("data")
                .add_label("oci_core_vnic")
                .add_label(&nic)
                .add_attribute(("vnic_id", reference(&format!("oci_core_vnic_attachment.{nic}.vnic_id"))))
                .build(),
        );
    }
}

/// The AMD shape's platform, when the machine asks for what it gives: a shielded instance for a
/// TPM (its virtual TPM, secure and measured boot; Canonical's images are shielded-instance
/// images), and the virtualization instructions for a machine running VMs of its own.
fn platform(mp: &MachinePlan) -> Option<Block> {
    let (tpm, nested) = (mp.tpm.is_some(), nested::wanted(mp));
    if !tpm && !nested {
        return None;
    }
    let mut b = Block::builder("platform_config").add_attribute(("type", string("AMD_VM")));
    if tpm {
        b = b
            .add_attribute(("is_secure_boot_enabled", boolean(true)))
            .add_attribute(("is_measured_boot_enabled", boolean(true)))
            .add_attribute(("is_trusted_platform_module_enabled", boolean(true)));
    }
    if nested {
        b = b.add_attribute(("are_virtual_instructions_enabled", boolean(true)));
    }
    Some(b.build())
}
