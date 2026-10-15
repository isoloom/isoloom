//! Linode (Akamai) `cloud-vm`: `.isoloom/cloud-vm/linode/main.tf`, rendered from the plan
//! through the VM core. One Linode per machine on its network's VPC subnet, each with its
//! private IPv4 pinned (`ipv4.vpc`), so the machines reach each other at exactly the spec's
//! addresses, and a firewall per machine enforcing `reach` both ways and opening only its own
//! published ports (`network`). Linux labs, a
//! machine on one network (a Linode has one VPC interface): the backend refuses the rest,
//! Windows and a controller.

use isoloom_plan::{Capabilities, Environment};
use isoloom_render::backend::{Backend, Files, RenderError};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, Template, boolean, call, list, number, reference, string};
use isoloom_render::{GeneratedFile, header};
use isoloom_spec::DEFAULT_MEMORY_MB;
use isoloom_spec::model::Target;

use super::shared::{self, Networks, Rules};
use crate::vm::cloud::{self, Reach, auto_stop, name_with, shutdown_script, tag_list};
use crate::vm::gpus::GpuSizes;
use crate::vm::linux;
use crate::vm::module::{Variable, project_lists, terraform};
use crate::vm::versions;
use crate::vm::{os, own_image, res, vms};

mod network;

/// The module's folder under `.isoloom/cloud-vm/`.
const SUB: &str = "linode";

/// What Linode takes.
const RULES: Rules = Rules {
    cloud: "Linode",
    networks: Networks::OnePerMachine,
    limits: shared::limits(
        "arm64 on Linode comes later (it has no arm64 plans)",
        Some("a Linode VPC's IPv6 ranges are Linode's to assign, so the spec's block can't be kept"),
        Some("a Linode VPC has no route tables, so no traffic can be sent through a machine"),
    ),
    has_image: |os| image(os).is_some(),
    disks: (10, 6),
    images: "Debian 12/13, Ubuntu 22.04/24.04",
    tpm: false,
    own: own_image::linode,
    // NVIDIA Quadro RTX 6000s, in the regions that have GPU plans.
    gpus: GpuSizes {
        cloud: "Linode",
        gpu: "NVIDIA RTX 6000",
        sizes: &[
            (1, "g1-gpu-rtx6000-1"),
            (2, "g1-gpu-rtx6000-2"),
            (3, "g1-gpu-rtx6000-3"),
            (4, "g1-gpu-rtx6000-4"),
        ],
        none: "",
    },
    nests: false,
};

/// The `cloud-vm` module on Linode.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinodeBackend;

impl Backend for LinodeBackend {
    fn target(&self) -> Target {
        Target::CloudVm
    }

    fn capabilities(&self) -> Capabilities {
        shared::capabilities()
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        shared::render(env, &RULES, module)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        isoloom_plan::refusal(&env.requirements, &self.capabilities())
            .map(|r| r.to_string())
            .or_else(|| shared::refusal(env, &RULES))
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
    network::blocks(env, &mut m);
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
        path: shared::path(SUB),
        contents: m.render()?,
    })
}

/// A Linode's public address.
fn public_ip(name: &str) -> Expression {
    call("one", [reference(&format!("linode_instance.{}.ipv4", res(name)))])
}

/// A resource block.
fn resource(kind: &str, name: &str) -> BlockBuilder {
    Block::builder("resource").add_label(kind).add_label(name)
}

/// A machine's root password (for the console: SSH logs in with the key), its instance (a
/// public interface, and a VPC interface pinned to its address), its set-up over SSH.
fn instance(env: &Environment, name: &str, m: &mut Module) {
    let mp = &env.machines[name];
    let id = res(name);
    let Some((net, address)) = mp.addresses.first().map(|(n, a)| (n.clone(), a.to_string())) else {
        return;
    };
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
    let mut vpc = Block::builder("interface")
        .add_attribute(("purpose", string("vpc")))
        .add_attribute(("subnet_id", reference(&format!("linode_vpc_subnet.{}.id", res(&net)))))
        .add_block(Block::builder("ipv4").add_attribute(("vpc", string(address))).build());
    // Its further addresses (`lan: [10, 11]`): ranges of the subnet the VPC routes to this
    // interface, one address each; its set-up puts them on the interface.
    if let Some(more) = mp.more_addresses.get(&net) {
        vpc = vpc.add_attribute(("ip_ranges", list(more.iter().map(|a| string(format!("{a}/32"))))));
    }
    let vpc = vpc.build();
    m.block(
        resource("linode_instance", &id)
            .add_attribute(("label", name_with(&format!("-{name}"))))
            .add_attribute(("region", reference("var.region")))
            .add_attribute((
                "type",
                string(RULES.gpus.of(mp).unwrap_or(plan(mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB)))),
            ))
            .add_attribute((
                "image",
                string(own_image::of(mp, own_image::linode).unwrap_or_else(|| image(os(mp).unwrap_or_default()).unwrap_or_default())),
            ))
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
        hooks: shared::hooks(mp, std::iter::once(Template::text(SUDO)).chain(network::routes(env, &net)).collect()),
    };
    m.blocks(crate::vm::disks::linode(name, &mp.disks));
    let mut setup = linux::provisioner(machine);
    // The firewall first: the set-up's downloads go through it.
    setup.depends_on.insert(0, reference(&format!("linode_firewall.{id}")));
    m.block(setup.block());
}
