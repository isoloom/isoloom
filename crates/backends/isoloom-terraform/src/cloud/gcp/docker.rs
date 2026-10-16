//! Google Cloud's `cloud-docker` module: a project of its own unless one is given, a network
//! and subnet, a firewall, and a Debian instance.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, Template, boolean, list, number, object, reference, string};
use isoloom_spec::model::Protocol;

use super::{provider_and_project, public_ip, variables};
use crate::cloud::docker::{Driver, Host, SizeVariable};
use crate::cloud::instance;
use crate::docker_host::{port_strings, published, tcp_ports};
use crate::module::providers;

/// Its `cloud-docker` driver.
pub(crate) const DRIVER: Driver = Driver {
    requires: providers::GOOGLE,
    variables: || variables().into(),
    size: Some(SizeVariable {
        name: "machine_type",
        default: |s| s.pick(["e2-medium", "e2-standard-2", "e2-standard-4"]),
    }),
    provider: provider_and_project,
    resources,
    user: "isoloom",
};

/// The cloud's resources, after the module's head, `setup` as the instance's startup script;
/// the host they make.
fn resources(env: &Environment, m: &mut Module, setup: Expression) -> Host {
    m.blocks(network(env));
    m.block(vm(env, setup));
    Host {
        address: public_ip("env"),
        user: "isoloom",
    }
}

/// `resource "<kind>" "env"` in the project, named after the environment.
fn resource(kind: &str) -> BlockBuilder {
    Block::builder("resource")
        .add_label(kind)
        .add_label("env")
        .add_attribute(("project", reference("local.project")))
        .add_attribute(("name", reference("local.name")))
}

/// One `allow` of the firewall.
fn allow(protocol: &str, ports: &[u16]) -> Block {
    Block::builder("allow")
        .add_attribute(("protocol", string(protocol)))
        .add_attribute(("ports", port_strings(ports)))
        .build()
}

/// The network, its subnet, and the firewall: SSH and the published ports from `allowed_cidr`.
fn network(env: &Environment) -> Vec<Block> {
    let tcp = tcp_ports(env);
    let udp = published(env, Protocol::Udp);
    let mut firewall = resource("google_compute_firewall")
        .add_attribute(("network", reference("google_compute_network.env.id")))
        .add_attribute(("source_ranges", list([reference("var.allowed_cidr")])))
        .add_block(allow("tcp", &tcp));
    if !udp.is_empty() {
        firewall = firewall.add_block(allow("udp", &udp));
    }
    vec![
        resource("google_compute_network")
            .add_attribute(("auto_create_subnetworks", boolean(false)))
            .build(),
        resource("google_compute_subnetwork")
            .add_attribute(("network", reference("google_compute_network.env.id")))
            .add_attribute(("ip_cidr_range", string("10.42.1.0/24")))
            .add_attribute(("region", reference("var.region")))
            .build(),
        firewall.build(),
    ]
}

/// Debian 12 in the region's first zone, the `isoloom` user with the runner's key, set up by its
/// startup script (Google's Debian images have no cloud-init; the script runs at every boot).
fn vm(env: &Environment, setup: Expression) -> Block {
    let ssh_keys = Template::text("isoloom:").with_expr(reference("var.ssh_public_key"));
    resource("google_compute_instance")
        .add_attribute(("machine_type", reference("var.machine_type")))
        .add_attribute(("zone", Template::new().with_expr(reference("var.region")).with_text("-a").into_expression()))
        .add_attribute(("labels", instance::tags(env)))
        .add_block(
            Block::builder("boot_disk")
                .add_block(
                    Block::builder("initialize_params")
                        .add_attribute(("image", string("debian-cloud/debian-12")))
                        .add_attribute(("size", number(30)))
                        .build(),
                )
                .build(),
        )
        .add_block(
            Block::builder("network_interface")
                .add_attribute(("subnetwork", reference("google_compute_subnetwork.env.id")))
                .add_block(Block::builder("access_config").build())
                .build(),
        )
        .add_attribute(("metadata", object([("ssh-keys", ssh_keys.into_expression()), ("startup-script", setup)])))
        .build()
}
