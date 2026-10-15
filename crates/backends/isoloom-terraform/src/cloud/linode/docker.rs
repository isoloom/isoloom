//! Linode's `cloud-docker` module: a Debian instance with the operator's key, behind a
//! firewall.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Module, call, list, reference, string};
use isoloom_spec::model::Protocol;

use super::{Action, Way, region, rule};
use crate::cloud::docker::{Driver, SizeVariable};
use crate::cloud::instance::tag_list;
use crate::docker_host::{Access, AutoStop, published, tcp_ports};
use crate::module::providers;

/// Its `cloud-docker` driver.
pub(crate) const DRIVER: Driver = Driver {
    requires: providers::LINODE,
    variables: || vec![region()],
    size: Some(SizeVariable {
        name: "type",
        default: |s| s.pick(["g6-standard-2", "g6-standard-4", "g6-standard-6"]),
    }),
    // No provider block: the provider reads its token from the environment.
    provider: |_, _| Vec::new(),
    resources,
};

/// The cloud's resources, after the module's head; the host they make, as its set-up reaches
/// it.
fn resources(env: &Environment, m: &mut Module) -> Access {
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
    Access {
        host: call("one", [reference("linode_instance.env.ipv4")]),
        user: "root",
        replace_on: reference("linode_instance.env.id"),
        auto_stop: AutoStop::SetUp,
    }
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
