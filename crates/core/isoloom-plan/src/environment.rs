//! The environment as one target builds it: every fact a backend needs, resolved once from a
//! validated spec (addresses, names, routes, published ports, volumes, settings, images, start
//! order, checks, requirements), so backends render the same decisions instead of each
//! re-deriving them from the spec.
//!
//! The plan is typed and target-aware (on the Docker targets, networks sit on their Docker
//! blocks); a backend reads it and writes its own format. What a backend only writes out as the
//! author gave it (a machine's port list, its DNS and VM settings), or what it alone does
//! (Compose's appliance wiring and trunks), it reads from [`Environment::spec`], the spec as laid
//! out for the target.

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr};

use indexmap::IndexMap;
use isoloom_spec::images;
use isoloom_spec::model::{
    Arch, Boot, Constraints, External, HostFolder, Input, MachineFile, Network, Protocol, Resources, Seal, SecretSource, Spec, Target, Tc, Ulimit, VmImage,
    Winrm,
};
use isoloom_spec::net::{NetworkBlock, Rate, RateError, docker_cidrs};
use isoloom_spec::windows::Tpm;

mod controller;
mod image;
mod machines;
mod shape;
mod size;
mod vm;

pub use controller::Controller;
pub use image::{Build, Image};
use machines::machines;
use shape::shape;
pub use shape::{MachineRole, MachineShape};
pub use vm::Vm;

use crate::addressing::{address_for, cidr, host_address, start_order};
use crate::ansible;
use crate::checks::{self, Resolved};
use isoloom_catalogue::Catalogue;

use crate::configure::{DaemonTable, Recipe};
use crate::hardware::{self, Hardware};
use crate::images::Images;
use crate::ipv6::{self, Ipv6Plan, Route6};
use crate::lifecycle::MachineLifecycle;
use crate::outside::{self, OutsideAddress, OutsideNetwork};
use crate::process::Process;
use crate::requirements::{self, Requirement};
use crate::routing;
use crate::secrets::SecretFile;
use crate::storage::{DataDisk, Lifetime};
use crate::tools::{self, ToolPlan};

/// One environment, planned for one target.
#[derive(Debug, Clone, PartialEq)]
pub struct Environment {
    /// The environment's name (Compose project, resource prefix).
    pub name: String,
    /// The target it is planned for.
    pub target: Target,
    /// The instance number, when several copies of the environment run side by side.
    pub instance: Option<u8>,
    /// Its networks, in spec order.
    pub networks: IndexMap<String, NetworkPlan>,
    /// The networks outside the environment (`external`, `bridged`), in spec order: never in
    /// [`Environment::networks`] nor in the spec the backends read.
    pub outside: IndexMap<String, OutsideNetwork>,
    /// Isoloom's router, when `reach` rules need one: its address on each network it routes.
    pub router: Option<IndexMap<String, Ipv4Addr>>,
    /// The router's IPv6 address on each IPv6 network it routes (empty without a router).
    pub router6: IndexMap<String, Ipv6Addr>,
    /// Its machines, in spec order.
    pub machines: IndexMap<String, MachinePlan>,
    /// Machine names, each after the machines it waits for.
    pub start_order: Vec<String>,
    /// The checks, resolved to positions and probes.
    pub checks: Vec<Resolved>,
    /// The controller VM running playbooks, when the environment needs one.
    pub controller: Option<Controller>,
    /// The spec's constraints (proxy, mirrors, registries, NTP), when it has any: the proxy's
    /// `no_proxy` lists everything reached without it, localhost and the environment's machine
    /// names and network blocks before the spec's own.
    pub constraints: Option<Constraints>,
    /// The launch values it reads (`inputs`), in spec order, each with whether it is sensitive:
    /// names only, never values.
    pub inputs: Vec<Input>,
    /// The tools beside it (`tools:`), by name, in spec order, each at its addresses.
    pub tools: IndexMap<String, ToolPlan>,
    /// The secrets, by name, each with where the runner reads its value (an input's file: the
    /// input), in spec order: never their values.
    pub secrets: IndexMap<String, SecretSource>,
    /// What the environment asks of the backend rendering it.
    pub requirements: Vec<Requirement>,
    /// The image references to pull, through the registry mirrors.
    pub images: Images,
    /// The Ansible inventory groups and their members (see `ansible::groups`).
    pub inventory_groups: IndexMap<String, Vec<String>>,
    /// The private DNS zone, when `dns.private` is on: its domain and records.
    pub private_dns: Option<crate::private_dns::PrivateDns>,
    /// The region each cloud's module defaults to, by the cloud's id (`aws`), when the person's
    /// defaults set one ([`crate::Lab::with_regions`]); a cloud left out keeps its own.
    pub regions: BTreeMap<String, String>,
    spec: Spec,
    /// The catalogue it was planned with, for [`Environment::replan`] and the backends.
    catalogue: &'static Catalogue,
    /// The daemon table its machines' configurations were resolved with, for [`Environment::replan`].
    daemons: DaemonTable,
}

/// A network, as the target lays it out.
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkPlan {
    /// Its block on this target (the Docker block on the Docker targets), parsed once here: the
    /// addresses Isoloom reserves on the network are its own.
    pub cidr: NetworkBlock,
    /// Its block as the author wrote it.
    pub declared_cidr: NetworkBlock,
    /// The host's own address on it (Docker's bridge, the hypervisor).
    pub host: Ipv4Addr,
    /// A machine of the environment that routes it, if any.
    pub gateway: Option<String>,
    /// Whether its machines may reach the internet.
    pub internet: bool,
    /// Link impairment on traffic entering it.
    pub tc: Option<Tc>,
    /// The LAN and VLAN id it is, when it is a LAN's VLAN.
    pub vlan: Option<(String, u16)>,
    /// Its IPv6 side, when it is dual-stack or IPv6-only.
    pub ipv6: Option<Ipv6Plan>,
    /// The largest packet it carries, when the spec sets one (`mtu`).
    pub mtu: Option<u16>,
}

/// One machine of the environment.
#[derive(Debug, Clone, PartialEq)]
pub struct MachinePlan {
    /// How the target produces it.
    pub shape: MachineShape,
    /// What it is to the user: the access machine, or a member of the environment.
    pub role: MachineRole,
    /// Its address on each of its networks, in its order (the first is its primary network).
    pub addresses: IndexMap<String, Ipv4Addr>,
    /// Its addresses after the primary one on each network where it has several (`lan: [10,
    /// 11]`), in its networks' order.
    pub more_addresses: IndexMap<String, Vec<Ipv4Addr>>,
    /// Its IPv6 address on each IPv6 network that derives them (not on `addresses: auto` ones).
    pub addresses6: IndexMap<String, Ipv6Addr>,
    /// Its names: its own, then its aliases, then (with `dns.private`) each of them under the
    /// private zone's domain.
    pub names: Vec<String>,
    /// Routes to the networks it isn't on.
    pub routes: Vec<Route>,
    /// IPv6 routes to the IPv6 blocks it isn't on, through the router.
    pub routes6: Vec<Route6>,
    /// Its default route through a gateway machine, if one routes one of its networks.
    pub default_gateway: Option<Ipv4Addr>,
    /// Whether it is the gateway of one of its networks (it forwards).
    pub is_gateway: bool,
    /// Whether nothing it is on reaches the internet and no gateway decides for it.
    pub offline: bool,
    /// How strongly its offline seal holds: `best-effort` when the machine (a privileged
    /// container) can lift it from inside, `guaranteed` (the default) otherwise.
    pub seal: Seal,
    /// The ports it publishes on the host.
    pub ports: Vec<PublishedPort>,
    /// Its volumes, in spec order.
    pub volumes: Vec<Volume>,
    /// The files placed on it, in spec order.
    pub files: Vec<MachineFile>,
    /// Its data disks, in spec order.
    pub disks: Vec<DataDisk>,
    /// The folders of the computer that runs the environment it mounts both ways (`host_folders`), in
    /// spec order.
    pub host_folders: Vec<HostFolder>,
    /// The settings a container gets from its runtime and a VM from its set-up.
    pub settings: Settings,
    /// The inputs it reads (environment variables given at run time).
    pub inputs: Vec<String>,
    /// The secrets it gets as files, delivered at run time (never their values).
    pub secrets: Vec<SecretFile>,
    /// Its size as the spec gives it (`resources`, each field it leaves out `None`); the size
    /// it runs at is [`MachinePlan::size`].
    pub resources: Resources,
    /// Its CPU architecture: what its VM, box or cloud instance is built for.
    pub arch: Arch,
    /// The architecture its container is pinned to, when its spec asks for one or its catalogue
    /// image has one only (see [`isoloom_spec::model::Machine::platform`]); `None`: the host's.
    pub platform: Option<Arch>,
    /// The bandwidth cap on each of its links (`rate_limit`, a `tc` rate such as `10mbit`).
    pub rate_limit: Option<Rate>,
    /// What it runs from, on this target.
    pub image: Option<Image>,
    /// The VM's own image per target (`vm.image`: a box URL, a Proxmox or cloud image), when
    /// the spec gives one.
    pub vm_image: Option<VmImage>,
    /// How the VM boots, when not the way its image does (`vm.boot`).
    pub boot: Option<Boot>,
    /// The machines it waits for, as the spec lists them.
    pub depends_on: Vec<String>,
    /// The set-up steps of a VM, in order (empty for the other shapes).
    pub lifecycle: MachineLifecycle,
    /// The OS its `vm:` form names (`debian-12`, `windows-server-2022`...), whatever the target:
    /// a VM's always ([`crate::Vm::os`]), an existing host's or a container's when it has that
    /// form. Private, so a VM's OS is read through [`crate::Vm`], never defaulted.
    os: Option<String>,
    /// How its Windows VM answers WinRM (`None` off Windows, and on a box reached over OpenSSH).
    pub winrm: Option<Winrm>,
    /// Whether its Windows VM's box is reached over OpenSSH instead (`vm.image.winssh`).
    pub winssh: bool,
    /// Its virtual TPM 2.0, when it gets one (`vm.tpm`, or Windows 11's own).
    pub tpm: Option<Tpm>,
    /// Its SSH endpoint as an existing host (`external:`), when the spec gives one.
    pub external: Option<External>,
    /// Its variables from the `provision:` steps' `host_vars`, merged in step order.
    pub host_vars: IndexMap<String, serde_json::Value>,
    /// Its addresses on the outside networks it joins, after its own networks.
    pub outside: IndexMap<String, OutsideAddress>,
    /// Its own command, working directory, `before` steps and healthcheck.
    pub process: Process,
    /// The GPUs, host devices and nested virtualization it asks for.
    pub hardware: Hardware,
    /// Its configuration resolved (an app's steps), which its container's build or its VM's
    /// set-up renders: the one source every CM renderer reads.
    pub recipe: Option<Recipe>,
}

impl MachinePlan {
    /// Whether it is the access machine.
    #[must_use]
    pub fn is_access(&self) -> bool {
        self.role == MachineRole::Access
    }

    /// Whether it runs Windows.
    pub fn is_windows(&self) -> bool {
        self.os.as_deref().is_some_and(images::is_windows)
    }
}

/// A route to a network a machine isn't on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    /// The network's block.
    pub cidr: NetworkBlock,
    /// The next hop.
    pub via: Ipv4Addr,
}

/// A port published on the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedPort {
    /// The port's name: how labels, outputs and the message refer to it.
    pub name: String,
    /// The machine's port.
    pub port: u16,
    /// The host port asked for.
    pub publish: u16,
    /// TCP or UDP.
    pub protocol: Protocol,
    /// Whether the host port is always `publish` (else one the host picks, unless pinned).
    pub fixed: bool,
}

/// A machine's volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// The volume's name in the spec.
    pub name: String,
    /// Where the machine mounts it.
    pub path: String,
    /// The folder of a shared volume it takes, if not all of it.
    pub subpath: Option<String>,
    /// The name the runtime knows it by (a Compose named volume).
    pub key: String,
    /// How long its storage lives.
    pub lifetime: Lifetime,
    /// Whether the machine keeps it to itself or shares it.
    pub kind: VolumeKind,
}

/// Who a volume belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeKind {
    /// The machine's own.
    Own,
    /// One of the spec's shared `volumes:`: `owner` serves it to the other `users`.
    Shared {
        /// The machine that holds the data.
        owner: String,
        /// Every machine that mounts it, the owner included.
        users: Vec<String>,
    },
}

/// The settings a container gets from its runtime: environment, sysctls, ulimits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// Fixed environment variables.
    pub environment: IndexMap<String, String>,
    /// Kernel parameters.
    pub sysctls: IndexMap<String, String>,
    /// Resource limits.
    pub ulimits: IndexMap<String, Ulimit>,
}

impl Settings {
    /// Whether there is nothing to set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.environment.is_empty() && self.sysctls.is_empty() && self.ulimits.is_empty()
    }
}

impl Environment {
    /// Plans another validated `spec` for `target` (`instance`: the copy it is) with what this
    /// environment was planned with (its catalogue's daemon table): for a backend that derives
    /// one environment from another (a hybrid's containers alone, a composite's Docker host).
    ///
    /// # Errors
    ///
    /// As [`Lab::plan`](crate::Lab::plan).
    pub fn replan(&self, spec: &Spec, target: Target, instance: Option<u8>) -> Result<Environment, PlanError> {
        Environment::planned(spec, target, instance, self.catalogue, &self.daemons)
    }

    /// The catalogue it was planned with (its `Lab`'s). Hidden: it returns a building-block crate's
    /// type, as [`Lab::catalogue`](crate::Lab::catalogue) does.
    #[doc(hidden)]
    #[must_use]
    pub fn catalogue(&self) -> &'static Catalogue {
        self.catalogue
    }

    /// Plans a validated `spec` for `target` (`instance`: the copy it is, when several run) with
    /// its `Lab`'s `catalogue`, each machine's configuration resolved with `daemons`, the
    /// catalogue's ([`Lab::plan`](crate::Lab::plan)).
    ///
    /// # Errors
    ///
    /// [`PlanError::UnresolvedRecipe`] for a machine whose configuration has steps its OS's
    /// family can't carry, which lowering refuses for the target first.
    pub(crate) fn planned(
        spec: &Spec,
        target: Target,
        instance: Option<u8>,
        catalogue: &'static Catalogue,
        daemons: &DaemonTable,
    ) -> Result<Environment, PlanError> {
        // Laid out with the outside networks (so a moved block avoids them), then without them.
        let laid_out = outside::without(&lay_out(spec, target));
        let machines = machines(spec, &laid_out, target, daemons)?;
        let requirements = all_requirements(spec, &laid_out, &machines);
        let networks = laid_out
            .networks
            .iter()
            .map(|(name, n)| {
                let declared = if spec.networks.contains_key(name) { spec } else { &laid_out };
                (name.clone(), network(&laid_out, name, n, cidr(declared, name)))
            })
            .collect();
        Ok(Environment {
            name: laid_out.name.clone(),
            target,
            instance,
            networks,
            outside: outside::networks(spec),
            router: routing::needed(&laid_out).then(|| routing::networks(&laid_out).map(|n| (n.clone(), routing::address(&laid_out, n))).collect()),
            router6: ipv6::router(&laid_out),
            machines,
            start_order: start_order(&laid_out).into_iter().map(String::from).collect(),
            checks: checks::plan(&laid_out),
            controller: Controller::needed(&laid_out).then(|| Controller::of(&laid_out)),
            constraints: crate::constraints::of(&laid_out),
            inputs: laid_out.inputs.clone(),
            secrets: laid_out.secrets.clone(),
            tools: tools::of(&laid_out),
            requirements,
            images: Images::of(&laid_out),
            inventory_groups: ansible::groups(&laid_out),
            private_dns: crate::private_dns::plan(&laid_out),
            spec: laid_out,
            regions: BTreeMap::new(),
            catalogue,
            daemons: daemons.clone(),
        })
    }

    /// The spec as laid out for the target: for what a backend only writes out as the author gave
    /// it (a machine's port list, its DNS and VM settings) or what it alone does, which the plan
    /// decides nothing about.
    #[must_use]
    pub fn spec(&self) -> &Spec {
        &self.spec
    }

    /// The names it takes on the host (its VMs, networks and libvirt domains), keyed by its name
    /// and so by its instance: the one place a backend builds them from.
    #[must_use]
    pub fn names(&self) -> crate::Names<'_> {
        crate::Names::new(&self.name)
    }

    /// The names of the launch values it reads, in spec order.
    #[must_use]
    pub fn input_names(&self) -> Vec<String> {
        self.inputs.iter().map(|i| i.name.clone()).collect()
    }

    /// The default of launch value `name`, when the spec declares one: what the generated files
    /// use when the runner gives no value (unset or empty).
    #[must_use]
    pub fn input_default(&self, name: &str) -> Option<&str> {
        self.inputs.iter().find(|i| i.name == name).and_then(|i| i.default.as_deref())
    }

    /// The address `from` reaches `to` at: on a network they share, else `to`'s first.
    #[must_use]
    pub fn address_for(&self, from: &str, to: &str) -> Ipv4Addr {
        address_for(&self.spec, from, to)
    }

    /// Where a probe reaches machine `name` on `network`: its IPv4 address, or on an IPv6-only
    /// network its IPv6 one. Its name, which the target's DNS resolves, when it has no address
    /// there: on an IPv6-only network that leaves addresses to the target (or a machine or a
    /// network the plan doesn't hold, which no check of the plan names).
    #[must_use]
    pub fn probe_address(&self, name: &str, network: &str) -> String {
        let m = self.machines.get(name);
        let address = if ipv6::only(&self.spec, network) {
            m.and_then(|m| m.addresses6.get(network)).map(ToString::to_string)
        } else {
            m.and_then(|m| m.addresses.get(network)).map(ToString::to_string)
        };
        address.unwrap_or_else(|| name.to_owned())
    }
}

/// Why a spec can't be planned: what lowering and validation refuse first, so a bug of either
/// when it happens. Planning says so rather than plan around it (a machine with no
/// configuration at all).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PlanError {
    /// A machine's configuration has steps its OS's family can't carry ([`Recipe::of`]).
    #[error("machine `{machine}`: its configuration doesn't resolve on its OS: {}", reasons(.unportable))]
    UnresolvedRecipe {
        /// The machine.
        machine: String,
        /// Each step its family can't carry, in step order.
        unportable: Vec<crate::configure::Unportable>,
    },
    /// The catalogue built into Isoloom doesn't load, so the environment's services and daemons
    /// can't be known: planning stops rather than plan with an empty catalogue.
    #[error("this Isoloom's catalogue doesn't load: {reason}")]
    Catalogue {
        /// Why it doesn't load.
        reason: String,
    },
    /// A machine's `rate_limit` isn't a rate ([`Rate::parse`]).
    #[error("machine `{machine}`: `rate_limit`: `{rate}` {error}")]
    RateLimit {
        /// The machine.
        machine: String,
        /// Its `rate_limit`, as written.
        rate: String,
        /// Why it isn't a rate.
        error: RateError,
    },
}

/// The reasons of `unportable`, joined.
fn reasons(unportable: &[crate::configure::Unportable]) -> String {
    unportable.iter().map(|u| u.reason.as_str()).collect::<Vec<_>>().join("; ")
}

/// Whether the target runs the environment on Docker's networks (their Docker blocks).
#[must_use]
pub(crate) fn on_docker(target: Target) -> bool {
    matches!(target, Target::Docker | Target::Hosted)
}

/// The spec as `target` lays it out: on the Docker targets, every network on its Docker block.
#[must_use]
pub fn lay_out(spec: &Spec, target: Target) -> Spec {
    let mut s = spec.clone();
    if on_docker(target) {
        for (name, c) in docker_cidrs(spec) {
            if let Some(n) = s.networks.get_mut(&name) {
                n.cidr = c.to_string();
            }
        }
    }
    s
}

/// Network `name` (`n`) as laid out in `spec`, its block as the author wrote it `declared`.
fn network(spec: &Spec, name: &str, n: &Network, declared: NetworkBlock) -> NetworkPlan {
    NetworkPlan {
        cidr: cidr(spec, name),
        declared_cidr: declared,
        host: host_address(spec, name),
        gateway: n.gateway.clone(),
        internet: n.internet,
        tc: n.tc.clone(),
        vlan: n.vlan.clone(),
        ipv6: ipv6::network(spec, name),
        mtu: n.mtu,
    }
}

/// Everything the environment requires, most fundamental first: the spec's own requirements,
/// the outside networks', then those that depend on each machine's shape.
fn all_requirements(spec: &Spec, laid_out: &Spec, machines: &IndexMap<String, MachinePlan>) -> Vec<Requirement> {
    let shape = |n: &str| machines.get(n).map(|m| m.shape);
    let mut out = requirements::of(laid_out);
    out.extend(outside::requirements(spec));
    out.extend(requirements::mixed_shapes(laid_out, shape));
    out.extend(crate::machine_files::requirements(laid_out, shape));
    out.extend(crate::published::requirement(laid_out));
    out.extend(crate::existing_hosts::check_requirement(laid_out, shape));
    out.extend(hardware::tpm(machines));
    out.extend(crate::boot::requirements(machines));
    out.extend(hardware::requirements(laid_out, shape));
    out.extend(crate::more_addresses::requirements(laid_out, shape));
    out
}

#[cfg(test)]
mod tests;
