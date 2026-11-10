//! Linode's `cloud-vm` module, `.isoloom/cloud-vm/linode/main.tf`, rendered from the plan
//! through the VM core. One Linode per machine on its network's VPC subnet, each with its
//! private IPv4 pinned (`ipv4.vpc`), so the machines reach each other at exactly the spec's
//! addresses, and a firewall per machine enforcing `reach` both ways and opening only its own
//! published ports (`network`). Linux labs, a machine on one network (a Linode has one VPC
//! interface): the module refuses the rest, Windows and a controller.

mod network;

use std::net::Ipv4Addr;

use crate::hcl_builder::{Block, Expression, Module, Template, boolean, call, list, reference, string};
use isoloom_plan::{Environment, MachinePlan};

use crate::cloud::Cloud;
use crate::cloud::instance::{name_with, tag_list};
use crate::cloud::user_data_limit::Limit;
use crate::cloud::vm::rules::{FIREWALLS_TC, Images, Mtu, Need, Networks, OsImages, PLAYBOOK_CHECKS, Rules, Size, Sizes, Volumes};
use crate::cloud::vm::{self, Driver, disks, gpus::GpuSizes, hooks, outputs, own_image};
use crate::limits::Limits;
use crate::machine::user_data::Capped;
use crate::module::{self, ident, providers, resource};

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
    need: Need::of,
    sizes: Some(PLANS),
    nesting_sizes: None,
    mtu: Mtu::Fixed,
    picks_ipv6: false,
    volumes: Volumes { smallest_gb: 10, most: 6 },
    tpm: false,
    subnets: None,
    kept: None,
    images: Images {
        table: &IMAGES,
        own: own_image::linode,
    },
};

/// The SSH user of every Linode: Linode's images log in as root.
pub(super) const USER: &str = "root";

/// Debian's Linode images have no `sudo`: installed before the steps (they run with it).
const SUDO: &str = "command -v sudo >/dev/null || (apt-get update -qq && apt-get install -y -qq sudo)";

/// Why Linode can't run this environment, if it can't.
fn refusal(env: &Environment) -> Option<String> {
    RULES.linux_refusal(env)
}

/// The Linode image of each OS.
const IMAGES: OsImages<&str> = OsImages(&[
    ("debian-12", "linode/debian12"),
    ("debian-13", "linode/debian13"),
    ("ubuntu-22.04", "linode/ubuntu22.04"),
    ("ubuntu-24.04", "linode/ubuntu24.04"),
]);

/// An OS's image, when there is one.
pub(super) fn image(os: &str) -> Option<&'static str> {
    IMAGES.of(os)
}

/// The plans, smallest first, each with its vCPUs, memory and disk (an instance has one VPC
/// interface).
pub(super) const PLANS: Sizes = Sizes::ordered(&[
    Size::new("g6-nanode-1", 1, 1024, 1).with_disk(25),
    Size::new("g6-standard-1", 1, 2048, 1).with_disk(50),
    Size::new("g6-standard-2", 2, 4096, 1).with_disk(80),
    Size::new("g6-standard-4", 4, 8192, 1).with_disk(160),
    Size::new("g6-standard-6", 6, 16384, 1).with_disk(320),
]);

/// How Linode takes an instance's user data: its metadata, base64, 65535 bytes of it decoded.
const CAP: Capped = Capped {
    limit: Limit(Cloud::Linode),
    gzipped: false,
};

/// Its `cloud-vm` driver: its module authenticates with a token in the environment.
pub(crate) const DRIVER: Driver = Driver {
    base: super::BASE,
    requires: (&[providers::LINODE, providers::RANDOM], &[]),
    refusal,
    write,
};

/// The module after its providers: the VPC and its subnets, the firewalls, each machine, the outputs.
fn write(env: &Environment, m: &mut Module) -> crate::Result<()> {
    network::blocks(env, m);
    for (vm, at) in vm::at_first_address(env) {
        machine(env, vm, at, m)?;
    }
    outputs::write_as(env, super::BASE.user, &public_ip, m);
    Ok(())
}

/// A Linode's public address.
fn public_ip(name: &str) -> Expression {
    call("one", [reference(&format!("linode_instance.{}.ipv4", ident(name)))])
}

/// Machine `name`'s root password (for the console: SSH logs in with the key), its instance (a
/// public interface, and a VPC interface pinned to its address), set up from its user data.
///
/// # Errors
///
/// [`crate::Error::Refused`] when the Linode's user data is over Linode's cap ([`CAP`]).
fn machine(env: &Environment, (name, mp): (&str, &MachinePlan), (net, address): (&str, Ipv4Addr), m: &mut Module) -> crate::Result<()> {
    let id = ident(name);
    m.block(module::password(&id));
    let at_boot = std::iter::once(Template::text(SUDO)).chain(network::routes(env, net)).collect();
    let setup = hooks::user_data(env, (name, mp), USER, at_boot, CAP)?;
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
            .add_attribute(("type", string(RULES.size_of(mp))))
            .add_attribute((
                "image",
                string(own_image::of(mp, own_image::linode).unwrap_or_else(|| image(os).unwrap_or_default())),
            ))
            .add_attribute(("authorized_keys", list([call("trimspace", [reference("var.ssh_public_key")])])))
            .add_attribute(("root_pass", reference(&format!("random_password.{id}.result"))))
            .add_attribute(("booted", boolean(true)))
            .add_attribute(("tags", tag_list(env)))
            .add_block(super::metadata(setup))
            .add_block(Block::builder("interface").add_attribute(("purpose", string("public"))).build())
            .add_block(vpc.build())
            .add_block(super::set_up_once())
            .build(),
    );
    m.blocks(disks::linode(env, name, &mp.disks));
    Ok(())
}
