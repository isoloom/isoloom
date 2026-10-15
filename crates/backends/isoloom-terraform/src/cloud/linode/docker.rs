//! Linode's `cloud-docker` module: a Debian instance with the operator's key, behind a
//! firewall.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Module, call, list, reference, string};
use isoloom_spec::model::Protocol;

use super::{Action, Way, region, rule};
use crate::cloud::instance::{self, tag_list};
use crate::cloud::operator;
use crate::docker_host::{self, Access, AutoStop, Sizing, published, tcp_ports};
use crate::module::providers::{self, terraform};
use crate::module::variable::Variable;

/// The module's resources and the host's set-up, from the plan.
pub(crate) fn write(env: &Environment, m: &mut Module) {
    m.block(terraform(&[providers::LINODE, providers::LOCAL]));
    let size = Sizing::of(env).pick(["g6-standard-2", "g6-standard-4", "g6-standard-6"]);
    m.block(region());
    m.block(Variable::new("type", "string").default(string(size)).block());
    m.blocks(operator::variables(env));
    m.blocks(instance::id_and_locals(env, Vec::new()));
    m.block(
        resource("linode_instance")
            .add_attribute(("region", reference("var.region")))
            .add_attribute(("type", reference("var.type")))
            .add_attribute(("image", string("linode/debian12")))
            .add_attribute(("authorized_keys", list([call("trimspace", [reference("var.ssh_public_key")])])))
            .add_attribute(("tags", tag_list(env)))
            .build(),
    );
    m.block(firewall(env));
    let access = Access {
        host: call("one", [reference("linode_instance.env.ipv4")]),
        user: "root",
        replace_on: reference("linode_instance.env.id"),
        auto_stop: AutoStop::SetUp,
    };
    docker_host::environment(env, &access, m);
}

/// `resource "<kind>" "env"`, labelled after the environment.
fn resource(kind: &str) -> BlockBuilder {
    crate::module::resource(kind, "env").add_attribute(("label", reference("local.name")))
}

/// A rule letting `allowed_cidr` in on `ports` (`"22,8080"`).
fn inbound(label: &str, protocol: &str, ports: &[u16]) -> Block {
    let ports: Vec<String> = ports.iter().map(u16::to_string).collect();
    rule(
        (Way::In, Action::Accept),
        label,
        protocol,
        Some(ports.join(",")),
        list([reference("var.allowed_cidr")]),
    )
}

/// SSH and the published ports in from `allowed_cidr`, the rest dropped; everything out.
fn firewall(env: &Environment) -> Block {
    let udp = published(env, Protocol::Udp);
    let mut b = resource("linode_firewall")
        .add_attribute(("inbound_policy", string("DROP")))
        .add_attribute(("outbound_policy", string("ACCEPT")))
        .add_block(inbound("ssh-and-published", "TCP", &tcp_ports(env)));
    if !udp.is_empty() {
        b = b.add_block(inbound("published-udp", "UDP", &udp));
    }
    b.add_attribute(("linodes", list([reference("linode_instance.env.id")]))).build()
}
