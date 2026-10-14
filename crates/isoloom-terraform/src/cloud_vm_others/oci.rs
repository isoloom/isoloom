//! Oracle Cloud (OCI) `cloud-vm`: `.isoloom/cloud-vm/oci/main.tf`, rendered from the plan
//! through the VM core. One compute instance per machine on one VCN; OCI pins a VNIC's private
//! IP, so every machine keeps its spec address. Single-network Linux labs only: the backend
//! refuses several networks, Windows and a controller.
//!
//! Each machine's published ports open on its own VNIC only, through a network security group
//! of its own; the subnet's security list holds what every machine shares (the VCN, SSH).
//!
//! The image is looked up, not given: a `data "oci_core_images"` finds the newest Canonical
//! Ubuntu for the shape, as the cloud-docker driver does. Oracle Cloud ships no Debian platform
//! image, so Debian labs run on Ubuntu too; the SSH login is `ubuntu` either way.

use isoloom_plan::{Capabilities, Environment, MachinePlan};
use isoloom_render::backend::{Backend, Files, RenderError};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, attr, boolean, call, index, list, number, object, reference, string};
use isoloom_render::{GeneratedFile, header};
use isoloom_spec::model::Target;
use isoloom_spec::{DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::single_network::{self, Rules};
use crate::vm::cloud::{self, Reach, auto_stop, name_with, shutdown_script};
use crate::vm::gpus::GpuSizes;
use crate::vm::linux;
use crate::vm::module::{Variable, project_lists, terraform};
use crate::vm::versions;
use crate::vm::{os, own_image, res, vms};

/// The module's folder under `.isoloom/cloud-vm/`.
const SUB: &str = "oci";

/// The shape every instance has, sized by its memory.
const SHAPE: &str = "VM.Standard.E4.Flex";

/// What Oracle Cloud takes: Ubuntu, and Debian on the newest Ubuntu LTS (it has no Debian image).
/// Any other OS needs an image of its own (`vm.image.oci`) rather than running Ubuntu in its place.
const RULES: Rules = Rules {
    cloud: "Oracle Cloud",
    single_machine: false,
    limits: single_network::limits(
        "arm64 on Oracle Cloud comes later (Ampere shapes and arm64 images)",
        "dual-stack virtual cloud networks on Oracle Cloud come later",
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
        single_network::capabilities()
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        single_network::render(env, &RULES, module)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        isoloom_plan::refusal(&env.requirements, &self.capabilities())
            .map(|r| r.to_string())
            .or_else(|| single_network::refusal(env, &RULES))
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
    network(env, &mut m);
    for (name, _) in vms(env) {
        instance(env, name, &mut m);
    }
    let reach = Reach {
        public_ip: &public_ip,
        user: &|_| USER.to_string(),
        controller: None,
    };
    cloud::outputs(env, &reach, &mut m);
    m.fragment(&project_lists(true));
    Ok(GeneratedFile {
        path: single_network::path(SUB),
        contents: m.render()?,
    })
}

/// An instance's public address.
fn public_ip(name: &str) -> Expression {
    reference(&format!("oci_core_instance.{}.public_ip", res(name)))
}

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

/// The VCN with the network's range, routed to the internet; its security list and subnet.
fn network(env: &Environment, m: &mut Module) {
    let cidr = env.networks.values().next().map(|n| n.cidr.as_str()).unwrap_or_default();
    let in_vcn = |b: BlockBuilder| {
        named(b)
            .add_attribute(("vcn_id", reference("oci_core_vcn.env.id")))
            .add_attribute(("display_name", reference("local.name")))
    };
    m.commented(
        "The environment's one network: a VCN with the spec's range, routed to the internet.",
        named(resource("oci_core_vcn", "env"))
            .add_attribute(("cidr_block", string(cidr)))
            .add_attribute(("display_name", reference("local.name")))
            .build(),
    );
    m.block(in_vcn(resource("oci_core_internet_gateway", "env")).build());
    let route = Block::builder("route_rules")
        .add_attribute(("destination", string("0.0.0.0/0")))
        .add_attribute(("network_entity_id", reference("oci_core_internet_gateway.env.id")))
        .build();
    m.block(in_vcn(resource("oci_core_route_table", "env")).add_block(route).build());
    m.commented(
        "What may reach every machine: everything inside the VCN, and SSH from allowed_cidr; egress\nopen. A machine's published ports are its own security group's.",
        security_list(in_vcn(resource("oci_core_security_list", "env")), cidr),
    );
    m.block(
        named(resource("oci_core_subnet", "env"))
            .add_attribute(("vcn_id", reference("oci_core_vcn.env.id")))
            .add_attribute(("cidr_block", string(cidr)))
            .add_attribute(("display_name", reference("local.name")))
            .add_attribute(("route_table_id", reference("oci_core_route_table.env.id")))
            .add_attribute(("security_list_ids", list([reference("oci_core_security_list.env.id")])))
            .build(),
    );
}

/// An ingress rule; on one port of TCP (6) or UDP (17) when `port` is given.
fn ingress(description: &str, source: Expression, protocol: &str, port: Option<u16>) -> Block {
    let mut b = Block::builder("ingress_security_rules")
        .add_attribute(("description", string(description)))
        .add_attribute(("source", source))
        .add_attribute(("protocol", string(protocol)));
    if let Some(p) = port {
        let options = if protocol == "17" { "udp_options" } else { "tcp_options" };
        let range = Block::builder(options)
            .add_attribute(("min", number(u64::from(p))))
            .add_attribute(("max", number(u64::from(p))))
            .build();
        b = b.add_block(range);
    }
    b.build()
}

/// Everything out, everything from the VCN, SSH from `allowed_cidr`.
fn security_list(b: BlockBuilder, cidr: &str) -> Block {
    let egress = Block::builder("egress_security_rules")
        .add_attribute(("destination", string("0.0.0.0/0")))
        .add_attribute(("protocol", string("all")))
        .build();
    let allowed = || reference("var.allowed_cidr");
    b.add_block(egress)
        .add_block(ingress("the VCN itself", string(cidr), "all", None))
        .add_block(ingress("SSH from allowed_cidr", allowed(), "6", Some(22)))
        .build()
}

/// A machine's own network security group, with a rule per port it publishes, so a port opens
/// on the machine that publishes it and no other. Returns the group's id, or `None` when the
/// machine publishes nothing.
fn published(name: &str, mp: &MachinePlan, m: &mut Module) -> Option<Expression> {
    if mp.ports.is_empty() {
        return None;
    }
    let id = res(name);
    m.commented(
        &format!("The ports `{name}` publishes, from allowed_cidr: on its VNIC only."),
        named(resource("oci_core_network_security_group", &id))
            .add_attribute(("vcn_id", reference("oci_core_vcn.env.id")))
            .add_attribute(("display_name", name_with(&format!("-{name}"))))
            .build(),
    );
    let mut ports: Vec<(bool, u16)> = mp.ports.iter().map(|p| (p.protocol.is_tcp(), p.publish)).collect();
    ports.sort_unstable_by_key(|&(tcp, port)| (!tcp, port));
    ports.dedup();
    for (tcp, port) in ports {
        m.block(published_rule(&id, tcp, port));
    }
    Some(reference(&format!("oci_core_network_security_group.{id}.id")))
}

/// The rule opening one published port (TCP is 6, UDP 17) of group `id`.
fn published_rule(id: &str, tcp: bool, port: u16) -> Block {
    let (protocol, options, label) = if tcp { ("6", "tcp_options", "tcp") } else { ("17", "udp_options", "udp") };
    let range = Block::builder("destination_port_range")
        .add_attribute(("min", number(u64::from(port))))
        .add_attribute(("max", number(u64::from(port))))
        .build();
    resource("oci_core_network_security_group_security_rule", &format!("{id}_{label}_{port}"))
        .add_attribute(("network_security_group_id", reference(&format!("oci_core_network_security_group.{id}.id"))))
        .add_attribute(("description", string("published")))
        .add_attribute(("direction", string("INGRESS")))
        .add_attribute(("protocol", string(protocol)))
        .add_attribute(("source", reference("var.allowed_cidr")))
        .add_attribute(("source_type", string("CIDR_BLOCK")))
        .add_block(Block::builder(options).add_block(range).build())
        .build()
}

/// A machine's instance at its fixed address, and its set-up over SSH.
fn instance(env: &Environment, name: &str, m: &mut Module) {
    let mp = &env.machines[name];
    let id = res(name);
    let address = mp.addresses.values().next().map(ToString::to_string).unwrap_or_default();
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
    let mut vnic = Block::builder("create_vnic_details")
        .add_attribute(("subnet_id", reference("oci_core_subnet.env.id")))
        .add_attribute(("private_ip", string(address.as_str())))
        .add_attribute(("assign_public_ip", boolean(true)))
        .add_attribute(("hostname_label", string(name)));
    if let Some(group) = published(name, mp, m) {
        vnic = vnic.add_attribute(("nsg_ids", list([group])));
    }
    let vnic = vnic.build();
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
    let machine = linux::Machine {
        env,
        name,
        replace_on: reference(&format!("oci_core_instance.{id}.id")),
        host: public_ip(name),
        user: USER.into(),
        hooks: single_network::hooks(mp, Vec::new()),
    };
    m.block(linux::provisioner(machine).block());
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
