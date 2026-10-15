//! Linode's `cloud-vm` module, `.isoloom/cloud-vm/linode/main.tf`, rendered from the plan
//! through the VM core. One Linode per machine on its network's VPC subnet, each with its
//! private IPv4 pinned (`ipv4.vpc`), so the machines reach each other at exactly the spec's
//! addresses, and a firewall per machine enforcing `reach` both ways and opening only its own
//! published ports (`network`). Linux labs, a machine on one network (a Linode has one VPC
//! interface): the module refuses the rest, Windows and a controller.

mod network;

use std::net::Ipv4Addr;

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{Block, Expression, Module, Template, boolean, call, list, reference, string};
use isoloom_spec::DEFAULT_MEMORY_MB;

use super::region;
use crate::cloud::instance::{name_with, tag_list};
use crate::cloud::vm::rules::{FIREWALLS_TC, Images, Mtu, Networks, PLAYBOOK_CHECKS, Rules, Volumes};
use crate::cloud::vm::{self, Driver, disks, gpus::GpuSizes, hooks, outputs, own_image};
use crate::cloud::{Cloud, auto_stop, operator};
use crate::limits::Limits;
use crate::machine::{Remote, linux};
use crate::module::{self, Usage, ident, providers, resource};

/// What Linode takes.
const RULES: Rules = Rules {
    cloud: Cloud::Linode,
    networks: Networks::OnePerMachine,
    limits: Limits {
        arm64: Some("arm64 on Linode comes later (it has no arm64 plans)"),
        gateways: Some("a Linode VPC has no route tables, so no traffic can be sent through a machine"),
        playbook_checks: Some(PLAYBOOK_CHECKS),
        tc: Some(FIREWALLS_TC),
        ipv6: Some("a Linode VPC's IPv6 ranges are Linode's to assign, so the spec's block can't be kept"),
    },
    // NVIDIA Quadro RTX 6000s, in the regions that have GPU plans.
    gpus: GpuSizes {
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
    mtu: Mtu::Fixed,
    picks_ipv6: false,
    volumes: Volumes { smallest_gb: 10, most: 6 },
    tpm: false,
    subnets: None,
    kept: None,
    images: Images {
        label: "Linode",
        has: |os| image(os).is_some(),
        list: "Debian 12/13, Ubuntu 22.04/24.04",
        own: own_image::linode,
    },
};

/// The SSH user of every Linode: Linode's images log in as root.
const USER: &str = "root";

/// Debian's Linode images have no `sudo`: installed before the steps (they run with it).
const SUDO: &str = "command -v sudo >/dev/null || (apt-get update -qq && apt-get install -y -qq sudo)";

/// Why Linode can't run this environment, if it can't.
fn refusal(env: &Environment) -> Option<String> {
    RULES.linux_refusal(env)
}

/// The Linode image for an OS.
fn image(os: &str) -> Option<&'static str> {
    Some(match os {
        "debian-12" => "linode/debian12",
        "debian-13" => "linode/debian13",
        "ubuntu-22.04" => "linode/ubuntu22.04",
        "ubuntu-24.04" => "linode/ubuntu24.04",
        _ => return None,
    })
}

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

/// Its `cloud-vm` driver: its module authenticates with a token in the environment.
pub(crate) const DRIVER: Driver = Driver {
    usage: |folder| Usage {
        before: "# Auth:   export LINODE_TOKEN=<token> (the provider reads it).\n",
        ..Usage::with_key(folder, operator::ALLOWED)
    },
    requires: (&[providers::LINODE, providers::RANDOM, providers::LOCAL], &[]),
    variables: || vec![region()],
    provider: |_, m| {
        m.block(Block::builder("provider").add_label("linode").build());
        Vec::new()
    },
    refusal,
    write,
};

/// The module after its providers: the VPC and its subnets, the firewalls, each machine, the outputs.
fn write(env: &Environment, m: &mut Module) {
    network::blocks(env, m);
    for (vm, at) in vm::at_first_address(env) {
        machine(env, vm, at, m);
    }
    outputs::write_as(env, USER, &public_ip, m);
}

/// A Linode's public address.
fn public_ip(name: &str) -> Expression {
    call("one", [reference(&format!("linode_instance.{}.ipv4", ident(name)))])
}

/// Machine `name`'s root password (for the console: SSH logs in with the key), its instance (a
/// public interface, and a VPC interface pinned to its address), its set-up over SSH.
fn machine(env: &Environment, (name, mp): (&str, &MachinePlan), (net, address): (&str, Ipv4Addr), m: &mut Module) {
    let id = ident(name);
    m.block(module::password(&id));
    let user_data = call("base64encode", [auto_stop::if_set(auto_stop::script(), string(""))]);
    let mut vpc = Block::builder("interface")
        .add_attribute(("purpose", string("vpc")))
        .add_attribute(("subnet_id", reference(&format!("linode_vpc_subnet.{}.id", ident(net)))))
        .add_block(Block::builder("ipv4").add_attribute(("vpc", string(address.to_string()))).build());
    // Its further addresses (`lan: [10, 11]`): ranges of the subnet the VPC routes to this
    // interface, one address each; its set-up puts them on the interface.
    if let Some(more) = mp.more_addresses.get(net) {
        vpc = vpc.add_attribute(("ip_ranges", list(more.iter().map(|a| string(format!("{a}/32"))))));
    }
    let os = mp.os.as_deref().unwrap_or_default();
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
                string(own_image::of(mp, own_image::linode).unwrap_or_else(|| image(os).unwrap_or_default())),
            ))
            .add_attribute(("authorized_keys", list([call("trimspace", [reference("var.ssh_public_key")])])))
            .add_attribute(("root_pass", reference(&format!("random_password.{id}.result"))))
            .add_attribute(("booted", boolean(true)))
            .add_attribute(("tags", tag_list(env)))
            .add_block(Block::builder("metadata").add_attribute(("user_data", user_data)).build())
            .add_block(Block::builder("interface").add_attribute(("purpose", string("public"))).build())
            .add_block(vpc.build())
            .build(),
    );
    let at_boot = std::iter::once(Template::text(SUDO)).chain(network::routes(env, net)).collect();
    let machine = linux::Machine {
        remote: Remote {
            env,
            name,
            plan: mp,
            replace_on: reference(&format!("linode_instance.{id}.id")),
            host: public_ip(name),
        },
        user: USER.to_owned(),
        hooks: hooks::of(mp, at_boot),
    };
    m.blocks(disks::linode(name, &mp.disks));
    let mut setup = linux::provisioner(machine);
    // The firewall first: the set-up's downloads go through it.
    setup.depends_on.insert(0, reference(&format!("linode_firewall.{id}")));
    m.block(setup.block());
}
