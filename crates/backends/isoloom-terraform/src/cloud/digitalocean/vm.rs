//! DigitalOcean's `cloud-vm` module, `.isoloom/cloud-vm/digitalocean/main.tf`, rendered from the
//! plan through the VM core. One droplet for the environment's single Linux machine, on a VPC with the
//! network's range. DigitalOcean can't pin a droplet's private IPv4 to a chosen address, so the
//! environment's fixed inter-machine addressing wouldn't hold: the module takes only single-network,
//! single-Linux-VM environments and refuses the rest.

use crate::hcl_builder::{Block, Expression, Module, list, reference, string};
use isoloom_plan::{Environment, MachinePlan, Vm};

use super::{Way, rule, ssh_key, vpc};
use crate::cloud::Cloud;
use crate::cloud::instance::{self, name_with, tag_list};
use crate::cloud::user_data_limit::Limit;
use crate::cloud::vm::rules::{FIREWALLS_TC, Images, Mtu, Need, Networks, OsImages, PLAYBOOK_CHECKS, Rules, Size, Sizes, Volumes};
use crate::cloud::vm::{Driver, disks, gpus::GpuSizes, hooks, outputs, own_image, published};
use crate::limits::Limits;
use crate::machine::user_data::Capped;
use crate::module::{ident, providers, resource};

/// What DigitalOcean takes.
const RULES: Rules = Rules {
    cloud: Cloud::DigitalOcean,
    networks: Networks::OneVm,
    limits: Limits {
        arm64: Some("DigitalOcean has no arm64 droplets"),
        gateways: Some("a DigitalOcean VPC has no route tables, so no traffic can be sent through a machine"),
        playbook_checks: Some(PLAYBOOK_CHECKS),
        tc: Some(FIREWALLS_TC),
        ipv6: Some("DigitalOcean VPCs are IPv4 only (a droplet's IPv6 address is a public one)"),
    },
    // NVIDIA H100s, in the regions that have GPU droplets (tor1, nyc2, atl1).
    gpus: GpuSizes {
        gpu: "NVIDIA H100",
        sizes: &[(1, "gpu-h100x1-80gb"), (8, "gpu-h100x8-640gb")],
        none: "",
    },
    nests: false,
    need: Need::of,
    sizes: Some(DROPLETS),
    nesting_sizes: None,
    mtu: Mtu::Fixed,
    picks_ipv6: false,
    volumes: Volumes { smallest_gb: 1, most: 7 },
    tpm: false,
    subnets: None,
    kept: None,
    images: Images {
        table: &IMAGES,
        own: own_image::digitalocean,
    },
};

/// The SSH user of every droplet: DigitalOcean's images log in as root.
pub(super) const USER: &str = "root";

/// Why DigitalOcean can't run this environment, if it can't: its rules, then a machine with
/// several addresses on its network.
fn refusal(env: &Environment) -> Option<String> {
    RULES.linux_refusal(env).or_else(|| several_addresses(env))
}

/// A machine with several addresses on one network (`lan: [10, 11]`): a Droplet has one address
/// on its VPC, which DigitalOcean assigns, and no secondary private address to add (a reserved
/// IP is a public address), so the VPC would never deliver the others.
fn several_addresses(env: &Environment) -> Option<String> {
    let (name, mp) = env.machines.iter().find(|(_, mp)| !mp.more_addresses.is_empty())?;
    let net = mp.more_addresses.keys().next()?;
    Some(format!(
        "machine `{name}` has several addresses on `{net}`: a Droplet has one address on its VPC (DigitalOcean has no secondary private address; a reserved IP is public)"
    ))
}

/// The droplet image slug of each OS.
const IMAGES: OsImages<&str> = OsImages(&[
    ("debian-12", "debian-12-x64"),
    ("debian-13", "debian-13-x64"),
    ("ubuntu-22.04", "ubuntu-22-04-x64"),
    ("ubuntu-24.04", "ubuntu-24-04-x64"),
]);

/// An OS's image, when there is one.
pub(super) fn image(os: &str) -> Option<&'static str> {
    IMAGES.of(os)
}

/// The droplet sizes, smallest first, each with its vCPUs, memory and disk (a droplet joins one
/// VPC: one interface).
pub(super) const DROPLETS: Sizes = Sizes::ordered(&[
    Size::new("s-1vcpu-1gb", 1, 1024, 1).with_disk(25),
    Size::new("s-2vcpu-2gb", 2, 2048, 1).with_disk(60),
    Size::new("s-2vcpu-4gb", 2, 4096, 1).with_disk(80),
    Size::new("s-4vcpu-8gb", 4, 8192, 1).with_disk(160),
    Size::new("s-8vcpu-16gb", 8, 16384, 1).with_disk(320),
]);

/// How DigitalOcean takes a droplet's user data: plain text, 64 KiB of it.
const CAP: Capped = Capped {
    limit: Limit(Cloud::DigitalOcean),
    gzipped: false,
};

/// Its `cloud-vm` driver: its module authenticates with a token in the environment.
pub(crate) const DRIVER: Driver = Driver {
    base: super::BASE,
    requires: (&[providers::DIGITALOCEAN], &[]),
    refusal,
    write,
};

/// The module after its providers: the VPC, the droplet, its firewall and set-up, the outputs.
fn write(env: &Environment, m: &mut Module) -> crate::Result<()> {
    network(env, m);
    for vm in env.vms() {
        droplet(env, vm, m)?;
    }
    outputs::write_as(env, super::BASE.user, &public_ip, m);
    Ok(())
}

/// A droplet's public address.
fn public_ip(name: &str) -> Expression {
    reference(&format!("digitalocean_droplet.{}.ipv4_address", ident(name)))
}

/// The VPC with the network's range, and the SSH key.
fn network(env: &Environment, m: &mut Module) {
    let cidr = env.networks.values().next().map(|n| n.cidr.to_string()).unwrap_or_default();
    m.commented(
        "The environment's network: a VPC with its network's range. DigitalOcean assigns the\ndroplet a private address in it (it can't be pinned), so this is best-effort addressing.",
        vpc(&cidr),
    );
    m.block(ssh_key());
}

/// Machine `name`'s droplet, set up from its user data, and its firewall.
///
/// # Errors
///
/// [`crate::Error::Refused`] when its user data is over [`CAP`].
fn droplet(env: &Environment, Vm { name, plan: mp, os }: Vm<'_>, m: &mut Module) -> crate::Result<()> {
    let id = ident(name);
    let slug = own_image::of(mp, own_image::digitalocean).unwrap_or_else(|| image(os).unwrap_or_default());
    m.blocks(disks::digitalocean(env, name, &mp.disks));
    let mut droplet = resource("digitalocean_droplet", &id)
        .add_attribute(("name", name_with(&format!("-{name}"))))
        .add_attribute(("region", reference("var.region")))
        .add_attribute(("size", string(RULES.size_of(mp))))
        .add_attribute(("image", string(slug)))
        .add_attribute(("vpc_uuid", reference("digitalocean_vpc.env.id")))
        .add_attribute(("ssh_keys", list([reference("digitalocean_ssh_key.env.id")])))
        .add_attribute(("user_data", hooks::user_data(env, (name, mp), USER, Vec::new(), CAP)?))
        .add_attribute(("tags", tag_list(env)))
        .add_block(instance::set_up_once(reference("user_data")).build());
    if !mp.disks.is_empty() {
        droplet = droplet.add_attribute(("volume_ids", disks::digitalocean_ids(name, &mp.disks)));
    }
    m.block(droplet.build());
    m.commented(
        "What may reach it: SSH and the published ports from allowed_cidr, everything from the VPC.",
        firewall(mp, &id, crate::seal::sealed(env, mp)),
    );
    Ok(())
}

/// SSH and the droplet's own published ports from `allowed_cidr`, everything from the VPC,
/// everything out; the VPC only once the run is sealed, for a `sealed` droplet (no internet).
fn firewall(mp: &MachinePlan, id: &str, sealed: bool) -> Block {
    let allowed = || list([reference("var.allowed_cidr")]);
    let vpc = || list([reference("digitalocean_vpc.env.ip_range")]);
    let everywhere = || list([string("0.0.0.0/0"), string("::/0")]);
    let anywhere = || {
        if sealed {
            crate::cloud::vm::seal::when_sealed(vpc(), everywhere())
        } else {
            everywhere()
        }
    };
    let mut b = resource("digitalocean_firewall", id)
        .add_attribute(("name", reference("local.name")))
        .add_attribute(("droplet_ids", list([reference(&format!("digitalocean_droplet.{id}.id"))])))
        .add_block(rule(Way::In, "tcp", Some("22"), allowed()));
    for (protocol, port) in published::ports(mp) {
        b = b.add_block(rule(Way::In, protocol.id(), Some(&port.to_string()), allowed()));
    }
    let every = [("tcp", Some("1-65535")), ("udp", Some("1-65535")), ("icmp", None)];
    for (protocol, ports) in every {
        b = b.add_block(rule(Way::In, protocol, ports, vpc()));
    }
    for (protocol, ports) in every {
        b = b.add_block(rule(Way::Out, protocol, ports, anywhere()));
    }
    b.build()
}
