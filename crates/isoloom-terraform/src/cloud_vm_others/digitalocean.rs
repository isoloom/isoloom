//! DigitalOcean `cloud-vm`: `.isoloom/cloud-vm/digitalocean/main.tf`, rendered from the plan
//! through the VM core. One droplet for the lab's single Linux machine, on a VPC with the
//! network's range. DigitalOcean can't pin a droplet's private IPv4 to a chosen address, so the
//! lab's fixed inter-machine addressing wouldn't hold: the backend takes only single-network,
//! single-Linux-VM labs and refuses the rest.

use isoloom_plan::{Capabilities, Environment};
use isoloom_render::backend::{Backend, Files, RenderError};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, list, reference, string};
use isoloom_render::{GeneratedFile, header};
use isoloom_spec::DEFAULT_MEMORY_MB;
use isoloom_spec::model::Target;

use super::single_network::{self, Rules};
use crate::vm::cloud::{self, Reach, auto_stop, shutdown_script, tag_list};
use crate::vm::linux;
use crate::vm::module::{Variable, project_lists, terraform};
use crate::vm::versions;
use crate::vm::{os, res, vms};

/// The module's folder under `.isoloom/cloud-vm/`.
const SUB: &str = "digitalocean";

/// What DigitalOcean takes.
const RULES: Rules = Rules {
    cloud: "DigitalOcean",
    single_machine: true,
    limits: single_network::limits(
        "arm64 on DigitalOcean comes later (it has no arm64 droplets)",
        "DigitalOcean VPCs are IPv4 only (a droplet's IPv6 address is a public one)",
    ),
    has_image: |os| image(os).is_some(),
    images: "Debian 12/13, Ubuntu 22.04/24.04",
};

/// The `cloud-vm` module on DigitalOcean.
#[derive(Debug, Clone, Copy, Default)]
pub struct DigitalOceanBackend;

impl Backend for DigitalOceanBackend {
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

/// The droplet image slug for an OS (DigitalOcean's images log in as root).
fn image(os: &str) -> Option<&'static str> {
    Some(match os {
        "debian-12" => "debian-12-x64",
        "debian-13" => "debian-13-x64",
        "ubuntu-22.04" => "ubuntu-22-04-x64",
        "ubuntu-24.04" => "ubuntu-24-04-x64",
        _ => return None,
    })
}

/// The SSH user of every droplet.
const USER: &str = "root";

/// The droplet size for a machine's memory.
fn size(memory_mb: u32) -> &'static str {
    match memory_mb {
        0..=1024 => "s-1vcpu-1gb",
        1025..=2048 => "s-2vcpu-2gb",
        2049..=4096 => "s-2vcpu-4gb",
        _ => "s-4vcpu-8gb",
    }
}

const USAGE: &str = "# Auth:   export DIGITALOCEAN_TOKEN=<token>\n# Start:  terraform -chdir=.isoloom/cloud-vm/digitalocean init && terraform -chdir=.isoloom/cloud-vm/digitalocean apply \\\n#           -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-vm/digitalocean destroy (same variables)\n";

/// The module: the VPC, the droplet, its firewall and set-up, the outputs.
fn module(env: &Environment) -> Result<GeneratedFile, isoloom_render::hcl_builder::Error> {
    let mut m = Module::new(format!("{}{USAGE}", header("#")));
    m.block(terraform(&[versions::DIGITALOCEAN, versions::LOCAL]));
    m.block(Variable::new("region", "string").default(string("fra1")).block());
    m.blocks(cloud::variables(env));
    m.block(Block::builder("provider").add_label("digitalocean").build());
    m.blocks(cloud::id_and_locals(env, Vec::new()));
    network(env, &mut m);
    for (name, _) in vms(env) {
        droplet(env, name, &mut m);
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

/// A droplet's public address.
fn public_ip(name: &str) -> Expression {
    reference(&format!("digitalocean_droplet.{}.ipv4_address", res(name)))
}

fn resource(kind: &str, name: &str) -> BlockBuilder {
    Block::builder("resource").add_label(kind).add_label(name)
}

/// The VPC with the network's range, and the SSH key.
fn network(env: &Environment, m: &mut Module) {
    let cidr = env.networks.values().next().map(|n| n.cidr.as_str()).unwrap_or_default();
    m.commented(
        "The environment's network: a VPC with the lab network's range. DigitalOcean assigns the\ndroplet a private address in it (it can't be pinned), so this is best-effort addressing.",
        resource("digitalocean_vpc", "env")
            .add_attribute(("name", reference("local.name")))
            .add_attribute(("region", reference("var.region")))
            .add_attribute(("ip_range", string(cidr)))
            .build(),
    );
    m.block(
        resource("digitalocean_ssh_key", "env")
            .add_attribute(("name", reference("local.name")))
            .add_attribute(("public_key", reference("var.ssh_public_key")))
            .build(),
    );
}

/// The droplet, its firewall, its set-up over SSH.
fn droplet(env: &Environment, name: &str, m: &mut Module) {
    let mp = &env.machines[name];
    let id = res(name);
    let slug = image(os(mp).unwrap_or_default()).unwrap_or_default();
    m.block(
        resource("digitalocean_droplet", &id)
            .add_attribute(("name", cloud::name_with(&format!("-{name}"))))
            .add_attribute(("region", reference("var.region")))
            .add_attribute(("size", string(size(mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB)))))
            .add_attribute(("image", string(slug)))
            .add_attribute(("vpc_uuid", reference("digitalocean_vpc.env.id")))
            .add_attribute(("ssh_keys", list([reference("digitalocean_ssh_key.env.fingerprint")])))
            .add_attribute(("user_data", auto_stop(shutdown_script(), isoloom_render::hcl_builder::null())))
            .add_attribute(("tags", tag_list(env)))
            .build(),
    );
    m.commented(
        "What may reach it: SSH and the published ports from allowed_cidr, everything from the VPC.",
        firewall(env, &id),
    );
    let machine = linux::Machine {
        env,
        name,
        replace_on: reference(&format!("digitalocean_droplet.{id}.id")),
        host: public_ip(name),
        user: USER.into(),
        hooks: single_network::hooks(mp, Vec::new()),
    };
    m.block(linux::provisioner(machine).block());
}

/// A firewall rule: inbound from `sources` or outbound to `destinations`.
fn rule(kind: &str, protocol: &str, ports: Option<&str>, addresses: Expression) -> Block {
    let (block, key) = if kind == "inbound" {
        ("inbound_rule", "source_addresses")
    } else {
        ("outbound_rule", "destination_addresses")
    };
    let mut b = Block::builder(block).add_attribute(("protocol", string(protocol)));
    if let Some(p) = ports {
        b = b.add_attribute(("port_range", string(p)));
    }
    b.add_attribute((key, addresses)).build()
}

/// SSH and the published ports from `allowed_cidr`, everything from the VPC, everything out.
fn firewall(env: &Environment, id: &str) -> Block {
    let allowed = || list([reference("var.allowed_cidr")]);
    let vpc = || list([reference("digitalocean_vpc.env.ip_range")]);
    let anywhere = || list([string("0.0.0.0/0"), string("::/0")]);
    let mut b = resource("digitalocean_firewall", id)
        .add_attribute(("name", reference("local.name")))
        .add_attribute(("droplet_ids", list([reference(&format!("digitalocean_droplet.{id}.id"))])))
        .add_block(rule("inbound", "tcp", Some("22"), allowed()));
    for (protocol, port) in single_network::published_ports(env) {
        b = b.add_block(rule("inbound", protocol.id(), Some(&port.to_string()), allowed()));
    }
    for (protocol, ports) in [("tcp", Some("1-65535")), ("udp", Some("1-65535")), ("icmp", None)] {
        b = b.add_block(rule("inbound", protocol, ports, vpc()));
    }
    for (protocol, ports) in [("tcp", Some("1-65535")), ("udp", Some("1-65535")), ("icmp", None)] {
        b = b.add_block(rule("outbound", protocol, ports, anywhere()));
    }
    b.build()
}
