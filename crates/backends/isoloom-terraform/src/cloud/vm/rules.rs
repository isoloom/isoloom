//! What a cloud takes, declared once and read by its refusal ([`Rules`]): the networks it
//! models, the features it lacks, its GPU types, whether its VMs nest, its sizes, its
//! networks' MTU, whether it picks IPv6 addresses itself, its volumes, its TPM and its images.
//! Its size table and what a VM asks of it are [`super::sizes`].

use std::ops::RangeInclusive;

use isoloom_plan::{Environment, MachinePlan, Vm};
use isoloom_spec::model::Ipv6Addresses;
use isoloom_spec::windows::Tpm;

use super::gpus::{self, GpuSizes};
pub(crate) use super::sizes::{Need, Size, Sizes};
use super::{disks, gateways, nested, own_image};
use crate::cloud::Cloud;
use crate::limits::Limits;
use crate::machine::controller;

/// Why the clouds refuse Ansible checks (`.yml`): no cloud runs them.
pub(crate) const PLAYBOOK_CHECKS: &str = "Ansible checks (.yml) don't run in the cloud";

/// Why a cloud whose firewalls stand for the router refuses link impairment (`tc`).
pub(crate) const FIREWALLS_TC: &str = "link impairment (`tc`) needs a router in the path; the cloud's firewalls have none";

/// Why a cloud whose security groups stand for the router refuses link impairment (`tc`).
pub(crate) const SECURITY_GROUPS_TC: &str = "link impairment (`tc`) needs a router in the path; the cloud's security groups have none";

/// The networks a cloud can model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Networks {
    /// One network and one VM (DigitalOcean: a droplet joins one VPC, at an address the cloud
    /// picks, so the environment's fixed addressing holds only alone).
    OneVm,
    /// Several networks, each machine on one (Linode: an instance has one VPC interface).
    OnePerMachine,
    /// Several networks, a machine on several (a network interface per network).
    Several,
}

/// A network's MTU on a cloud.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mtu {
    /// The cloud has no setting for it: a network setting one is refused.
    Fixed,
    /// The cloud sets each network's own, from `low` to `high` (Google Cloud's VPCs).
    Between {
        /// The smallest it takes.
        low: u16,
        /// The largest it takes.
        high: u16,
    },
}

/// A cloud's volumes, which a machine's data disks must fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Volumes {
    /// The smallest volume it makes, in GB.
    pub(crate) smallest_gb: u32,
    /// The most a VM takes.
    pub(crate) most: usize,
}

/// The smallest subnets a cloud makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Subnets {
    /// What the cloud calls them (`subnetworks`).
    pub(crate) called: &'static str,
    /// The longest prefix it takes (29: a /29).
    pub(crate) longest: u8,
}

/// The first addresses of a subnet a cloud keeps for itself, which no machine may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Kept {
    /// The first and the last octet kept.
    pub(crate) octets: (u8, u8),
    /// Them, as the refusal says it (`a subnetwork's first address (.1, its gateway)`).
    pub(crate) said: &'static str,
}

/// AWS's and Azure's: a subnet's first addresses, its router's and the DNS's (and its
/// network address).
pub(crate) const FIRST_THREE: Kept = Kept {
    octets: (0, 3),
    said: "the first addresses of a subnet (.1 to .3)",
};

/// A cloud's images for the OSes a machine runs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Images {
    /// Its image table: the OSes it has an image for, which the refusal lists.
    pub(crate) table: &'static dyn ImageTable,
    /// The field of `vm.image` naming a machine's own image there, which stands in for its OS's.
    pub(crate) own: own_image::Field,
}

impl Images {
    /// Why machine `name` has no image on `cloud`: none for its OS, and none of its own.
    pub(crate) fn missing(&self, cloud: Cloud, name: &str, os: &str, own: Option<&str>) -> Option<String> {
        (!self.table.oses().contains(&os) && own.is_none()).then(|| format!("machine `{name}`: no {cloud} image for `{os}` ({})", self.table.listed()))
    }
}

/// A cloud's image table, as a refusal reads it.
pub(crate) trait ImageTable: std::fmt::Debug + Sync {
    /// The OSes it has an image for (their plan names, `debian-12`), in its order.
    fn oses(&self) -> Vec<&'static str>;

    /// The OSes, as a refusal lists them ([`listed`]).
    fn listed(&self) -> String {
        listed(self.oses())
    }
}

/// The images a cloud has, by the OS each is for (its plan name, `debian-12`).
#[derive(Debug)]
pub(crate) struct OsImages<T: 'static>(pub(crate) &'static [(&'static str, T)]);

impl<T: Copy> OsImages<T> {
    /// The OS's image, when there is one.
    pub(crate) fn of(&self, os: &str) -> Option<T> {
        self.0.iter().find(|(o, _)| *o == os).map(|(_, i)| *i)
    }
}

impl<T: std::fmt::Debug + Sync> ImageTable for OsImages<T> {
    fn oses(&self) -> Vec<&'static str> {
        self.0.iter().map(|(os, _)| *os).collect()
    }
}

/// OSes as a refusal lists them, from their plan names: each family once, in the order it first
/// comes, with its versions (`debian-12`, `debian-13`, `kali`: `Debian 12/13, Kali`).
pub(crate) fn listed<'a>(oses: impl IntoIterator<Item = &'a str>) -> String {
    let mut families: Vec<(String, Vec<&str>)> = Vec::new();
    for os in oses {
        let (family, version) = match os.rsplit_once('-') {
            Some((f, v)) if v.starts_with(|c: char| c.is_ascii_digit()) => (f, Some(v)),
            _ => (os, None),
        };
        let name = family
            .split('-')
            .map(|w| {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" ");
        match families.iter_mut().find(|(n, _)| *n == name) {
            Some((_, versions)) => versions.extend(version),
            None => families.push((name, version.into_iter().collect())),
        }
    }
    families
        .into_iter()
        .map(|(name, versions)| {
            if versions.is_empty() {
                name
            } else {
                format!("{name} {}", versions.join("/"))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a cloud takes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rules {
    /// The cloud.
    pub(crate) cloud: Cloud,
    /// The networks it can model.
    pub(crate) networks: Networks,
    /// The features it lacks.
    pub(crate) limits: Limits,
    /// Its GPU instance types.
    pub(crate) gpus: GpuSizes,
    /// Whether its VMs can run VMs of their own (`nested_virtualization`).
    pub(crate) nests: bool,
    /// What a machine asks of its size ([`Need::of`], or the cloud's own count of what an
    /// interface holds).
    pub(crate) need: fn(&MachinePlan) -> Need,
    /// Its sizes for a machine (`None`: its shapes are sized to the machine, Oracle Cloud's
    /// flexible ones).
    pub(crate) sizes: Option<Sizes>,
    /// Its sizes for a machine that nests (`None`: none nests, or its shapes are sized to the
    /// machine).
    pub(crate) nesting_sizes: Option<Sizes>,
    /// Its networks' MTU.
    pub(crate) mtu: Mtu,
    /// Whether it picks a machine's IPv6 address itself (`ipv6.addresses: auto`), or gives each
    /// interface the one derived from the machine's octet.
    pub(crate) picks_ipv6: bool,
    /// Its volumes.
    pub(crate) volumes: Volumes,
    /// Whether its VMs can have a virtual TPM.
    pub(crate) tpm: bool,
    /// Its images.
    pub(crate) images: Images,
    /// Its smallest subnets (`None`: any network the spec takes).
    pub(crate) subnets: Option<Subnets>,
    /// The addresses it keeps in a subnet (`None`: none the spec could give, or its own refusal
    /// says which).
    pub(crate) kept: Option<Kept>,
}

impl Rules {
    /// The first feature the environment asks for and the cloud can't give, in this order: its
    /// limits, GPUs, nesting, a size (CPUs, memory, disk, interfaces: [`Rules::size`]), a
    /// network's MTU, IPv6 addresses it doesn't pick, data disks.
    pub(crate) fn features(&self, env: &Environment) -> Option<String> {
        self.limits
            .refusal(env)
            .or_else(|| self.gpus(env))
            .or_else(|| self.nesting(env))
            .or_else(|| self.size(env))
            .or_else(|| self.mtu(env))
            .or_else(|| self.ipv6_addresses(env))
            .or_else(|| self.volumes(env))
    }

    /// Why the cloud can't make a network's subnet, if it can't: smaller than it makes.
    pub(crate) fn subnets(&self, env: &Environment) -> Option<String> {
        let Subnets { called, longest } = self.subnets?;
        let small = env.networks.values().any(|n| n.cidr.prefix() > longest);
        small.then(|| format!("{} {called} are /{longest} or larger", self.cloud))
    }

    /// Why `vm` has no image on the cloud, if it hasn't: none for its OS, and none of its own.
    pub(crate) fn image(&self, vm: Vm) -> Option<String> {
        self.images.missing(self.cloud, vm.name, vm.os, own_image::of(vm.plan, self.images.own))
    }

    /// Why `vm` can't have its address on one of its networks, if it can't: one the cloud keeps.
    pub(crate) fn kept(&self, env: &Environment, vm: Vm) -> Option<String> {
        let Kept { octets: (first, last), said } = self.kept?;
        kept_address(env, vm, first..=last).map(|net| format!("machine `{}`: {} keeps {said} on `{net}`", vm.name, self.cloud))
    }

    /// Why the cloud can't give the machines their GPUs, if it can't.
    pub(crate) fn gpus(&self, env: &Environment) -> Option<String> {
        gpus::refusal(env, self.cloud, &self.gpus)
    }

    /// Why the cloud can't run the machines' VMs inside them, if it can't.
    pub(crate) fn nesting(&self, env: &Environment) -> Option<String> {
        nested::refusal(env, self.cloud, self.nests)
    }

    /// The sizes for machine `mp`: the nesting ones when it runs VMs of its own.
    pub(crate) fn sizes_of(&self, mp: &MachinePlan) -> Option<Sizes> {
        if nested::wanted(mp) { self.nesting_sizes } else { self.sizes }
    }

    /// The size of machine `mp`: its GPUs' type when it asks for GPUs, else the smallest of its
    /// sizes ([`Rules::sizes_of`]) that takes what it asks for.
    pub(crate) fn size_of(&self, mp: &MachinePlan) -> &'static str {
        self.gpus
            .of(mp)
            .or_else(|| self.sizes_of(mp).map(|s| s.fit(&(self.need)(mp))))
            .unwrap_or_default()
    }

    /// The controller's size, on every network.
    pub(crate) fn controller_size(&self, env: &Environment) -> &'static str {
        self.sizes.map(|s| s.fit(&Need::controller(env))).unwrap_or_default()
    }

    /// Why a VM can't have its size here, if one can't: it asks for more than the cloud's largest
    /// size has ([`Sizes::short`]; a GPU type's size is its own), or the controller, on every
    /// network, does.
    pub(crate) fn size(&self, env: &Environment) -> Option<String> {
        let machine = env
            .vms()
            .filter(|Vm { plan: mp, .. }| self.gpus.of(mp).is_none())
            .find_map(|Vm { name, plan: mp, .. }| self.sizes_of(mp)?.short(self.cloud, &format!("machine `{name}`"), &(self.need)(mp)));
        machine.or_else(|| {
            controller::needed(env)
                .then_some(self.sizes?)?
                .short(self.cloud, "the controller", &Need::controller(env))
        })
    }

    /// Why a network's MTU can't be its own here, if one can't.
    pub(crate) fn mtu(&self, env: &Environment) -> Option<String> {
        match self.mtu {
            Mtu::Fixed => {
                let (name, _) = env.networks.iter().find(|(_, n)| n.mtu.is_some())?;
                Some(format!("network `{name}` sets its `mtu`: {} has no setting for a network's MTU", self.cloud))
            }
            Mtu::Between { low, high } => env
                .networks
                .iter()
                .find_map(|(name, n)| n.mtu.filter(|m| !(low..=high).contains(m)).map(|m| (name, m)))
                .map(|(name, m)| format!("network `{name}`'s `mtu` ({m}): a {} VPC takes {low} to {high}", self.cloud)),
        }
    }

    /// Why a network leaving IPv6 addresses to the target can't have them here (the cloud gives
    /// each interface the address derived from the machine's octet), if one can't.
    fn ipv6_addresses(&self, env: &Environment) -> Option<String> {
        if self.picks_ipv6 {
            return None;
        }
        let (name, _) = env
            .networks
            .iter()
            .find(|(_, n)| n.ipv6.as_ref().is_some_and(|v| v.addresses == Ipv6Addresses::Auto))?;
        Some(format!(
            "network `{name}` leaves IPv6 addresses to the target (`ipv6.addresses: auto`): Isoloom gives each {} interface the address derived from the machine's octet, so use `static`",
            self.cloud
        ))
    }

    /// Why the cloud's volumes can't be a machine's data disks, if they can't.
    pub(crate) fn volumes(&self, env: &Environment) -> Option<String> {
        disks::too_small(env, self.cloud, self.volumes)
    }

    /// Why a cloud running Linux environments only on the networks it models (DigitalOcean, Linode,
    /// Oracle Cloud) can't run this environment, if it can't: its networks, Linux only, no
    /// controller, its features, its TPM, its images.
    pub(crate) fn linux_refusal(&self, env: &Environment) -> Option<String> {
        let cloud = self.cloud;
        if let Some(why) = self.networks_refusal(env) {
            return Some(why);
        }
        if env.vms().any(|vm| vm.plan.is_windows()) {
            return Some(format!("{cloud} cloud-vm is Linux only"));
        }
        if controller::needed(env) {
            return Some(format!("{cloud} cloud-vm has no Ansible controller"));
        }
        if let Some(why) = self.features(env) {
            return Some(why);
        }
        if let Some(Vm { name, .. }) = env.vms().find(|vm| vm.plan.tpm == Some(Tpm::Required)).filter(|_| !self.tpm) {
            return Some(format!(
                "machine `{name}` asks for a virtual TPM (`hardware.tpm: required`): {cloud} gives none"
            ));
        }
        env.vms()
            .find_map(|vm| self.images.missing(self.cloud, vm.name, vm.os, own_image::of(vm.plan, self.images.own)))
    }

    /// Why the cloud's networks can't model the environment's, if they can't.
    fn networks_refusal(&self, env: &Environment) -> Option<String> {
        let cloud = self.cloud;
        match self.networks {
            Networks::OneVm if env.networks.len() != 1 || env.vms().count() > 1 => Some(format!(
                "{cloud} cloud-vm takes one machine on one network: an instance joins one VPC, at a private address {cloud} picks, so other machines couldn't reach it at its spec address"
            )),
            Networks::OnePerMachine => env
                .vms()
                .find(|Vm { plan: mp, .. }| mp.addresses.len() > 1)
                .map(|Vm { name, .. }| format!("machine `{name}` is on several networks: a {cloud} instance has one VPC interface")),
            _ => None,
        }
    }
}

/// The network where machine `name` takes one of the `kept` addresses of its subnet, which the
/// cloud keeps for itself. A gateway machine's gateway address doesn't count: it moves elsewhere
/// (`gateways`).
fn kept_address<'a>(env: &Environment, vm: Vm<'a>, kept: RangeInclusive<u8>) -> Option<&'a str> {
    vm.plan
        .addresses
        .iter()
        .find(|(n, a)| kept.contains(&a.octets()[3]) && !gateways::is_gateway_address(env, vm.name, n))
        .map(|(n, _)| n.as_str())
}
