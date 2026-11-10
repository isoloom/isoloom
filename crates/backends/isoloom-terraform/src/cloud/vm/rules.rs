//! What a cloud takes, declared once and read by its refusal ([`Rules`]): the networks it
//! models, the features it lacks, its GPU types, whether its VMs nest, its sizes, its
//! networks' MTU, whether it picks IPv6 addresses itself, its volumes, its TPM and its images.

use std::ops::RangeInclusive;

use isoloom_plan::{Environment, MachinePlan};
use isoloom_spec::model::{Ipv6Addresses, Machine};
use isoloom_spec::net::Cidr;
use isoloom_spec::windows::Tpm;

use super::gpus::{self, GpuSizes};
use super::{disks, gateways, nested, own_image};
use crate::cloud::Cloud;
use crate::limits::Limits;
use crate::machine::{controller, vms};

/// Why the clouds refuse Ansible checks (`.yml`): no cloud runs them yet.
pub(crate) const PLAYBOOK_CHECKS: &str = "Ansible checks (.yml) don't run in the cloud";

/// Why a cloud whose firewalls stand for the router refuses link impairment (`tc`).
pub(crate) const FIREWALLS_TC: &str = "link impairment (`tc`) needs a router in the path; the cloud's firewalls have none";

/// Why a cloud whose security groups stand for the router refuses link impairment (`tc`).
pub(crate) const SECURITY_GROUPS_TC: &str = "link impairment (`tc`) needs a router in the path; the cloud's security groups have none";

/// The networks a cloud can model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Networks {
    /// One network and one VM (DigitalOcean: a droplet joins one VPC, at an address the cloud
    /// picks, so the lab's fixed addressing holds only alone).
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

/// One size of a cloud's table, and what a VM of it gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Size {
    /// The size (`t3.micro`).
    pub(crate) name: &'static str,
    /// Its vCPUs.
    pub(crate) vcpus: u32,
    /// Its memory, in MB.
    pub(crate) memory_mb: u32,
    /// Its disk, in GB, when the size fixes it (DigitalOcean's droplets, Linode's plans); `None`
    /// when a machine's disk is its own setting.
    pub(crate) disk_gb: Option<u32>,
    /// The network interfaces it takes at most.
    pub(crate) interfaces: usize,
    /// The addresses one of its interfaces takes at most, where the cloud's sizes set it (AWS);
    /// `None` when the size doesn't.
    pub(crate) addresses: Option<usize>,
}

impl Size {
    /// A size with `vcpus`, `memory_mb` and room for `interfaces`, its disk a machine's own.
    pub(crate) const fn new(name: &'static str, vcpus: u32, memory_mb: u32, interfaces: usize) -> Size {
        Size {
            name,
            vcpus,
            memory_mb,
            disk_gb: None,
            interfaces,
            addresses: None,
        }
    }

    /// The size, its disk fixed at `disk_gb`.
    pub(crate) const fn with_disk(self, disk_gb: u32) -> Size {
        Size {
            disk_gb: Some(disk_gb),
            ..self
        }
    }

    /// The size, each interface taking `addresses` at most.
    pub(crate) const fn with_addresses(self, addresses: usize) -> Size {
        Size {
            addresses: Some(addresses),
            ..self
        }
    }

    /// Whether a VM of this size has what `need` asks for.
    fn takes(&self, need: &Need) -> bool {
        self.vcpus >= need.cpus
            && self.memory_mb >= need.memory_mb
            && self.disk_gb.is_none_or(|gb| gb >= need.disk_gb)
            && self.interfaces >= need.interfaces
            && self.addresses.is_none_or(|a| a >= need.addresses)
    }
}

/// The least memory a Windows VM gets, in MB: Windows wants 2 GB and more.
const WINDOWS_MEMORY_MB: u32 = 2048;

/// What a VM asks of its size: its CPUs, memory and disk, an interface per network, and the
/// addresses its busiest interface holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Need {
    /// CPUs.
    pub(crate) cpus: u32,
    /// Memory, in MB.
    pub(crate) memory_mb: u32,
    /// Disk, in GB.
    pub(crate) disk_gb: u32,
    /// Network interfaces: one per network.
    pub(crate) interfaces: usize,
    /// The addresses of its busiest interface (1: its own).
    pub(crate) addresses: usize,
}

impl Need {
    /// `cpus`, `memory_mb` and `disk_gb` on one interface, which holds its own address.
    pub(crate) fn new(cpus: u32, memory_mb: u32, disk_gb: u32) -> Need {
        Need {
            cpus,
            memory_mb,
            disk_gb,
            interfaces: 1,
            addresses: 1,
        }
    }

    /// What machine `mp` asks for: its size (a Windows machine's 2 GB of memory at least), and an
    /// interface on each of its networks.
    pub(crate) fn of(mp: &MachinePlan) -> Need {
        let size = mp.size();
        let memory_mb = if mp.is_windows() {
            size.memory_mb.max(WINDOWS_MEMORY_MB)
        } else {
            size.memory_mb
        };
        Need::new(size.cpus, memory_mb, size.disk_gb).on(mp.addresses.len())
    }

    /// What the controller asks for: its size, and an interface on every network.
    pub(crate) fn controller(env: &Environment) -> Need {
        let (cpus, memory_mb, disk_gb) = controller::size(env);
        Need::new(cpus, memory_mb, disk_gb).on(env.networks.len())
    }

    /// The same on `networks` networks, an interface on each (one at least).
    pub(crate) fn on(self, networks: usize) -> Need {
        Need {
            interfaces: networks.max(1),
            ..self
        }
    }

    /// The same, its busiest interface holding `addresses`.
    pub(crate) fn with_addresses(self, addresses: usize) -> Need {
        Need { addresses, ..self }
    }
}

/// A cloud's size table, smallest first, each size at least the one before it in every
/// respect ([`Sizes::ordered`]), so its last takes whatever any of them takes: a VM gets the first
/// that takes what it asks for, and one the last can't take is refused ([`Sizes::short`]).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sizes(&'static [Size]);

impl Sizes {
    /// `table` as a size table.
    ///
    /// # Panics
    ///
    /// When a size has less of something than the one before it (at compile time, for the
    /// constant tables).
    pub(crate) const fn ordered(table: &'static [Size]) -> Sizes {
        let mut rest = table;
        while let [a, b, ..] = rest {
            let disk = match (a.disk_gb, b.disk_gb) {
                (Some(a), Some(b)) => b >= a,
                (a, b) => a.is_none() && b.is_none(),
            };
            let addresses = match (a.addresses, b.addresses) {
                (Some(a), Some(b)) => b >= a,
                (a, b) => a.is_none() && b.is_none(),
            };
            assert!(
                b.vcpus >= a.vcpus && b.memory_mb >= a.memory_mb && b.interfaces >= a.interfaces && disk && addresses,
                "a size table's sizes grow in every respect"
            );
            if let [_, tail @ ..] = rest {
                rest = tail;
            }
        }
        Sizes(table)
    }

    /// The sizes' names, smallest first.
    #[cfg(test)]
    pub(crate) fn names(self) -> impl Iterator<Item = &'static str> {
        self.0.iter().map(|s| s.name)
    }

    /// The smallest size that takes `need`; the largest when none does, which the cloud's
    /// refusal ([`Sizes::short`]) rules out.
    pub(crate) fn fit(self, need: &Need) -> &'static str {
        self.0.iter().find(|s| s.takes(need)).or(self.0.last()).map_or("", |s| s.name)
    }

    /// What the largest size lacks for `who` (``machine `web` ``, `the controller`), which asks
    /// for `need` on `cloud`, if it lacks something: CPUs, memory, disk, interfaces, then
    /// addresses on an interface.
    pub(crate) fn short(self, cloud: Cloud, who: &str, need: &Need) -> Option<String> {
        let largest = self.0.last()?;
        let has = |what: String| format!("the largest {cloud} size here, `{}`, {what}", largest.name);
        if need.cpus > largest.vcpus {
            return Some(format!("{who} asks for {} CPUs: {}", need.cpus, has(format!("has {} vCPUs", largest.vcpus))));
        }
        if need.memory_mb > largest.memory_mb {
            return Some(format!(
                "{who} asks for {} of memory: {}",
                mb(need.memory_mb),
                has(format!("has {}", mb(largest.memory_mb)))
            ));
        }
        if let Some(disk) = largest.disk_gb.filter(|gb| need.disk_gb > *gb) {
            return Some(format!("{who} asks for a {} GB disk: {}", need.disk_gb, has(format!("has {disk} GB"))));
        }
        if need.interfaces > largest.interfaces {
            return Some(format!(
                "{who} is on {} networks: {}",
                need.interfaces,
                has(format!("takes {} network interfaces, one per network", largest.interfaces))
            ));
        }
        let most = largest.addresses.filter(|a| need.addresses > *a)?;
        Some(format!(
            "{who} asks for {} addresses on one network interface: {}",
            need.addresses,
            has(format!("takes {most}"))
        ))
    }
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
        let small = env.networks.values().filter_map(|n| Cidr::parse(&n.cidr)).any(|c| c.prefix() > longest);
        small.then(|| format!("{} {called} are /{longest} or larger", self.cloud))
    }

    /// Why VM `name` has no image on the cloud, if it hasn't: none for its OS, and none of its
    /// own.
    pub(crate) fn image(&self, name: &str, m: &Machine) -> Option<String> {
        let vm = m.vm.as_ref()?;
        self.images.missing(self.cloud, name, &vm.os, own_image::of_machine(m, self.images.own))
    }

    /// Why machine `name` can't have its address on one of its networks, if it can't: one the
    /// cloud keeps.
    pub(crate) fn kept(&self, env: &Environment, name: &str, m: &Machine) -> Option<String> {
        let Kept { octets: (first, last), said } = self.kept?;
        kept_address(env, name, m, first..=last).map(|net| format!("machine `{name}`: {} keeps {said} on `{net}`", self.cloud))
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
        let machine = vms(env)
            .filter(|(_, mp)| self.gpus.of(mp).is_none())
            .find_map(|(name, mp)| self.sizes_of(mp)?.short(self.cloud, &format!("machine `{name}`"), &(self.need)(mp)));
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

    /// Why a cloud running Linux labs only on the networks it models (DigitalOcean, Linode,
    /// Oracle Cloud) can't run this environment, if it can't: its networks, Linux only, no
    /// controller, its features, its TPM, its images.
    pub(crate) fn linux_refusal(&self, env: &Environment) -> Option<String> {
        let cloud = self.cloud;
        if let Some(why) = self.networks_refusal(env) {
            return Some(why);
        }
        if vms(env).any(|(_, mp)| mp.is_windows()) {
            return Some(format!("{cloud} cloud-vm is Linux only"));
        }
        if controller::needed(env) {
            return Some(format!("{cloud} cloud-vm has no Ansible controller"));
        }
        if let Some(why) = self.features(env) {
            return Some(why);
        }
        if let Some((name, _)) = vms(env).find(|(_, mp)| mp.tpm == Some(Tpm::Required)).filter(|_| !self.tpm) {
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
            Networks::OneVm if env.networks.len() != 1 || vms(env).count() > 1 => Some(format!(
                "{cloud} cloud-vm takes one machine on one network: an instance joins one VPC, at a private address {cloud} picks, so other machines couldn't reach it at its spec address"
            )),
            Networks::OnePerMachine => vms(env)
                .find(|(_, mp)| mp.addresses.len() > 1)
                .map(|(name, _)| format!("machine `{name}` is on several networks: a {cloud} instance has one VPC interface")),
            _ => None,
        }
    }
}

/// An amount of memory as a refusal says it: whole GB when it is, else MB.
fn mb(memory_mb: u32) -> String {
    if memory_mb.is_multiple_of(1024) {
        format!("{} GB", memory_mb / 1024)
    } else {
        format!("{memory_mb} MB")
    }
}

/// The network where machine `name` takes one of the `kept` addresses of its subnet, which the
/// cloud keeps for itself. A gateway machine's gateway address doesn't count: it moves elsewhere
/// (`gateways`).
fn kept_address<'a>(env: &Environment, name: &str, m: &'a Machine, kept: RangeInclusive<u8>) -> Option<&'a str> {
    m.networks
        .iter()
        .find(|(n, o)| kept.contains(o) && !gateways::is_gateway_address(env, name, n))
        .map(|(n, _)| n.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_table_that_shrinks_somewhere_is_refused() {
        static SHRINKS: [Size; 2] = [Size::new("a", 4, 1024, 2), Size::new("b", 2, 2048, 2)];
        static GROWS: [Size; 2] = [Size::new("a", 2, 1024, 2), Size::new("b", 2, 2048, 3)];
        assert!(std::panic::catch_unwind(|| Sizes::ordered(&SHRINKS)).is_err());
        let need = Need::new(1, 1500, 10);
        assert_eq!(Sizes::ordered(&GROWS).fit(&need), "b");
        assert_eq!(Sizes::ordered(&GROWS).fit(&need.on(4)), "b", "the largest when none takes it");
        assert!(
            Sizes::ordered(&GROWS)
                .short(Cloud::Aws, "x", &need.on(4))
                .is_some_and(|w| w.contains("takes 3 network interfaces"))
        );
    }
}
