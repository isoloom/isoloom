//! DigitalOcean's `cloud-docker` module: a VPC, the operator's key, a Debian droplet, and a
//! firewall.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, list, reference, string};
use isoloom_spec::model::Protocol;

use super::{Way, region, rule, ssh_key, vpc};
use crate::cloud::docker::{Driver, Host, SizeVariable};
use crate::cloud::instance::tag_list;
use crate::docker_host::published;
use crate::module::providers;

/// Its `cloud-docker` driver.
pub(crate) const DRIVER: Driver = Driver {
    requires: providers::DIGITALOCEAN,
    variables: || vec![region()],
    size: Some(SizeVariable {
        name: "size",
        default: |s| s.pick(["s-2vcpu-4gb", "s-4vcpu-8gb", "s-8vcpu-16gb"]),
    }),
    // No provider block: the provider reads its token from the environment.
    provider: |_, _| Vec::new(),
    resources,
    user: "root",
};

/// The cloud's resources, after the module's head, `user_data` in the droplet; the host they
/// make.
fn resources(env: &Environment, m: &mut Module, user_data: Expression) -> Host {
    m.block(vpc("10.42.1.0/24"));
    m.block(ssh_key());
    m.block(droplet(env, user_data));
    m.block(firewall(env));
    Host {
        address: reference("digitalocean_droplet.env.ipv4_address"),
        user: "root",
    }
}

/// `resource "<kind>" "env"`, named after the environment.
fn resource(kind: &str) -> BlockBuilder {
    crate::module::resource(kind, "env").add_attribute(("name", reference("local.name")))
}

/// Debian 12 with the runner's key, set up from its user data.
fn droplet(env: &Environment, user_data: Expression) -> Block {
    resource("digitalocean_droplet")
        .add_attribute(("region", reference("var.region")))
        .add_attribute(("size", reference("var.size")))
        .add_attribute(("image", string("debian-12-x64")))
        .add_attribute(("vpc_uuid", reference("digitalocean_vpc.env.id")))
        .add_attribute(("ssh_keys", list([reference("digitalocean_ssh_key.env.id")])))
        .add_attribute(("user_data", user_data))
        .add_attribute(("tags", tag_list(env)))
        .build()
}

/// SSH and the published ports in from `allowed_cidr`, everything out.
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
        .build()
}
