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
mod metadata;
mod network;
mod private_dns;
mod published;

use std::net::Ipv4Addr;

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, attr, boolean, call, entry, index, list, number, object, reference, string};
use isoloom_spec::{DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::{SHAPE, availability_domain, availability_domains, owned, ubuntu_images};
use crate::cloud::Cloud;
use crate::cloud::instance::{self, name_with};
use crate::cloud::vm::rules::{FIREWALLS_TC, ImageTable, Images, Mtu, Networks, OsImages, PLAYBOOK_CHECKS, Rules, Volumes, listed};
use crate::cloud::vm::{self, Driver, disks, gateways, gpus::GpuSizes, hooks, interfaces, nested, outputs, own_image};
use crate::limits::Limits;
use crate::machine::vms;
use crate::module::{ident, providers, resource};

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
        table: &IMAGES,
        own: own_image::oci,
    },
};

/// The SSH user of every instance (Ubuntu's).
pub(super) const USER: &str = "ubuntu";

/// Why Oracle Cloud can't run this environment, if it can't: its rules, then what its VCN can't
/// hold.
fn refusal(env: &Environment) -> Option<String> {
    RULES.linux_refusal(env).or_else(|| network::refusal(env)).or_else(|| ipv6::refusal(env))
}

/// The Ubuntu version each OS runs on: Oracle Cloud has no Debian image, so Debian labs fall back
/// to the newest Ubuntu LTS, as cloud-docker does.
const IMAGES: UbuntuImages = UbuntuImages(OsImages(&[
    ("ubuntu-22.04", "22.04"),
    ("ubuntu-24.04", NEWEST_UBUNTU),
    ("debian-12", NEWEST_UBUNTU),
    ("debian-13", NEWEST_UBUNTU),
]));

/// The newest Ubuntu LTS, which an OS with no image of its own runs on.
const NEWEST_UBUNTU: &str = "24.04";

/// Oracle Cloud's images: Canonical's Ubuntu, by the version each OS runs on.
#[derive(Debug)]
struct UbuntuImages(OsImages<&'static str>);

impl ImageTable for UbuntuImages {
    fn oses(&self) -> Vec<&'static str> {
        self.0.oses()
    }

    /// Ubuntu's own versions, then the OSes that run on one (`Debian 12/13 on Ubuntu 24.04`).
    fn listed(&self) -> String {
        let own = |os: &str, v: &str| os.strip_prefix("ubuntu-") == Some(v);
        let mut parts = vec![listed(self.0.0.iter().filter(|(os, v)| own(os, v)).map(|(os, _)| *os))];
        let mut versions: Vec<&str> = self.0.0.iter().filter(|(os, v)| !own(os, v)).map(|(_, v)| *v).collect();
        versions.dedup();
        for v in versions {
            let on = self.0.0.iter().filter(|(os, u)| *u == v && !own(os, u)).map(|(os, _)| *os);
            parts.push(format!("{} on Ubuntu {v}", listed(on)));
        }
        parts.join(", ")
    }
}

/// The Ubuntu version an OS runs on (the newest LTS for one Oracle Cloud has no image for, which
/// the refusal already turned away).
pub(super) fn ubuntu(os: &str) -> &'static str {
    IMAGES.0.of(os).unwrap_or(NEWEST_UBUNTU)
}

/// The `oci_core_images` data source of an Ubuntu version.
pub(super) fn image_source(version: &str) -> String {
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
    base: super::BASE,
    requires: (&[providers::OCI], &[]),
    refusal,
    // No cap on its user data that the module checks: its machines are written as they are.
    write: |env, m| {
        write(env, m);
        Ok(())
    },
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

/// Machine `name`'s instance at its fixed address, set up from its user data (its other VNICs
/// found at boot through the metadata service), its VNICs and further addresses.
fn machine(env: &Environment, (name, mp): (&str, &MachinePlan), (net, address): (&str, Ipv4Addr), m: &mut Module) {
    let group = published::group(name, mp, m);
    let mut at_boot = interfaces::up_found(env, &mp.addresses, &|_, a| metadata::mac(a));
    at_boot.extend(ipv6::at_boot(env, mp));
    at_boot.extend(gateways::at_boot(mp));
    let setup = hooks::user_data(env, (name, mp), USER, at_boot);
    m.commented(
        &format!("Machine `{name}`, at its fixed address {address}."),
        instance(env, (name, mp), (net, &address.to_string()), (group, setup)),
    );
    vnics(env, (name, mp), m);
    addresses::blocks((name, mp), m);
}

/// Machine `name`'s instance, its first VNIC on network `net` at `address` (with its published
/// ports' `group`), set up by `setup` (its user data, which cloud-init runs).
fn instance(env: &Environment, (name, mp): (&str, &MachinePlan), (net, address): (&str, &str), (group, setup): (Option<Expression>, Expression)) -> Block {
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
        ("user_data", call("base64encode", [setup])),
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
        .add_block(instance::set_up_once(entry(reference("metadata"), "user_data")).build())
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
