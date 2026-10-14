//! Linode (Akamai) `cloud-vm`: `.isoloom/cloud-vm/linode/main.tf`, rendered from the plan
//! through the VM core. One Linode per machine on a single VPC subnet, each with its private
//! IPv4 pinned (`ipv4.vpc`), so the machines reach each other at exactly the spec's addresses.
//! Single-network Linux labs only: the backend refuses several networks, Windows and a
//! controller.

use isoloom_plan::{Capabilities, Environment};
use isoloom_render::backend::{Backend, Files, RenderError};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, Template, boolean, call, list, number, reference, string};
use isoloom_render::{GeneratedFile, header};
use isoloom_spec::DEFAULT_MEMORY_MB;
use isoloom_spec::model::Target;

use super::single_network::{self, Rules};
use crate::vm::cloud::{self, Reach, auto_stop, name_with, shutdown_script, tag_list};
use crate::vm::linux;
use crate::vm::module::{Variable, project_lists, terraform};
use crate::vm::versions;
use crate::vm::{os, res, vms};

/// The module's folder under `.isoloom/cloud-vm/`.
const SUB: &str = "linode";

/// What Linode takes.
const RULES: Rules = Rules {
    cloud: "Linode",
    single_machine: false,
    limits: single_network::limits(
        "arm64 on Linode comes later (it has no arm64 plans)",
        "a Linode VPC's IPv6 ranges are Linode's to assign, so the spec's block can't be kept",
    ),
    has_image: |os| image(os).is_some(),
    images: "Debian 12/13, Ubuntu 22.04/24.04",
};

/// The `cloud-vm` module on Linode.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinodeBackend;

impl Backend for LinodeBackend {
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

/// The Linode image for an OS (Linode's images log in as root).
fn image(os: &str) -> Option<&'static str> {
    Some(match os {
        "debian-12" => "linode/debian12",
        "debian-13" => "linode/debian13",
        "ubuntu-22.04" => "linode/ubuntu22.04",
        "ubuntu-24.04" => "linode/ubuntu24.04",
        _ => return None,
    })
}

/// The SSH user of every Linode.
const USER: &str = "root";

/// The Linode plan for a machine's memory.
fn plan(memory_mb: u32) -> &'static str {
    match memory_mb {
        0..=1024 => "g6-nanode-1",
        1025..=2048 => "g6-standard-1",
        2049..=4096 => "g6-standard-2",
        4097..=8192 => "g6-standard-4",
        _ => "g6-standard-6",
    }
}

const USAGE: &str = "# Auth:   export LINODE_TOKEN=<token> (the provider reads it).\n# Start:  terraform -chdir=.isoloom/cloud-vm/linode init && terraform -chdir=.isoloom/cloud-vm/linode apply \\\n#           -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-vm/linode destroy (same variables)\n";

/// Debian's Linode images have no `sudo`: installed before the steps (they run with it).
const SUDO: &str = "command -v sudo >/dev/null || (apt-get update -qq && apt-get install -y -qq sudo)";

/// The module: the VPC and its subnet, the firewall, each machine, the outputs.
fn module(env: &Environment) -> Result<GeneratedFile, isoloom_render::hcl_builder::Error> {
    let mut m = Module::new(format!("{}{USAGE}", header("#")));
    m.block(terraform(&[versions::LINODE, versions::RANDOM, versions::LOCAL]));
    m.block(Variable::new("region", "string").default(string("fr-par")).block());
    m.blocks(cloud::variables(env));
    m.block(Block::builder("provider").add_label("linode").build());
    m.blocks(cloud::id_and_locals(env, Vec::new()));
    network(env, &mut m);
    m.commented(
        "Who may reach the machines: SSH and the published ports from allowed_cidr, everything from\nthe subnet, the rest dropped. One firewall for every instance.",
        firewall(env),
    );
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

/// A Linode's public address.
fn public_ip(name: &str) -> Expression {
    call("one", [reference(&format!("linode_instance.{}.ipv4", res(name)))])
}

fn resource(kind: &str, name: &str) -> BlockBuilder {
    Block::builder("resource").add_label(kind).add_label(name)
}

/// The network's CIDR.
fn cidr(env: &Environment) -> &str {
    env.networks.values().next().map(|n| n.cidr.as_str()).unwrap_or_default()
}

/// The VPC, and a subnet at the network's exact range.
fn network(env: &Environment, m: &mut Module) {
    let (net, cidr) = env.networks.iter().next().map(|(n, p)| (n.as_str(), p.cidr.as_str())).unwrap_or_default();
    m.commented(
        &format!("The environment's one network: a VPC with a subnet at its exact range, so every machine keeps\nits address ({cidr})."),
        resource("linode_vpc", "env")
            .add_attribute(("label", reference("local.name")))
            .add_attribute(("region", reference("var.region")))
            .build(),
    );
    m.block(
        resource("linode_vpc_subnet", "env")
            .add_attribute(("vpc_id", reference("linode_vpc.env.id")))
            .add_attribute(("label", string(net)))
            .add_attribute(("ipv4", string(cidr)))
            .build(),
    );
}

/// An inbound rule.
fn inbound(label: &str, protocol: &str, ports: Option<String>, from: Expression) -> Block {
    let mut b = Block::builder("inbound")
        .add_attribute(("label", string(label)))
        .add_attribute(("action", string("ACCEPT")))
        .add_attribute(("protocol", string(protocol)));
    if let Some(p) = ports {
        b = b.add_attribute(("ports", string(p)));
    }
    b.add_attribute(("ipv4", from)).build()
}

/// The firewall of every instance.
fn firewall(env: &Environment) -> Block {
    let allowed = || list([reference("var.allowed_cidr")]);
    let subnet = || list([string(cidr(env))]);
    let mut b = resource("linode_firewall", "env")
        .add_attribute(("label", reference("local.name")))
        .add_attribute(("inbound_policy", string("DROP")))
        .add_attribute(("outbound_policy", string("ACCEPT")))
        .add_block(inbound("ssh", "TCP", Some("22".into()), allowed()));
    let published = single_network::published_ports(env);
    for (udp, label, protocol) in [(false, "published", "TCP"), (true, "published-udp", "UDP")] {
        let ports: Vec<String> = published.iter().filter(|(p, _)| p.is_tcp() != udp).map(|(_, n)| n.to_string()).collect();
        if !ports.is_empty() {
            b = b.add_block(inbound(label, protocol, Some(ports.join(",")), allowed()));
        }
    }
    b.add_block(inbound("vpc-tcp", "TCP", Some("1-65535".into()), subnet()))
        .add_block(inbound("vpc-udp", "UDP", Some("1-65535".into()), subnet()))
        .add_block(inbound("vpc-icmp", "ICMP", None, subnet()))
        .add_attribute(("linodes", list(vms(env).map(|(n, _)| reference(&format!("linode_instance.{}.id", res(n)))))))
        .build()
}

/// A machine's root password (for the console: SSH logs in with the key), its instance (a
/// public interface, and a VPC interface pinned to its address), its set-up over SSH.
fn instance(env: &Environment, name: &str, m: &mut Module) {
    let mp = &env.machines[name];
    let id = res(name);
    let address = mp.addresses.values().next().map(ToString::to_string).unwrap_or_default();
    m.block(
        resource("random_password", &id)
            .add_attribute(("length", number(24)))
            .add_attribute(("special", boolean(false)))
            .add_attribute(("min_upper", number(2)))
            .add_attribute(("min_lower", number(2)))
            .add_attribute(("min_numeric", number(2)))
            .build(),
    );
    let user_data = call("base64encode", [auto_stop(shutdown_script(), string(""))]);
    let vpc = Block::builder("interface")
        .add_attribute(("purpose", string("vpc")))
        .add_attribute(("subnet_id", reference("linode_vpc_subnet.env.id")))
        .add_block(Block::builder("ipv4").add_attribute(("vpc", string(address))).build())
        .build();
    m.block(
        resource("linode_instance", &id)
            .add_attribute(("label", name_with(&format!("-{name}"))))
            .add_attribute(("region", reference("var.region")))
            .add_attribute(("type", string(plan(mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB)))))
            .add_attribute(("image", string(image(os(mp).unwrap_or_default()).unwrap_or_default())))
            .add_attribute(("authorized_keys", list([call("trimspace", [reference("var.ssh_public_key")])])))
            .add_attribute(("root_pass", reference(&format!("random_password.{id}.result"))))
            .add_attribute(("booted", boolean(true)))
            .add_attribute(("tags", tag_list(env)))
            .add_block(Block::builder("metadata").add_attribute(("user_data", user_data)).build())
            .add_block(Block::builder("interface").add_attribute(("purpose", string("public"))).build())
            .add_block(vpc)
            .build(),
    );
    let machine = linux::Machine {
        env,
        name,
        replace_on: reference(&format!("linode_instance.{id}.id")),
        host: public_ip(name),
        user: USER.into(),
        hooks: single_network::hooks(mp, vec![Template::text(SUDO)]),
    };
    let mut setup = linux::provisioner(machine);
    // The firewall first: the set-up's downloads go through it.
    setup.depends_on.insert(0, reference("linode_firewall.env"));
    m.block(setup.block());
}
