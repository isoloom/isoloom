//! DigitalOcean's `cloud-docker` module: a VPC, the operator's key, a Debian droplet, and a
//! firewall.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Module, list, reference, string};
use isoloom_spec::model::Protocol;

use super::{Way, region, rule, ssh_key, vpc};
use crate::cloud::instance::{self, tag_list};
use crate::cloud::operator;
use crate::docker_host::{self, Access, AutoStop, Sizing, published};
use crate::module::providers::{self, terraform};
use crate::module::variable::Variable;

/// The module's resources and the host's set-up, from the plan.
pub(crate) fn write(env: &Environment, m: &mut Module) {
    m.block(terraform(&[providers::DIGITALOCEAN, providers::LOCAL]));
    let size = Sizing::of(env).pick(["s-2vcpu-4gb", "s-4vcpu-8gb", "s-8vcpu-16gb"]);
    m.block(region());
    m.block(Variable::new("size", "string").default(string(size)).block());
    m.blocks(operator::variables(env));
    m.blocks(instance::id_and_locals(env, Vec::new()));
    m.block(vpc("10.42.1.0/24"));
    m.block(ssh_key());
    m.block(droplet(env));
    m.block(firewall(env));
    let access = Access {
        host: reference("digitalocean_droplet.env.ipv4_address"),
        user: "root",
        replace_on: reference("digitalocean_droplet.env.id"),
        auto_stop: AutoStop::SetUp,
    };
    docker_host::environment(env, &access, m);
}

/// `resource "<kind>" "env"`, named after the environment.
fn resource(kind: &str) -> BlockBuilder {
    crate::module::resource(kind, "env").add_attribute(("name", reference("local.name")))
}

/// Debian 12 with the operator's key.
fn droplet(env: &Environment) -> Block {
    resource("digitalocean_droplet")
        .add_attribute(("region", reference("var.region")))
        .add_attribute(("size", reference("var.size")))
        .add_attribute(("image", string("debian-12-x64")))
        .add_attribute(("vpc_uuid", reference("digitalocean_vpc.env.id")))
        .add_attribute(("ssh_keys", list([reference("digitalocean_ssh_key.env.id")])))
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
