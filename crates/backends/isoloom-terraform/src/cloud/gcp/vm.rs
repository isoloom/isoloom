//! Google Cloud's `cloud-vm` module, `.isoloom/cloud-vm/gcp/main.tf`, rendered from the plan
//! through the VM core.
//!
//! - A project of its own (created and billed when none is given, deleted with the environment),
//!   a VPC network per network holding a subnetwork with its exact range, peered or routed
//!   through a machine (`network`); firewall rules matching each machine by its tag do what the
//!   AWS security groups do.
//! - Each machine is a `google_compute_instance` at its address with a public address that sets
//!   itself up from its startup script (`crate::cloud::vm::hooks::user_data`), once the
//!   runner delivered the project over SSH. The SSH user comes from the `ssh-keys` metadata
//!   (created at first boot): the image's usual user, as on AWS. A machine
//!   on several networks has an interface in each VPC (up to 8, as many as its vCPUs, two at
//!   least), IP forwarding on so replies leave from the right one.
//! - A network a machine routes (`gateway:`): the machine takes the highest free address there,
//!   since Google Cloud keeps both the first address (the gateway's) and the second to last (the
//!   router's).
//!
//! What Google Cloud doesn't run the AWS way, so its refusal declines it: Windows, whose
//! first-boot set-up and WinRM this module doesn't write for Google Cloud.

mod machines;
mod network;
mod private_dns;

use std::net::Ipv4Addr;

use crate::hcl_builder::Module;
use isoloom_plan::{Environment, NetworkPlan};
use isoloom_spec::images::is_windows;
use isoloom_spec::net::Cidr;

use super::{own_project, public_ip};
use crate::cloud::Cloud;
use crate::cloud::vm::rules::{Images, Kept, Mtu, Need, Networks, OsImages, PLAYBOOK_CHECKS, Rules, Size, Sizes, Subnets, Volumes};
use crate::cloud::vm::{self, Driver, gateways, gpus::GpuSizes, own_image};
use crate::limits::Limits;
use crate::module::providers;

/// What Google Cloud takes.
const RULES: Rules = Rules {
    cloud: Cloud::Gcp,
    networks: Networks::Several,
    limits: Limits {
        arm64: Some("Isoloom generates no arm64 VM on Google Cloud"),
        // A VPC per network: a route in one VPC to another's range goes through a machine.
        gateways: None,
        playbook_checks: Some(PLAYBOOK_CHECKS),
        tc: Some("link impairment (`tc`) needs a router in the path; the cloud's firewall rules have none"),
        ipv6: Some("Google Cloud assigns a VPC's internal IPv6 range itself (inside fd20::/20), so the spec's block can't be kept"),
    },
    // NVIDIA L4s (G2, the GPUs attached). Projects get no quota for them until they ask, and
    // only some zones have them.
    gpus: GpuSizes {
        gpu: "NVIDIA L4",
        sizes: &[(1, "g2-standard-4"), (2, "g2-standard-24"), (4, "g2-standard-48"), (8, "g2-standard-96")],
        none: "",
    },
    nests: true,
    need: Need::of,
    sizes: Some(MACHINE_TYPES),
    nesting_sizes: Some(NESTING),
    mtu: Mtu::Between { low: 1300, high: 8896 },
    picks_ipv6: false,
    volumes: Volumes { smallest_gb: 10, most: 15 },
    tpm: true,
    subnets: Some(Subnets {
        called: "subnetworks",
        longest: 29,
    }),
    kept: Some(Kept {
        octets: (1, 1),
        said: "a subnetwork's first address (.1, its gateway)",
    }),
    images: Images {
        table: &IMAGES,
        own: own_image::gcp,
    },
};

/// How the module writes its machines: Linux ones (its refusal declines Windows).
const MACHINES: vm::Machines = vm::Machines {
    linux: machines::linux,
    windows: None,
    controller: machines::controller,
};

/// The machine types whose CPUs nest (N2: E2 has no nested virtualization), smallest first, as
/// [`MACHINE_TYPES`].
const NESTING: Sizes = Sizes::ordered(&[
    Size::new("n2-standard-2", 2, 8192, 2),
    Size::new("n2-standard-4", 4, 16384, 4),
    Size::new("n2-standard-8", 8, 32768, 8),
]);

/// The image family and SSH user of each OS. The user is created from the `ssh-keys` metadata,
/// so it is the AWS image's user and the inventory is the same.
const IMAGES: OsImages<(&str, &str)> = OsImages(&[
    ("debian-12", ("debian-cloud/debian-12", "admin")),
    ("debian-13", ("debian-cloud/debian-13", "admin")),
    ("ubuntu-22.04", ("ubuntu-os-cloud/ubuntu-2204-lts", "ubuntu")),
    ("ubuntu-24.04", ("ubuntu-os-cloud/ubuntu-2404-lts-amd64", "ubuntu")),
]);

/// An OS's image, when there is one.
pub(super) fn image(os: &str) -> Option<(&'static str, &'static str)> {
    IMAGES.of(os)
}

/// The SSH user of an OS's image.
pub(super) fn user(os: &str) -> &'static str {
    image(os).map_or("admin", |(_, user)| user)
}

/// The machine types Isoloom picks, smallest first: E2 shared-core up to 4 GB, then standard.
/// Each with its vCPUs, its memory, and the interfaces it takes: one per vCPU, two at least,
/// eight at most (<https://cloud.google.com/vpc/docs/create-use-multiple-interfaces#max-interfaces>).
const MACHINE_TYPES: Sizes = Sizes::ordered(&[
    Size::new("e2-small", 2, 2048, 2),
    Size::new("e2-medium", 2, 4096, 2),
    Size::new("e2-standard-2", 2, 8192, 2),
    Size::new("e2-standard-4", 4, 16384, 4),
    Size::new("e2-standard-8", 8, 32768, 8),
]);

/// Why Google Cloud can't run this environment, if it can't: its features (a machine's or the
/// controller's size among them), its address rules, then Linux machines with images, a GPU
/// type's interfaces, and a gateway machine's address.
fn refusal(env: &Environment) -> Option<String> {
    let spec = env.spec();
    if let Some(why) = RULES.features(env).or_else(|| RULES.subnets(env)) {
        return Some(why);
    }
    let machine = spec.machines.iter().find_map(|(name, m)| {
        let vm = m.vm.as_ref()?;
        if is_windows(&vm.os) {
            return Some(format!("machine `{name}`: Isoloom generates no Windows VM on Google Cloud"));
        }
        gpu_interfaces(env, name).or_else(|| RULES.image(name, m)).or_else(|| RULES.kept(env, name, m))
    });
    if machine.is_some() {
        return machine;
    }
    gateways::routed(env)
        .into_iter()
        .find(|(n, _)| relocate(env, n).is_none())
        .map(|(net, _)| format!("network `{net}`: no address left for its gateway machine (Google Cloud keeps the router's)"))
}

/// Why GPU machine `name`'s networks can't each be an interface, if they can't: a GPU type takes
/// one per vCPU (its name's last number, `g2-standard-4`: 4), as [`MACHINE_TYPES`] say of theirs.
fn gpu_interfaces(env: &Environment, name: &str) -> Option<String> {
    let mp = env.machines.get(name)?;
    let t = RULES.gpus.of(mp)?;
    let nics = mp.addresses.len();
    let vcpus: usize = t.rsplit('-').next().and_then(|n| n.parse().ok()).unwrap_or(1);
    (vcpus < nics).then(|| format!("machine `{name}` is on {nics} networks: a {t} takes {vcpus} interfaces at most (one per vCPU)"))
}

/// Where a gateway machine goes on the network it routes: the highest address nothing else takes.
fn relocate(env: &Environment, net: &str) -> Option<Ipv4Addr> {
    gateways::free_address(env, net, Cidr::parse(&env.networks.get(net)?.cidr)?)
}

/// A network's VPC's MTU: the one it sets, else Ethernet's 1500, as on every other target
/// (Google's default is 1460).
fn mtu(n: &NetworkPlan) -> u16 {
    n.mtu.unwrap_or(1500)
}

/// Its `cloud-vm` driver: its module takes the billing account its project goes on.
pub(crate) const DRIVER: Driver = Driver {
    base: super::BASE,
    requires: (&[providers::GOOGLE], &[]),
    refusal,
    write,
};

/// The module's blocks after the providers, from the plan.
fn write(env: &Environment, m: &mut Module) -> crate::Result<()> {
    let env = &*gateways::relocated_with(env, &|e, n, _| relocate(e, n));
    network::blocks(env, m);
    private_dns::write(env, own_project(), m);
    MACHINES.write_with_outputs(env, super::BASE.user, public_ip, m)
}
