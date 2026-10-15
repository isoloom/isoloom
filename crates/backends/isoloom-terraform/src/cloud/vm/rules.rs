//! What a cloud takes, declared once and read by its refusal ([`Rules`]): the networks it
//! models, the features it lacks, its GPU types, whether its VMs nest, its networks' MTU,
//! whether it picks IPv6 addresses itself, its volumes, its TPM and its images.

use std::ops::RangeInclusive;

use isoloom_plan::Environment;
use isoloom_spec::model::{Ipv6Addresses, Machine};
use isoloom_spec::windows::Tpm;

use super::gpus::{self, GpuSizes};
use super::{disks, gateways, nested, own_image};
use crate::cloud::Cloud;
use crate::limits::Limits;
use crate::machine::{controller, vms};

/// Why the clouds refuse Ansible checks (`.yml`): no cloud runs them yet.
pub(crate) const PLAYBOOK_CHECKS: &str = "Ansible checks (.yml) in the cloud come later";

/// Why a cloud whose firewalls stand for the router refuses link impairment (`tc`).
pub(crate) const FIREWALLS_TC: &str = "link impairment (`tc`) needs a router in the path; the cloud's firewalls have none";

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

/// A cloud's images for the OSes a machine runs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Images {
    /// The cloud, as the refusal names it (`GCP`).
    pub(crate) label: &'static str,
    /// Whether it has an image for an OS.
    pub(crate) has: fn(&str) -> bool,
    /// The OSes it has images for, as the refusal lists them.
    pub(crate) list: &'static str,
    /// The field of `vm.image` naming a machine's own image there, which stands in for its OS's.
    pub(crate) own: own_image::Field,
}

impl Images {
    /// Why machine `name` has no image here: none for its OS, and none of its own.
    pub(crate) fn missing(&self, name: &str, os: &str, own: Option<&str>) -> Option<String> {
        (!(self.has)(os) && own.is_none()).then(|| format!("machine `{name}`: no {} image for `{os}` yet ({})", self.label, self.list))
    }
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
}

impl Rules {
    /// The first feature the environment asks for and the cloud can't give, in this order: its
    /// limits, GPUs, nesting, a network's MTU, IPv6 addresses it doesn't pick, data disks.
    pub(crate) fn features(&self, env: &Environment) -> Option<String> {
        self.limits
            .refusal(env)
            .or_else(|| self.gpus(env))
            .or_else(|| self.nesting(env))
            .or_else(|| self.mtu(env))
            .or_else(|| self.ipv6_addresses(env))
            .or_else(|| self.volumes(env))
    }

    /// Why the cloud can't give the machines their GPUs, if it can't.
    pub(crate) fn gpus(&self, env: &Environment) -> Option<String> {
        gpus::refusal(env, self.cloud, &self.gpus)
    }

    /// Why the cloud can't run the machines' VMs inside them, if it can't.
    pub(crate) fn nesting(&self, env: &Environment) -> Option<String> {
        nested::refusal(env, self.cloud, self.nests)
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
            return Some(format!("{cloud} cloud-vm is Linux only so far"));
        }
        if controller::needed(env) {
            return Some(format!("{cloud} cloud-vm has no Ansible controller yet"));
        }
        if let Some(why) = self.features(env) {
            return Some(why);
        }
        if let Some((name, _)) = vms(env).find(|(_, mp)| mp.tpm == Some(Tpm::Required)).filter(|_| !self.tpm) {
            return Some(format!("machine `{name}` asks for a virtual TPM (`vm.tpm: true`): {cloud} gives none"));
        }
        vms(env).find_map(|(name, mp)| {
            let os = mp.os.as_deref().unwrap_or_default();
            self.images.missing(name, os, own_image::of(mp, self.images.own))
        })
    }

    /// Why the cloud's networks can't model the environment's, if they can't.
    fn networks_refusal(&self, env: &Environment) -> Option<String> {
        let cloud = self.cloud;
        match self.networks {
            Networks::OneVm if env.networks.len() != 1 || vms(env).count() > 1 => Some(format!(
                "{cloud} cloud-vm takes one machine on one network: a droplet joins one VPC, at a private address {cloud} picks, so other machines couldn't reach it at its spec address"
            )),
            Networks::OnePerMachine => vms(env)
                .find(|(_, mp)| mp.addresses.len() > 1)
                .map(|(name, _)| format!("machine `{name}` is on several networks: a {cloud} instance has one VPC interface")),
            _ => None,
        }
    }
}

/// The network where machine `name` takes one of the `kept` addresses of its subnet, which the
/// cloud keeps for itself. A gateway machine's gateway address doesn't count: it moves elsewhere
/// (`gateways`).
pub(crate) fn kept_address<'a>(env: &Environment, name: &str, m: &'a Machine, kept: RangeInclusive<u8>) -> Option<&'a str> {
    m.networks
        .iter()
        .find(|(n, o)| kept.contains(o) && !gateways::is_gateway_address(env, name, n))
        .map(|(n, _)| n.as_str())
}
