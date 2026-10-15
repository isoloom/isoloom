//! Linode: a Debian instance with the operator's key, behind a firewall.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, Module, call, list, reference, string};
use isoloom_spec::model::Protocol;

use super::host::{self, Reach, Sizing, published};
use crate::vm::cloud;
use crate::vm::module::{Variable, terraform};
use crate::vm::versions;

/// The module, from the plan.
pub fn module(env: &Environment) -> Module {
    let mut m = Module::new(host::preamble(super::DIR, "linode", "-var allowed_cidr=<your IP>/32 "));
    m.block(terraform(&[versions::LINODE, versions::LOCAL]));
    let size = Sizing::of(env).pick(["g6-standard-2", "g6-standard-4", "g6-standard-6"]);
    m.blocks([
        Variable::new("region", "string").default(string("fr-par")).block(),
        Variable::new("type", "string").default(string(size)).block(),
    ]);
    m.blocks(host::variables(env, true));
    m.blocks(cloud::id_and_locals(env, Vec::new()));
    m.block(
        resource("linode_instance")
            .add_attribute(("region", reference("var.region")))
            .add_attribute(("type", reference("var.type")))
            .add_attribute(("image", string("linode/debian12")))
            .add_attribute(("authorized_keys", list([call("trimspace", [reference("var.ssh_public_key")])])))
            .add_attribute(("tags", cloud::tag_list(env)))
            .build(),
    );
    m.block(firewall(env));
    let reach = Reach {
        host: call("one", [reference("linode_instance.env.ipv4")]),
        user: "root",
        replace_on: reference("linode_instance.env.id"),
        auto_stop: true,
    };
    host::environment(env, &reach, &mut m);
    m
}

fn resource(kind: &str) -> isoloom_render::hcl_builder::BlockBuilder {
    Block::builder("resource")
        .add_label(kind)
        .add_label("env")
        .add_attribute(("label", reference("local.name")))
}

/// A rule letting `allowed_cidr` in on `ports` (`"22,8080"`).
fn inbound(label: &str, protocol: &str, ports: &[u16]) -> Block {
    let ports: Vec<String> = ports.iter().map(u16::to_string).collect();
    Block::builder("inbound")
        .add_attribute(("label", string(label)))
        .add_attribute(("action", string("ACCEPT")))
        .add_attribute(("protocol", string(protocol)))
        .add_attribute(("ports", string(ports.join(","))))
        .add_attribute(("ipv4", list([reference("var.allowed_cidr")])))
        .build()
}

/// SSH and the published ports in from `allowed_cidr`, the rest dropped; everything out.
fn firewall(env: &Environment) -> Block {
    let mut tcp = vec![22];
    tcp.extend(published(env, Protocol::Tcp));
    let udp = published(env, Protocol::Udp);
    let mut b = resource("linode_firewall")
        .add_attribute(("inbound_policy", string("DROP")))
        .add_attribute(("outbound_policy", string("ACCEPT")))
        .add_block(inbound("ssh-and-published", "TCP", &tcp));
    if !udp.is_empty() {
        b = b.add_block(inbound("published-udp", "UDP", &udp));
    }
    b.add_attribute(("linodes", list([reference("linode_instance.env.id")]))).build()
}
