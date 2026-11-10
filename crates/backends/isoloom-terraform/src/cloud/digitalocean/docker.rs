//! DigitalOcean's `cloud-docker` module: a VPC, the operator's key, a droplet of the Docker
//! host's OS, and a firewall.

use crate::hcl_builder::{Block, BlockBuilder, Expression, Module, list, reference, string};
use isoloom_plan::Environment;
use isoloom_spec::model::Protocol;

use super::{Way, rule, ssh_key, vm, vpc};
use crate::cloud::docker::{Driver, HOST_OS, HostSizes, SizeVariable};
use crate::cloud::instance::{self, tag_list};
use crate::docker_host::published;
use crate::module::providers;

/// Its `cloud-docker` driver.
pub(crate) const DRIVER: Driver = Driver {
    base: super::BASE,
    requires: providers::DIGITALOCEAN,
    size: Some(SizeVariable {
        name: "size",
        sizes: HostSizes::Table(super::vm::DROPLETS),
    }),
    resources,
};

/// The cloud's resources, after the module's head, `user_data` in the droplet; the Docker
/// host's address.
fn resources(env: &Environment, m: &mut Module, user_data: Expression) -> Expression {
    m.block(vpc(crate::cloud::docker::HOST_SUBNET));
    m.block(ssh_key());
    m.block(droplet(env, user_data));
    m.block(firewall(env));
    reference("digitalocean_droplet.env.ipv4_address")
}

/// `resource "<kind>" "env"`, named after the environment.
fn resource(kind: &str) -> BlockBuilder {
    crate::module::resource(kind, "env").add_attribute(("name", reference("local.name")))
}

/// The Docker host's OS from DigitalOcean's image table, with the runner's key, its expiry in its user data, run once ([`instance::set_up_once`]).
fn droplet(env: &Environment, user_data: Expression) -> Block {
    resource("digitalocean_droplet")
        .add_attribute(("region", reference("var.region")))
        .add_attribute(("size", reference("var.size")))
        .add_attribute(("image", string(vm::image(HOST_OS).unwrap_or_default())))
        .add_attribute(("vpc_uuid", reference("digitalocean_vpc.env.id")))
        .add_attribute(("ssh_keys", list([reference("digitalocean_ssh_key.env.id")])))
        .add_attribute(("user_data", user_data))
        .add_attribute(("tags", tag_list(env)))
        .add_block(instance::set_up_once(reference("user_data")).build())
        .build()
}

/// SSH and the published ports in from `allowed_cidr`, everything out (ICMP too, as on
/// `cloud-vm`: ping and path MTU discovery from the containers).
fn firewall(env: &Environment) -> Block {
    let allowed = || list([reference("var.allowed_cidr")]);
    let anywhere = || list([string("0.0.0.0/0"), string("::/0")]);
    let mut b = resource("digitalocean_firewall")
        .add_attribute(("droplet_ids", list([reference("digitalocean_droplet.env.id")])))
        .add_block(rule(Way::In, "tcp", Some("22"), allowed()));
    for (protocol, p) in published(env, Protocol::Tcp)
        .into_iter()
        .map(|p| ("tcp", p))
        .chain(published(env, Protocol::Udp).into_iter().map(|p| ("udp", p)))
    {
        b = b.add_block(rule(Way::In, protocol, Some(&p.to_string()), allowed()));
    }
    b.add_block(rule(Way::Out, "tcp", Some("1-65535"), anywhere()))
        .add_block(rule(Way::Out, "udp", Some("1-65535"), anywhere()))
        .add_block(rule(Way::Out, "icmp", None, anywhere()))
        .build()
}
