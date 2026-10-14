//! Oracle Cloud (OCI) `cloud-vm`: `.isoloom/cloud-vm/oci/main.tf`, rendered from the plan
//! through the VM core. One compute instance per machine on one VCN, a subnet per network; OCI
//! pins a VNIC's private IP, so every machine keeps its spec address, with a VNIC on each of its
//! networks. Each network has its own security list (`network`), and a network a machine routes
//! goes through it. Linux labs: the backend refuses Windows and a controller.
//!
//! Each machine's published ports open on its own VNIC only, through a network security group
//! of its own; the subnet's security list holds what every machine shares (the VCN, SSH).
//!
//! The image is looked up, not given: a `data "oci_core_images"` finds the newest Canonical
//! Ubuntu for the shape, as the cloud-docker driver does. Oracle Cloud ships no Debian platform
//! image, so Debian labs run on Ubuntu too; the SSH login is `ubuntu` either way.

use isoloom_plan::{Capabilities, Environment};
use isoloom_render::backend::{Backend, Files, RenderError};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, attr, boolean, call, index, list, number, object, reference, string};
use isoloom_render::{GeneratedFile, header};
use isoloom_spec::model::Target;
use isoloom_spec::{DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::shared::{self, Networks, Rules};
use crate::vm::cloud::{self, Reach, auto_stop, name_with, shutdown_script};
use crate::vm::gpus::GpuSizes;
use crate::vm::linux;
use crate::vm::module::{Variable, project_lists, terraform};
use crate::vm::versions;
use crate::vm::{gateways, os, own_image, res, vms};

mod network;
mod published;

/// The module's folder under `.isoloom/cloud-vm/`.
const SUB: &str = "oci";

/// The shape every instance has, sized by its memory.
const SHAPE: &str = "VM.Standard.E4.Flex";

/// What Oracle Cloud takes: Ubuntu, and Debian on the newest Ubuntu LTS (it has no Debian image).
/// Any other OS needs an image of its own (`vm.image.oci`) rather than running Ubuntu in its place.
const RULES: Rules = Rules {
    cloud: "Oracle Cloud",
    networks: Networks::Several,
    limits: shared::limits(
        "arm64 on Oracle Cloud comes later (Ampere shapes and arm64 images)",
        "dual-stack virtual cloud networks on Oracle Cloud come later",
        None,
    ),
    has_image: |os| matches!(os, "debian-12" | "debian-13" | "ubuntu-22.04" | "ubuntu-24.04"),
    images: "Ubuntu 22.04/24.04, Debian 12/13 on Ubuntu 24.04",
    tpm: true,
    own: own_image::oci,
    gpus: GpuSizes {
        cloud: "Oracle Cloud",
        gpu: "",
        sizes: &[],
        none: "GPUs on Oracle Cloud come later (its GPU shapes are fixed, not the flexible shape every machine has, and take images built for them)",
    },
    // The AMD shape exposes its virtualization instructions when asked (`platform_config`).
    nests: true,
};

/// The `cloud-vm` module on Oracle Cloud.
#[derive(Debug, Clone, Copy, Default)]
pub struct OciBackend;

impl Backend for OciBackend {
    fn target(&self) -> Target {
        Target::CloudVm
    }

    fn capabilities(&self) -> Capabilities {
        shared::capabilities()
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        shared::render(env, &RULES, module)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        isoloom_plan::refusal(&env.requirements, &self.capabilities())
            .map(|r| r.to_string())
            .or_else(|| shared::refusal(env, &RULES))
            .or_else(|| network::refusal(env))
    }
}

/// The SSH user of every instance (Ubuntu's).
const USER: &str = "ubuntu";

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

const USAGE: &str = "# Start:  terraform -chdir=.isoloom/cloud-vm/oci init && terraform -chdir=.isoloom/cloud-vm/oci apply \\\n#           -var compartment_id=<compartment ocid> -var allowed_cidr=<your IP>/32 \\\n#           -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-vm/oci destroy (same variables)\n# Auth:   the oci provider reads ~/.oci/config; region defaults, override with -var region=<id>.\n";

/// The module: the images, the VCN, its security list and subnet, each machine, the outputs.
fn module(env: &Environment) -> Result<GeneratedFile, isoloom_render::hcl_builder::Error> {
    let env = &*gateways::relocated(env);
    let mut m = Module::new(format!("{}{USAGE}", header("#")));
    m.block(terraform(&[versions::OCI, versions::LOCAL]));
    m.block(
        Variable::new("compartment_id", "string")
            .describe("The compartment the environment goes in")
            .block(),
    );
    m.block(
        Variable::new("region", "string")
            .default(string("eu-paris-1"))
            .describe("The OCI region to launch in")
            .block(),
    );
    m.blocks(cloud::variables(env));
    m.block(
        Block::builder("provider")
            .add_label("oci")
            .add_attribute(("region", reference("var.region")))
            .build(),
    );
    m.blocks(cloud::id_and_locals(env, Vec::new()));
    m.commented(
        "The compartment's availability domains; the instances land in the first.",
        Block::builder("data")
            .add_label("oci_identity_availability_domains")
            .add_label("ads")
            .add_attribute(("compartment_id", reference("var.compartment_id")))
            .build(),
    );
    images(env, &mut m);
    network::blocks(env, &mut m);
    crate::vm::private_dns::oci(env, &mut m);
    for (name, _) in vms(env) {
        instance(env, name, &mut m);
    }
    network::gateway_routes(env, &mut m);
    let reach = Reach {
        public_ip: &public_ip,
        user: &|_| USER.to_string(),
        controller: None,
    };
    cloud::outputs(env, &reach, &mut m);
    m.fragment(&project_lists(true));
    Ok(GeneratedFile {
        path: shared::path(SUB),
        contents: m.render()?,
    })
}

/// An instance's public address.
fn public_ip(name: &str) -> Expression {
    reference(&format!("oci_core_instance.{}.public_ip", res(name)))
}

/// A resource block.
fn resource(kind: &str, name: &str) -> BlockBuilder {
    Block::builder("resource").add_label(kind).add_label(name)
}

/// In the compartment, named after the environment.
fn named(b: BlockBuilder) -> BlockBuilder {
    b.add_attribute(("compartment_id", reference("var.compartment_id")))
}

/// The newest Canonical Ubuntu for the shape, one lookup per version the machines use.
fn images(env: &Environment, m: &mut Module) {
    let mut versions: Vec<&str> = vms(env)
        .filter(|(_, mp)| own_image::of(mp, own_image::oci).is_none())
        .map(|(_, mp)| ubuntu(os(mp).unwrap_or_default()))
        .collect();
    versions.sort_unstable();
    versions.dedup();
    for v in versions {
        m.commented(
            &format!("Ubuntu {v}: Oracle Cloud has no Debian image of its own."),
            named(Block::builder("data").add_label("oci_core_images").add_label(image_source(v)))
                .add_attribute(("operating_system", string("Canonical Ubuntu")))
                .add_attribute(("operating_system_version", string(v)))
                .add_attribute(("shape", string(SHAPE)))
                .add_attribute(("sort_by", string("TIMECREATED")))
                .add_attribute(("sort_order", string("DESC")))
                .build(),
        );
    }
}

/// A machine's instance at its fixed address, and its set-up over SSH.
fn instance(env: &Environment, name: &str, m: &mut Module) {
    let mp = &env.machines[name];
    let id = res(name);
    let Some((net, address)) = mp.addresses.first().map(|(n, a)| (n.clone(), a.to_string())) else {
        return;
    };
    // A gateway forwards traffic that isn't its own; a machine on several networks answers from
    // the interface its default route leaves by.
    let forwards = mp.is_gateway || mp.addresses.len() > 1;
    let (ocpus, memory_gb) = sizing(mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB));
    let shape = Block::builder("shape_config")
        .add_attribute(("ocpus", number(u64::from(ocpus))))
        .add_attribute(("memory_in_gbs", number(u64::from(memory_gb))))
        .build();
    let images = reference(&format!("data.oci_core_images.{}.images", image_source(ubuntu(os(mp).unwrap_or_default()))));
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
    let vnic = primary_vnic(name, &net, &address, forwards, published::group(name, mp, m));
    let script = shutdown_script();
    let metadata = object([
        ("ssh_authorized_keys", reference("var.ssh_public_key")),
        ("user_data", call("base64encode", [auto_stop(script, string("#!/bin/sh\n"))])),
    ]);
    m.commented(
        &format!("Machine `{name}`, at its fixed address {address}."),
        named(resource("oci_core_instance", &id))
            .add_attribute(("availability_domain", availability_domain()))
            .add_attribute(("display_name", name_with(&format!("-{name}"))))
            .add_attribute(("shape", string(SHAPE)))
            .add_block(shape)
            .add_blocks(platform(mp))
            .add_block(source)
            .add_block(vnic)
            .add_attribute(("metadata", metadata))
            .build(),
    );
    vnics(env, name, m);
    let mac = |n: &str| call("lower", [reference(&format!("data.oci_core_vnic.{id}_{}.mac_address", res(n)))]);
    let mut at_boot = cloud::interfaces(env, &mp.addresses, &mac);
    at_boot.extend(gateways::at_boot(mp));
    let machine = linux::Machine {
        env,
        name,
        replace_on: reference(&format!("oci_core_instance.{id}.id")),
        host: public_ip(name),
        user: USER.into(),
        hooks: shared::hooks(mp, at_boot),
    };
    m.block(linux::provisioner(machine).block());
}

/// The instance's first VNIC, on its first network at its address, with its published ports'
/// group when it has one.
fn primary_vnic(name: &str, net: &str, address: &str, forwards: bool, group: Option<Expression>) -> Block {
    let mut vnic = Block::builder("create_vnic_details")
        .add_attribute(("subnet_id", reference(&format!("oci_core_subnet.{}.id", res(net)))))
        .add_attribute(("private_ip", string(address)))
        .add_attribute(("assign_public_ip", boolean(true)))
        .add_attribute(("hostname_label", string(name)))
        .add_attribute(("skip_source_dest_check", boolean(forwards)));
    if let Some(group) = group {
        vnic = vnic.add_attribute(("nsg_ids", list([group])));
    }
    vnic.build()
}

/// A VNIC on each network after the first, at the machine's address there (forwarding, as the
/// first), and its MAC address, which finds it at boot.
fn vnics(env: &Environment, name: &str, m: &mut Module) {
    let id = res(name);
    for (net, address) in env.machines[name].addresses.iter().skip(1) {
        let nic = format!("{id}_{}", res(net));
        let details = Block::builder("create_vnic_details")
            .add_attribute(("subnet_id", reference(&format!("oci_core_subnet.{}.id", res(net)))))
            .add_attribute(("private_ip", string(address.to_string())))
            .add_attribute(("assign_public_ip", boolean(false)))
            .add_attribute(("skip_source_dest_check", boolean(true)))
            .build();
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
fn platform(mp: &isoloom_plan::MachinePlan) -> Option<Block> {
    let (tpm, nested) = (mp.tpm.is_some(), crate::vm::nested::wanted(mp));
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

/// The first availability domain's name.
fn availability_domain() -> Expression {
    let ads = index(reference("data.oci_identity_availability_domains.ads.availability_domains"), 0);
    attr(ads, "name")
}
