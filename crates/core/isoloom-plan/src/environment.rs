//! The environment as one target builds it: every fact a backend needs, resolved once from a
//! validated spec (addresses, names, routes, published ports, volumes, settings, images, start
//! order, checks, requirements), so backends render the same decisions instead of each
//! re-deriving them from the spec.
//!
//! The plan is typed and target-aware (on the Docker targets, networks sit on their Docker
//! blocks); a backend reads it and writes its own format. What the plan doesn't model yet
//! (appliance wiring, trunk scripts) a backend still reads from [`Environment::spec`], the spec
//! as laid out for the target.

use std::net::{Ipv4Addr, Ipv6Addr};

use indexmap::IndexMap;
use isoloom_spec::images;
use isoloom_spec::model::{Arch, Boot, Constraints, External, Machine, MachineFile, Network, Protocol, Spec, Target, Tc, Ulimit, VmImage, Winrm};
use isoloom_spec::validate::docker_cidrs;
use isoloom_spec::windows::{self, Tpm};

mod image;

pub use image::{Build, Image};

use crate::addressing::{address, address_for, host_address, start_order};
use crate::ansible;
use crate::checks::{self, Resolved};
use crate::hardware::{self, Hardware};
use crate::images::Images;
use crate::ipv6::{self, Ipv6Plan, Route6};
use crate::lifecycle::{self, MachineLifecycle};
use crate::outside::{self, OutsideAddress, OutsideNetwork};
use crate::process::Process;
use crate::requirements::{self, Requirement};
use crate::routing;
use crate::secrets::SecretFile;
use crate::storage::{self, DataDisk, Lifetime};

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
    /// The spec's constraints (proxy, mirrors, registries, NTP), when it has any.
    pub constraints: Option<Constraints>,
    /// What the environment asks of the backend rendering it.
    pub requirements: Vec<Requirement>,
    /// The image references to pull, through the registry mirrors.
    pub images: Images,
    /// The Ansible inventory groups and their members (see [`ansible::groups`]).
    pub inventory_groups: IndexMap<String, Vec<String>>,
    /// The private DNS zone, when `dns.private` is on: its domain and records.
    pub private_dns: Option<crate::private_dns::PrivateDns>,
    spec: Spec,
}

/// A network, as the target lays it out.
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkPlan {
    /// Its block on this target (the Docker block on the Docker targets).
    pub cidr: String,
    /// Its block as the author wrote it.
    pub declared_cidr: String,
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

/// How a machine is produced on the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineShape {
    /// A container.
    Container,
    /// A virtual machine.
    Vm,
    /// An access machine the runner supplies (nothing to build).
    Supplied,
    /// An existing machine, reached as it is (every machine on the `external` target).
    External,
}

/// One machine of the environment.
#[derive(Debug, Clone, PartialEq)]
pub struct MachinePlan {
    /// How the target produces it.
    pub shape: MachineShape,
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
    /// The ports it publishes on the host.
    pub ports: Vec<PublishedPort>,
    /// Its volumes, in spec order.
    pub volumes: Vec<Volume>,
    /// The files placed on it, in spec order.
    pub files: Vec<MachineFile>,
    /// Its data disks, in spec order.
    pub disks: Vec<DataDisk>,
    /// The settings a container gets from its runtime and a VM from its set-up.
    pub settings: Settings,
    /// The inputs it reads (environment variables given at run time).
    pub inputs: Vec<String>,
    /// The secrets it gets as files, delivered at run time (never their values).
    pub secrets: Vec<SecretFile>,
    /// Its size.
    pub size: Size,
    /// Its CPU architecture.
    pub arch: Arch,
    /// The bandwidth cap on each of its links (`rate_limit`, a `tc` rate such as `10mbit`).
    pub rate_limit: Option<String>,
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
    /// The OS of its VM implementation (`debian-12`, `windows-server-2022`...), whatever the
    /// target: an existing host keeps the OS its `vm:` names.
    pub os: Option<String>,
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
}

impl MachinePlan {
    /// Whether it runs Windows.
    pub fn is_windows(&self) -> bool {
        self.os.as_deref().is_some_and(images::is_windows)
    }
}

/// A route to a network a machine isn't on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// The network's block.
    pub cidr: String,
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

/// A machine's or the controller's size; `None` leaves the target's default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Size {
    /// CPUs.
    pub cpus: Option<u32>,
    /// Memory, in MB.
    pub memory_mb: Option<u32>,
    /// Disk, in GB.
    pub disk_gb: Option<u32>,
}

/// The controller VM, which runs the environment's playbooks and Ansible checks: its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Controller {
    /// CPUs: [`Controller::CPUS`] unless the spec says.
    pub cpus: u32,
    /// Memory in MB: [`Controller::MEMORY_MB`] unless the spec says.
    pub memory_mb: u32,
    /// Disk in GB, when the spec sets one (else its image's own).
    pub disk_gb: Option<u32>,
}

impl Controller {
    /// Its CPUs when the spec leaves them out.
    pub const CPUS: u32 = 1;
    /// Its memory in MB when the spec leaves it out: Python, ansible-core and pywinrm fit.
    pub const MEMORY_MB: u32 = 512;

    /// The controller of `spec` (`controller.resources`, else the defaults), whether or not
    /// the environment needs one.
    #[must_use]
    pub fn of(spec: &Spec) -> Controller {
        let r = spec.controller.as_ref().and_then(|c| c.resources.as_ref());
        Controller {
            cpus: r.and_then(|r| r.cpus).unwrap_or(Controller::CPUS),
            memory_mb: r.and_then(|r| r.memory_mb).unwrap_or(Controller::MEMORY_MB),
            disk_gb: r.and_then(|r| r.disk_gb),
        }
    }

    /// Whether `spec` needs a controller: it has `provision:` steps or Ansible checks.
    #[must_use]
    pub fn needed(spec: &Spec) -> bool {
        !spec.provision.is_empty() || spec.checks.iter().any(isoloom_spec::Check::is_playbook)
    }
}

impl Environment {
    /// Plans a validated `spec` for `target` (`instance`: the copy it is, when several run).
    #[must_use]
    pub fn build(spec: &Spec, target: Target, instance: Option<u8>) -> Environment {
        // Laid out with the outside networks (so a moved block avoids them), then without them.
        let laid_out = outside::without(&lay_out(spec, target));
        let machines = machines(spec, &laid_out, target);
        let requirements = all_requirements(spec, &laid_out, &machines);
        let networks = laid_out
            .networks
            .iter()
            .map(|(name, n)| {
                let declared = spec.networks.get(name).map_or(&n.cidr, |o| &o.cidr);
                (name.clone(), network(&laid_out, name, n, declared))
            })
            .collect();
        Environment {
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
            constraints: laid_out.constraints.clone(),
            requirements,
            images: Images::of(&laid_out),
            inventory_groups: ansible::groups(&laid_out),
            private_dns: crate::private_dns::plan(&laid_out),
            spec: laid_out,
        }
    }

    /// The spec as laid out for the target, for what the plan doesn't model yet.
    #[must_use]
    pub fn spec(&self) -> &Spec {
        &self.spec
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

/// Whether the target runs the environment on Docker's networks (their Docker blocks).
#[must_use]
pub fn on_docker(target: Target) -> bool {
    matches!(target, Target::Docker | Target::Hosted)
}

/// The spec as `target` lays it out: on the Docker targets, every network on its Docker block.
#[must_use]
pub fn lay_out(spec: &Spec, target: Target) -> Spec {
    let mut s = spec.clone();
    if on_docker(target) {
        for (name, c) in docker_cidrs(spec) {
            if let Some(n) = s.networks.get_mut(&name) {
                n.cidr = format!("{}/{}", Ipv4Addr::from(c.base), c.len);
            }
        }
    }
    s
}

/// Network `name` (`n`) as laid out in `spec`, its block as the author wrote it `declared`.
fn network(spec: &Spec, name: &str, n: &Network, declared: &str) -> NetworkPlan {
    NetworkPlan {
        cidr: n.cidr.clone(),
        declared_cidr: declared.to_owned(),
        host: host_address(spec, name),
        gateway: n.gateway.clone(),
        internet: n.internet,
        tc: n.tc.clone(),
        vlan: n.vlan.clone(),
        ipv6: ipv6::network(spec, name),
        mtu: n.mtu,
    }
}

/// How the target produces machine `m`. Lowering keeps only the forms the target runs and
/// refuses a machine without one, so a machine with neither form here is the access machine the
/// runner supplies.
fn shape(target: Target, m: &Machine) -> MachineShape {
    if target == Target::External {
        return MachineShape::External;
    }
    let container = match target {
        Target::Docker | Target::Hosted | Target::DockerVm | Target::CloudDocker | Target::Kubernetes => true,
        Target::Hybrid => m.docker.is_some(),
        // Proxmox runs a machine with no `vm:` as an LXC container when it opts in.
        Target::Proxmox => m.vm.is_none() && m.docker.as_ref().is_some_and(|d| d.lxc),
        _ => false,
    };
    match (container, m.docker.is_some(), m.vm.is_some()) {
        (true, true, _) => MachineShape::Container,
        (false, _, true) => MachineShape::Vm,
        _ => MachineShape::Supplied,
    }
}

/// The machines of `laid_out`, each with its place on the outside networks of `spec`.
fn machines(spec: &Spec, laid_out: &Spec, target: Target) -> IndexMap<String, MachinePlan> {
    laid_out
        .machines
        .iter()
        .map(|(name, m)| {
            let mut mp = machine(laid_out, target, name, m);
            mp.outside = spec.machines.get(name).map(|om| outside::addresses(spec, om)).unwrap_or_default();
            (name.clone(), mp)
        })
        .collect()
}

/// Everything the environment requires, most fundamental first: the spec's own requirements,
/// the outside networks', then those that depend on each machine's shape.
fn all_requirements(spec: &Spec, laid_out: &Spec, machines: &IndexMap<String, MachinePlan>) -> Vec<Requirement> {
    let shape = |n: &str| machines.get(n).map(|m| m.shape);
    let mut out = requirements::of(laid_out);
    out.extend(outside::requirements(spec));
    out.extend(requirements::mixed_shapes(laid_out, shape));
    out.extend(crate::machine_files::requirements(laid_out, shape));
    out.extend(requirements::tpm(machines));
    out.extend(crate::boot::requirements(machines));
    out.extend(hardware::requirements(laid_out, shape));
    out.extend(crate::more_addresses::requirements(laid_out, shape));
    out
}

/// Machine `name` (`m`), planned for `target`.
fn machine(spec: &Spec, target: Target, name: &str, m: &Machine) -> MachinePlan {
    let shape = shape(target, m);
    MachinePlan {
        shape,
        addresses: m.networks.iter().map(|(n, o)| (n.clone(), address(spec, n, *o))).collect(),
        more_addresses: crate::more_addresses::of(spec, m),
        addresses6: ipv6::addresses(spec, m),
        names: crate::private_dns::with_domain(
            spec,
            std::iter::once(name).chain(m.aliases.iter().map(String::as_str)).map(String::from).collect(),
        ),
        routes: routing::routes(spec, name, m).into_iter().map(|(cidr, via)| Route { cidr, via }).collect(),
        routes6: if target == Target::CloudVm { Vec::new() } else { ipv6::routes(spec, m) },
        default_gateway: routing::default_gateway(spec, name, m),
        is_gateway: routing::is_gateway(spec, name),
        offline: routing::offline(spec, name, m),
        ports: published(m),
        volumes: volumes(spec, name, m),
        files: m.files.clone(),
        disks: storage::disks(name, m),
        settings: Settings {
            environment: m.environment.clone(),
            sysctls: m.sysctls.clone(),
            ulimits: m.ulimits.clone(),
        },
        inputs: m.inputs.clone(),
        secrets: m.secrets.iter().map(|s| SecretFile::of(spec, s)).collect(),
        size: m
            .resources
            .map(|r| Size {
                cpus: r.cpus,
                memory_mb: r.memory_mb,
                disk_gb: r.disk_gb,
            })
            .unwrap_or_default(),
        arch: m.arch,
        rate_limit: m.rate_limit.clone(),
        image: image::of(spec, shape, name, m),
        vm_image: m.vm.as_ref().and_then(|v| v.image.clone()),
        boot: m.vm.as_ref().and_then(|v| v.boot.clone()),
        depends_on: m.depends_on.clone(),
        lifecycle: if shape == MachineShape::Vm {
            let mut steps = lifecycle::of(spec, name, m);
            if target == Target::CloudVm {
                ipv6::cloud_routes(&mut steps);
            }
            steps
        } else {
            Vec::new()
        },
        os: m.vm.as_ref().map(|v| v.os.clone()),
        winrm: m.vm.as_ref().filter(|v| images::is_windows(&v.os) && !windows::winssh(v)).map(images::winrm),
        winssh: m.vm.as_ref().is_some_and(windows::winssh),
        tpm: m.vm.as_ref().and_then(windows::tpm),
        external: m.external.clone(),
        host_vars: ansible::host_vars(spec, name),
        outside: IndexMap::new(),
        process: Process::of(m),
        hardware: Hardware::of(m),
    }
}

fn published(m: &Machine) -> Vec<PublishedPort> {
    m.ports
        .iter()
        .filter_map(|s| {
            s.publish.map(|publish| PublishedPort {
                name: m.port_label(s),
                port: s.port,
                publish,
                protocol: s.protocol,
                fixed: s.fixed,
            })
        })
        .collect()
}

fn volumes(spec: &Spec, name: &str, m: &Machine) -> Vec<Volume> {
    m.volumes
        .iter()
        .map(|(v, mount)| Volume {
            name: v.clone(),
            path: mount.path().to_owned(),
            subpath: mount.subpath().map(String::from),
            key: spec.volume_key(name, v),
            lifetime: storage::lifetime(spec, v),
            // Only a shared volume of the spec's `volumes:` has an owner.
            kind: match spec.volume_owner(v) {
                Some(owner) => VolumeKind::Shared {
                    owner: owner.to_owned(),
                    users: spec.volume_users(v).into_iter().map(String::from).collect(),
                },
                None => VolumeKind::Own,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::{Controller, Environment};
    use crate::testing;

    const NEEDS_ONE: &str = "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12 }\nprovision: [{ ansible: site.yml }]\n";

    fn controller(extra: &str) -> Option<Controller> {
        testing::environment(&format!("{NEEDS_ONE}{extra}"), Target::Vagrant).controller
    }

    #[test]
    fn the_controller_has_one_cpu_and_512_mb_unless_the_spec_sizes_it() {
        assert_eq!(
            controller(""),
            Some(Controller {
                cpus: 1,
                memory_mb: 512,
                disk_gb: None
            })
        );
        assert_eq!(
            controller("controller: { resources: { cpus: 2, memory_mb: 768, disk_gb: 10 } }\n"),
            Some(Controller {
                cpus: 2,
                memory_mb: 768,
                disk_gb: Some(10)
            })
        );
    }

    #[test]
    fn settings_are_empty_until_one_is_set() {
        assert!(super::Settings::default().is_empty());
        let mut s = super::Settings::default();
        s.sysctls.insert("net.ipv4.ip_forward".into(), "1".into());
        assert!(!s.is_empty());
        let mut u = super::Settings::default();
        u.environment.insert("A".into(), "1".into());
        assert!(!u.is_empty());
    }

    #[test]
    fn a_probe_reaches_a_machine_at_its_address_else_by_name() {
        let env = testing::environment(
            "version: 1\nname: t\nnetworks:\n  v4: { cidr: 10.9.0.0/24 }\n  v6: { cidr: 10.8.0.0/24, ipv6: { cidr: \"fd00:8::/64\", only: true } }\n  auto: { cidr: 10.7.0.0/24, ipv6: { cidr: \"fd00:7::/64\", only: true, addresses: auto } }\nmachines:\n  web: { networks: { v4: 10, v6: 10, auto: 10 }, app: { image: nginx } }\n",
            Target::Kubernetes,
        );
        assert_eq!(env.probe_address("web", "v4"), "10.9.0.10");
        assert_eq!(env.probe_address("web", "v6"), "fd00:8::10");
        assert_eq!(env.probe_address("web", "auto"), "web");
        assert_eq!(env.probe_address("ghost", "v4"), "ghost");
    }

    /// `web` has both forms, `img` only an image, `vm` only a VM (a reboot), `ct` is an LXC
    /// container on Proxmox, `user` is supplied; `data` is shared by `vm` and `both`.
    const SHAPES: &str = "version: 1
name: t
networks: { a: { cidr: 10.1.0.0/24, ipv6: { cidr: \"fd00:1::/64\" } }, b: { cidr: 10.2.0.0/24, ipv6: { cidr: \"fd00:2::/64\" } } }
reach: [{ from: a, to: b }]
volumes: { data: { owner: vm } }
machines:
  both: { networks: { a: 10 }, os: debian@12, app: { image: nginx, steps: [{ run: a.sh }] }, volumes: { data: /d, own: /o } }
  img: { networks: { a: 11 }, app: { image: nginx } }
  vm: { networks: { b: 12 }, os: debian@12, app: { steps: [{ run: a.sh }, reboot] }, volumes: { data: /d } }
  ct: { networks: { a: 13 }, app: { image: alpine } }
  win: { networks: { a: 14 }, os: windows-server@2022 }
  user: { access: true, networks: { a: 5 } }
overrides:
  proxmox: { machines: { ct: { lxc: true } } }
  vagrant: { machines: { win: { winrm: ssl } } }
";

    fn shapes(target: Target) -> Vec<(String, super::MachineShape)> {
        testing::environment(SHAPES, target)
            .machines
            .iter()
            .map(|(n, m)| (n.clone(), m.shape))
            .collect()
    }

    fn shape_of(target: Target, machine: &str) -> Option<super::MachineShape> {
        shapes(target).into_iter().find(|(n, _)| n == machine).map(|(_, s)| s)
    }

    #[test]
    fn each_target_produces_a_machine_in_the_form_it_runs() {
        use super::MachineShape::{Container, External, Supplied, Vm};
        for t in [Target::Docker, Target::Hosted, Target::DockerVm, Target::CloudDocker, Target::Kubernetes] {
            assert_eq!(shape_of(t, "both"), Some(Container), "{t:?}");
            assert_eq!(shape_of(t, "user"), Some(Supplied), "{t:?}");
        }
        assert_eq!(shape_of(Target::Hybrid, "both"), Some(Container), "a container where it has one");
        assert_eq!(shape_of(Target::Hybrid, "vm"), Some(Vm));
        assert_eq!(shape_of(Target::Proxmox, "ct"), Some(Container), "an LXC container");
        assert_eq!(shape_of(Target::Proxmox, "both"), Some(Vm));
        assert_eq!(shape_of(Target::Vagrant, "both"), Some(Vm));
        assert_eq!(shape_of(Target::External, "both"), Some(External));
    }

    #[test]
    fn a_vm_has_set_up_steps_and_a_container_none() {
        let env = testing::environment(SHAPES, Target::Hybrid);
        let steps = |m: &str| env.machines.get(m).map_or(0, |p| p.lifecycle.len());
        assert!(steps("vm") > 0);
        assert_eq!(steps("both"), 0);
    }

    #[test]
    fn a_cloud_routes_ipv6_itself_and_the_other_vm_targets_through_the_router() {
        let routes6 = |t: Target| {
            testing::environment(SHAPES, t)
                .machines
                .get("both")
                .map(|m| m.routes6.len())
                .unwrap_or_default()
        };
        assert_eq!(routes6(Target::Vagrant), 1);
        assert_eq!(routes6(Target::CloudVm), 0);
        let cloud = testing::environment(SHAPES, Target::CloudVm);
        let ipv6_routes = cloud.machines.get("both").and_then(|m| {
            m.lifecycle.iter().find_map(|s| match s {
                crate::Step::Ipv6 { routes, .. } => Some(routes.len()),
                _ => None,
            })
        });
        assert_eq!(ipv6_routes, Some(0));
    }

    #[test]
    fn a_shared_volume_names_its_owner_and_users_and_an_own_one_is_the_machines() {
        let env = testing::environment(SHAPES, Target::Vagrant);
        let both = env.machines.get("both").map(|m| m.volumes.clone()).unwrap_or_default();
        let kinds: Vec<(&str, &super::VolumeKind)> = both.iter().map(|v| (v.name.as_str(), &v.kind)).collect();
        assert_eq!(
            kinds,
            [
                (
                    "data",
                    &super::VolumeKind::Shared {
                        owner: "vm".into(),
                        users: vec!["both".into(), "vm".into()]
                    }
                ),
                ("own", &super::VolumeKind::Own)
            ]
        );
    }

    #[test]
    fn a_windows_vm_answers_winrm_its_way() {
        let env = testing::environment(SHAPES, Target::Vagrant);
        let win = env.machines.get("win").expect("planned");
        assert!(win.is_windows() && !win.winssh);
        assert_eq!(win.winrm, Some(isoloom_spec::model::Winrm::Ssl));
        let vm = env.machines.get("vm").expect("planned");
        assert!(!vm.is_windows() && vm.winrm.is_none());
        assert!(super::on_docker(Target::Docker) && super::on_docker(Target::Hosted) && !super::on_docker(Target::DockerVm));
    }

    #[test]
    fn only_playbooks_need_a_controller() {
        let lab = testing::lab(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12 }\nchecks: [checks/a.sh]\n",
        );
        let spec = lab.for_target(Target::Vagrant);
        assert!(!Controller::needed(&spec) && Environment::build(&spec, Target::Vagrant, None).controller.is_none());
        let playbook = testing::lab(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12 }\nchecks: [checks/a.yml]\n",
        );
        assert!(Controller::needed(&playbook.for_target(Target::Vagrant)));
        assert_eq!(Controller::of(&spec).memory_mb, Controller::MEMORY_MB);
    }
}
