//! DigitalOcean: a VPC, the operator's key, a Debian droplet, and a firewall.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{BinaryOperator, Block, Expression, Module, Template, binary, call, conditional, list, reference, string};
use isoloom_spec::model::Protocol;

use super::host::{self, Reach, Sizing, published};
use crate::vm::module::{Variable, terraform};

/// The module, from the plan.
pub fn module(env: &Environment) -> Module {
    let mut m = Module::new(host::preamble(super::DIR, "digitalocean", "-var allowed_cidr=<your IP>/32 "));
    m.block(terraform(&[("digitalocean", "digitalocean/digitalocean", "~> 2.0")]));
    let size = Sizing::of(env).pick(["s-2vcpu-4gb", "s-4vcpu-8gb", "s-8vcpu-16gb"]);
    m.blocks([
        Variable::new("region", "string").default(string("fra1")).block(),
        Variable::new("size", "string").default(string(size)).block(),
    ]);
    m.blocks(host::variables(env, true));
    m.block(host::id());
    m.block(host::locals(env, Vec::new()));
    m.block(
        resource("digitalocean_vpc")
            .add_attribute(("region", reference("var.region")))
            .add_attribute(("ip_range", string("10.42.1.0/24")))
            .build(),
    );
    m.block(
        resource("digitalocean_ssh_key")
            .add_attribute(("public_key", reference("var.ssh_public_key")))
            .build(),
    );
    m.block(droplet(env));
    m.block(firewall(env));
    let reach = Reach {
        host: reference("digitalocean_droplet.env.ipv4_address"),
        user: "root",
        replace_on: reference("digitalocean_droplet.env.id"),
        auto_stop: true,
    };
    host::environment(env, &reach, &mut m);
    m
}

fn resource(kind: &str) -> isoloom_render::hcl_builder::BlockBuilder {
    Block::builder("resource")
        .add_label(kind)
        .add_label("env")
        .add_attribute(("name", reference("local.name")))
}

/// The environment's tags: DigitalOcean's are plain words, so the end date is one of them.
pub fn word_tags(env: &Environment) -> Expression {
    let expires = conditional(
        binary(reference("var.expires_at"), BinaryOperator::Eq, string("")),
        list([]),
        list([Template::text("isoloom-expires-").with_expr(reference("var.expires_at")).into_expression()]),
    );
    call(
        "concat",
        [list([string("isoloom"), string(env.name.as_str()), reference("local.name")]), expires],
    )
}

fn droplet(env: &Environment) -> Block {
    resource("digitalocean_droplet")
        .add_attribute(("region", reference("var.region")))
        .add_attribute(("size", reference("var.size")))
        .add_attribute(("image", string("debian-12-x64")))
        .add_attribute(("vpc_uuid", reference("digitalocean_vpc.env.id")))
        .add_attribute(("ssh_keys", list([reference("digitalocean_ssh_key.env.id")])))
        .add_attribute(("tags", word_tags(env)))
        .build()
}

/// One rule in from `allowed_cidr`.
fn inbound(protocol: &str, port: u16) -> Block {
    Block::builder("inbound_rule")
        .add_attribute(("protocol", string(protocol)))
        .add_attribute(("port_range", string(port.to_string())))
        .add_attribute(("source_addresses", list([reference("var.allowed_cidr")])))
        .build()
}

/// Everything out over one protocol.
fn outbound(protocol: &str) -> Block {
    Block::builder("outbound_rule")
        .add_attribute(("protocol", string(protocol)))
        .add_attribute(("port_range", string("1-65535")))
        .add_attribute(("destination_addresses", list([string("0.0.0.0/0"), string("::/0")])))
        .build()
}

/// SSH and the published ports in from `allowed_cidr`, everything out.
fn firewall(env: &Environment) -> Block {
    let mut b = resource("digitalocean_firewall")
        .add_attribute(("droplet_ids", list([reference("digitalocean_droplet.env.id")])))
        .add_block(inbound("tcp", 22));
    for p in published(env, Protocol::Tcp) {
        b = b.add_block(inbound("tcp", p));
    }
    for p in published(env, Protocol::Udp) {
        b = b.add_block(inbound("udp", p));
    }
    b.add_block(outbound("tcp")).add_block(outbound("udp")).build()
}
