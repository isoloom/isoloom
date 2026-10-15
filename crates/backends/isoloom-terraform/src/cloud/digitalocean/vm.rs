//! DigitalOcean's `cloud-vm` module, `.isoloom/cloud-vm/digitalocean/main.tf`, rendered from the
//! plan through the VM core. One droplet for the lab's single Linux machine, on a VPC with the
//! network's range. DigitalOcean can't pin a droplet's private IPv4 to a chosen address, so the
//! lab's fixed inter-machine addressing wouldn't hold: the module takes only single-network,
//! single-Linux-VM labs and refuses the rest.

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::GeneratedFile;
use isoloom_render::hcl_builder::{Block, Expression, Module, list, null, reference, string};
use isoloom_spec::DEFAULT_MEMORY_MB;
use isoloom_spec::model::Target;

use super::{Way, region, rule, ssh_key, vpc};
use crate::Result;
use crate::cloud::instance::{self, name_with, tag_list};
use crate::cloud::vm::gpus::GpuSizes;
use crate::cloud::vm::outputs::{self, Reach};
use crate::cloud::vm::rules::{FIREWALLS_TC, Images, Mtu, Networks, PLAYBOOK_CHECKS, Rules, Volumes};
use crate::cloud::vm::{disks, hooks, own_image, published};
use crate::cloud::{Cloud, auto_stop, operator};
use crate::limits::Limits;
use crate::machine::{linux, vms};
use crate::module::{self, ProjectCopy, Usage, ident, providers, resource};

/// What DigitalOcean takes.
const RULES: Rules = Rules {
    cloud: Cloud::DigitalOcean,
    networks: Networks::OneVm,
    limits: Limits {
        arm64: Some("arm64 on DigitalOcean comes later (it has no arm64 droplets)"),
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
    mtu: Mtu::Fixed,
    picks_ipv6: false,
    volumes: Volumes { smallest_gb: 1, most: 7 },
    tpm: false,
    images: Images {
        label: "DigitalOcean",
        has: |os| image(os).is_some(),
        list: "Debian 12/13, Ubuntu 22.04/24.04",
        own: own_image::digitalocean,
    },
};

/// The SSH user of every droplet: DigitalOcean's images log in as root.
const USER: &str = "root";

/// Why DigitalOcean can't run this environment, if it can't: its rules, then a machine with
/// several addresses on its network.
pub(crate) fn refusal(env: &Environment) -> Option<String> {
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

/// The droplet image slug for an OS.
fn image(os: &str) -> Option<&'static str> {
    Some(match os {
        "debian-12" => "debian-12-x64",
        "debian-13" => "debian-13-x64",
        "ubuntu-22.04" => "ubuntu-22-04-x64",
        "ubuntu-24.04" => "ubuntu-24-04-x64",
        _ => return None,
    })
}

/// The droplet size for a machine's memory.
fn size(memory_mb: u32) -> &'static str {
    match memory_mb {
        0..=1024 => "s-1vcpu-1gb",
        1025..=2048 => "s-2vcpu-2gb",
        2049..=4096 => "s-2vcpu-4gb",
        _ => "s-4vcpu-8gb",
    }
}

/// The module: the VPC, the droplet, its firewall and set-up, the outputs.
pub(crate) fn module(env: &Environment) -> Result<GeneratedFile> {
    let folder = format!("{}/{}", Target::CloudVm.id(), Cloud::DigitalOcean.id());
    let usage = Usage {
        before: "# Auth:   export DIGITALOCEAN_TOKEN=<token>\n",
        ..Usage::with_key(&folder, operator::ALLOWED)
    };
    let mut m = module::start(&usage);
    m.block(providers::terraform(&[providers::DIGITALOCEAN, providers::LOCAL]));
    m.block(region());
    m.blocks(operator::variables(env));
    m.block(Block::builder("provider").add_label("digitalocean").build());
    m.blocks(instance::id_and_locals(env, Vec::new()));
    network(env, &mut m);
    for (name, mp) in vms(env) {
        droplet(env, name, mp, &mut m);
    }
    let reach = Reach {
        public_ip: &public_ip,
        user: &|_| USER.to_owned(),
        controller: None,
    };
    outputs::write(env, &reach, &mut m);
    module::file(Target::CloudVm, &folder, m, ProjectCopy::Archive)
}

/// A droplet's public address.
fn public_ip(name: &str) -> Expression {
    reference(&format!("digitalocean_droplet.{}.ipv4_address", ident(name)))
}

/// The VPC with the network's range, and the SSH key.
fn network(env: &Environment, m: &mut Module) {
    let cidr = env.networks.values().next().map(|n| n.cidr.as_str()).unwrap_or_default();
    m.commented(
        "The environment's network: a VPC with the lab network's range. DigitalOcean assigns the\ndroplet a private address in it (it can't be pinned), so this is best-effort addressing.",
        vpc(cidr),
    );
    m.block(ssh_key());
}

/// Machine `name`'s droplet, its firewall, its set-up over SSH.
fn droplet(env: &Environment, name: &str, mp: &MachinePlan, m: &mut Module) {
    let id = ident(name);
    let os = mp.os.as_deref().unwrap_or_default();
    let slug = own_image::of(mp, own_image::digitalocean).unwrap_or_else(|| image(os).unwrap_or_default());
    m.blocks(disks::digitalocean(name, &mp.disks));
    let mut droplet = resource("digitalocean_droplet", &id)
        .add_attribute(("name", name_with(&format!("-{name}"))))
        .add_attribute(("region", reference("var.region")))
        .add_attribute((
            "size",
            string(RULES.gpus.of(mp).unwrap_or(size(mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB)))),
        ))
        .add_attribute(("image", string(slug)))
        .add_attribute(("vpc_uuid", reference("digitalocean_vpc.env.id")))
        .add_attribute(("ssh_keys", list([reference("digitalocean_ssh_key.env.fingerprint")])))
        .add_attribute(("user_data", auto_stop::if_set(auto_stop::script(), null())))
        .add_attribute(("tags", tag_list(env)));
    if !mp.disks.is_empty() {
        droplet = droplet.add_attribute(("volume_ids", disks::digitalocean_ids(name, &mp.disks)));
    }
    m.block(droplet.build());
    m.commented(
        "What may reach it: SSH and the published ports from allowed_cidr, everything from the VPC.",
        firewall(mp, &id),
    );
    let machine = linux::Machine {
        env,
        name,
        replace_on: reference(&format!("digitalocean_droplet.{id}.id")),
        host: public_ip(name),
        user: USER.to_owned(),
        hooks: hooks::of(mp, Vec::new()),
    };
    m.block(linux::provisioner(machine).block());
}

/// SSH and the droplet's own published ports from `allowed_cidr`, everything from the VPC,
/// everything out.
fn firewall(mp: &MachinePlan, id: &str) -> Block {
    let allowed = || list([reference("var.allowed_cidr")]);
    let vpc = || list([reference("digitalocean_vpc.env.ip_range")]);
    let anywhere = || list([string("0.0.0.0/0"), string("::/0")]);
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
