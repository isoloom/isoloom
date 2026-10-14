//! Oracle Cloud (OCI) `cloud-vm`: `.isoloom/cloud-vm/oci/main.tf`, rendered from the plan
//! through the VM core. One compute instance per machine on one VCN; OCI pins a VNIC's private
//! IP, so every machine keeps its spec address. Single-network Linux labs only: the backend
//! refuses several networks, Windows and a controller.
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

use super::single_network::{self, Rules};
use crate::vm::cloud::{self, Reach, auto_stop, name_with, shutdown_script};
use crate::vm::linux;
use crate::vm::module::{Variable, project_lists, terraform};
use crate::vm::versions;
use crate::vm::{os, res, vms};

/// The module's folder under `.isoloom/cloud-vm/`.
const SUB: &str = "oci";

/// The shape every instance has, sized by its memory.
const SHAPE: &str = "VM.Standard.E4.Flex";

/// What Oracle Cloud takes: every OS runs on an Ubuntu image.
const RULES: Rules = Rules {
    cloud: "Oracle Cloud",
    single_machine: false,
    limits: single_network::limits(
        "arm64 on Oracle Cloud comes later (Ampere shapes and arm64 images)",
        "dual-stack virtual cloud networks on Oracle Cloud come later",
    ),
    has_image: |_| true,
    images: "Ubuntu 22.04/24.04",
    tpm: true,
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
    let mut versions: Vec<&str> = vms(env).map(|(_, mp)| ubuntu(os(mp).unwrap_or_default())).collect();
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
        "What may reach the machines: everything inside the VCN, SSH and the published ports from\nallowed_cidr; egress open.",
        security_list(env, in_vcn(resource("oci_core_security_list", "env")), cidr),
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

/// Everything out, everything from the VCN, SSH and the published ports from `allowed_cidr`.
fn security_list(env: &Environment, b: BlockBuilder, cidr: &str) -> Block {
    let egress = Block::builder("egress_security_rules")
        .add_attribute(("destination", string("0.0.0.0/0")))
        .add_attribute(("protocol", string("all")))
        .build();
    let allowed = || reference("var.allowed_cidr");
    let mut b = b
        .add_block(egress)
        .add_block(ingress("the VCN itself", string(cidr), "all", None))
        .add_block(ingress("SSH from allowed_cidr", allowed(), "6", Some(22)));
    for (protocol, port) in single_network::published_ports(env) {
        let number = if protocol.is_tcp() { "6" } else { "17" };
        b = b.add_block(ingress("published", allowed(), number, Some(port)));
    }
    b.build()
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
        .add_attribute(("source_id", attr(index(images, 0), "id")))
        .add_attribute(("boot_volume_size_in_gbs", number(u64::from(boot))))
        .build();
    let vnic = Block::builder("create_vnic_details")
        .add_attribute(("subnet_id", reference("oci_core_subnet.env.id")))
        .add_attribute(("private_ip", string(address.as_str())))
        .add_attribute(("assign_public_ip", boolean(true)))
        .add_attribute(("hostname_label", string(name)))
        .build();
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
            .add_blocks(shielded(mp.tpm.is_some()))
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

/// A shielded instance, when the machine has a TPM: its virtual TPM, secure and measured boot
/// (the platform of the AMD shape; Canonical's images are shielded-instance images).
fn shielded(tpm: bool) -> Option<Block> {
    tpm.then(|| {
        Block::builder("platform_config")
            .add_attribute(("type", string("AMD_VM")))
            .add_attribute(("is_secure_boot_enabled", boolean(true)))
            .add_attribute(("is_measured_boot_enabled", boolean(true)))
            .add_attribute(("is_trusted_platform_module_enabled", boolean(true)))
            .build()
    })
}

/// The first availability domain's name.
fn availability_domain() -> Expression {
    let ads = index(reference("data.oci_identity_availability_domains.ads.availability_domains"), 0);
    attr(ads, "name")
}
