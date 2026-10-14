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
use isoloom_spec::model::{Arch, Constraints, External, Machine, MachineFile, Protocol, Spec, Target, Tc, Ulimit, Winrm};
use isoloom_spec::validate::docker_cidrs;

use crate::addressing::{address, address_for, host_address, start_order};
use crate::ansible;
use crate::checks::{self, Resolved};
use crate::images::{Images, tag_of};
use crate::ipv6::{self, Ipv6Plan, Route6};
use crate::lifecycle::{self, MachineLifecycle};
use crate::outside::{self, OutsideAddress, OutsideNetwork};
use crate::requirements::{self, Requirement};
use crate::routing;
use crate::secrets::SecretFile;

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
    /// Its IPv6 address on each IPv6 network that derives them (not on `addresses: auto` ones).
    pub addresses6: IndexMap<String, Ipv6Addr>,
    /// Its names: its own, then its aliases.
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
    /// What it runs from, on this target.
    pub image: Option<Image>,
    /// The machines it waits for, as the spec lists them.
    pub depends_on: Vec<String>,
    /// The set-up steps of a VM, in order (empty for the other shapes).
    pub lifecycle: MachineLifecycle,
    /// The OS of its VM implementation (`debian-12`, `windows-server-2022`...), whatever the
    /// target: an existing host keeps the OS its `vm:` names.
    pub os: Option<String>,
    /// How its Windows VM answers WinRM (`None` off Windows).
    pub winrm: Option<Winrm>,
    /// Its SSH endpoint as an existing host (`external:`), when the spec gives one.
    pub external: Option<External>,
    /// Its variables from the `provision:` steps' `host_vars`, merged in step order.
    pub host_vars: IndexMap<String, serde_json::Value>,
    /// Its addresses on the outside networks it joins, after its own networks.
    pub outside: IndexMap<String, OutsideAddress>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishedPort {
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
    /// The name the runtime knows it by (a Compose named volume).
    pub key: String,
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

/// What a machine runs from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Image {
    /// A container image.
    Container {
        /// The reference to run (pulled through a mirror when one is set; a built image's tag).
        reference: String,
        /// How the project builds it, if it does.
        build: Option<Build>,
    },
    /// A VM's operating system.
    Vm {
        /// The OS id (`ubuntu-24.04`, `windows-2022`...).
        os: String,
    },
}

/// A container image the project builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Build {
    /// The build context, from the project folder.
    pub context: String,
    /// The Dockerfile, from the project folder, when not the context's own.
    pub dockerfile: Option<String>,
    /// The Dockerfile's stage to build, when not its last.
    pub target: Option<String>,
    /// The spec's build arguments.
    pub args: IndexMap<String, String>,
}

/// The controller VM, which runs the environment's playbooks and Ansible checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Controller {
    /// Its size (1 CPU and 512 MB unless the spec says otherwise).
    pub size: Size,
}

impl Environment {
    /// Plans a validated `spec` for `target` (`instance`: the copy it is, when several run).
    pub fn build(spec: &Spec, target: Target, instance: Option<u8>) -> Environment {
        let original = spec;
        // Laid out with the outside networks (so a moved block avoids them), then without them.
        let laid_out = outside::without(&lay_out(spec, target));
        let spec = &laid_out;
        let router = routing::needed(spec).then(|| routing::networks(spec).map(|n| (n.clone(), routing::address(spec, n))).collect());
        let machines: IndexMap<String, MachinePlan> = spec
            .machines
            .iter()
            .map(|(n, m)| {
                let mut mp = machine(spec, target, n, m);
                mp.outside = original.machines.get(n).map(|m| outside::addresses(original, m)).unwrap_or_default();
                (n.clone(), mp)
            })
            .collect();
        let mut requirements = requirements::of(spec);
        requirements.extend(outside::requirements(original));
        requirements.extend(requirements::mixed_shapes(spec, |n| machines.get(n).map(|m| m.shape)));
        requirements.extend(crate::machine_files::requirements(spec, |n| machines.get(n).map(|m| m.shape)));
        Environment {
            name: spec.name.clone(),
            target,
            instance,
            networks: spec
                .networks
                .keys()
                .map(|n| (n.clone(), network(spec, &original.networks[n].cidr, n)))
                .collect(),
            outside: outside::networks(original),
            router,
            router6: ipv6::router(spec),
            machines,
            start_order: start_order(spec).into_iter().map(String::from).collect(),
            checks: checks::plan(spec),
            controller: controller(spec),
            constraints: spec.constraints.clone(),
            requirements,
            images: Images::of(spec),
            inventory_groups: ansible::groups(spec),
            spec: laid_out.clone(),
        }
    }

    /// The spec as laid out for the target, for what the plan doesn't model yet.
    pub fn spec(&self) -> &Spec {
        &self.spec
    }

    /// The address `from` reaches `to` at: on a network they share, else `to`'s first.
    pub fn address_for(&self, from: &str, to: &str) -> Ipv4Addr {
        address_for(&self.spec, from, to)
    }

    /// Where a probe reaches machine `name` on `network`: its IPv4 address, or on an IPv6-only
    /// network its IPv6 one (its name, which the target's DNS resolves, when the target hands
    /// addresses out).
    pub fn probe_address(&self, name: &str, network: &str) -> String {
        let m = &self.machines[name];
        if !ipv6::only(&self.spec, network) {
            return m.addresses[network].to_string();
        }
        m.addresses6.get(network).map_or_else(|| name.to_string(), ToString::to_string)
    }
}

/// Whether the target runs the environment on Docker's networks (their Docker blocks).
pub fn on_docker(target: Target) -> bool {
    matches!(target, Target::Docker | Target::Hosted)
}

/// The spec as `target` lays it out: on the Docker targets, every network on its Docker block.
fn lay_out(spec: &Spec, target: Target) -> Spec {
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

fn network(spec: &Spec, declared: &str, name: &str) -> NetworkPlan {
    let n = &spec.networks[name];
    NetworkPlan {
        cidr: n.cidr.clone(),
        declared_cidr: declared.to_string(),
        host: host_address(spec, name),
        gateway: n.gateway.clone(),
        internet: n.internet,
        tc: n.tc.clone(),
        vlan: n.vlan.clone(),
        ipv6: ipv6::network(spec, name),
    }
}

fn shape(target: Target, m: &Machine) -> MachineShape {
    if target == Target::External {
        return MachineShape::External;
    }
    let container = match target {
        Target::Docker | Target::Hosted | Target::DockerVm | Target::CloudDocker | Target::Kubernetes => true,
        Target::Hybrid => m.docker.is_some(),
        _ => false,
    };
    match (container, m.docker.is_some(), m.vm.is_some()) {
        (true, true, _) => MachineShape::Container,
        (false, _, true) => MachineShape::Vm,
        _ => MachineShape::Supplied,
    }
}

fn machine(spec: &Spec, target: Target, name: &str, m: &Machine) -> MachinePlan {
    let shape = shape(target, m);
    MachinePlan {
        shape,
        addresses: m.networks.iter().map(|(n, o)| (n.clone(), address(spec, n, *o))).collect(),
        addresses6: ipv6::addresses(spec, m),
        names: std::iter::once(name).chain(m.aliases.iter().map(String::as_str)).map(String::from).collect(),
        routes: routing::routes(spec, name, m).into_iter().map(|(cidr, via)| Route { cidr, via }).collect(),
        routes6: if target == Target::CloudVm { Vec::new() } else { ipv6::routes(spec, m) },
        default_gateway: routing::default_gateway(spec, name, m),
        is_gateway: routing::is_gateway(spec, name),
        offline: routing::offline(spec, name, m),
        ports: published(m),
        volumes: volumes(spec, name, m),
        files: m.files.clone(),
        settings: Settings {
            environment: m.environment.clone(),
            sysctls: m.sysctls.clone(),
            ulimits: m.ulimits.clone(),
        },
        inputs: m.inputs.clone(),
        secrets: m.secrets.iter().map(SecretFile::of).collect(),
        size: m
            .resources
            .map(|r| Size {
                cpus: r.cpus,
                memory_mb: r.memory_mb,
                disk_gb: r.disk_gb,
            })
            .unwrap_or_default(),
        arch: m.arch,
        image: image(spec, shape, name, m),
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
        winrm: m.vm.as_ref().filter(|v| images::is_windows(&v.os)).map(images::winrm),
        external: m.external.clone(),
        host_vars: ansible::host_vars(spec, name),
        outside: IndexMap::new(),
    }
}

fn published(m: &Machine) -> Vec<PublishedPort> {
    m.services
        .iter()
        .filter_map(|s| {
            s.publish.map(|publish| PublishedPort {
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
        .map(|(v, path)| Volume {
            name: v.clone(),
            path: path.clone(),
            key: spec.volume_key(name, v),
            kind: match spec.volume_owner(v) {
                Some(owner) if spec.volumes.contains_key(v) => VolumeKind::Shared {
                    owner: owner.to_string(),
                    users: spec.volume_users(v).into_iter().map(String::from).collect(),
                },
                _ => VolumeKind::Own,
            },
        })
        .collect()
}

fn image(spec: &Spec, shape: MachineShape, name: &str, m: &Machine) -> Option<Image> {
    match shape {
        MachineShape::Container => {
            let d = m.docker.as_ref()?;
            let tag = tag_of(spec, name, d.image.as_deref());
            Some(Image::Container {
                reference: Images::of(spec).pull(&tag),
                build: d.build.as_ref().map(|context| Build {
                    context: context.clone(),
                    dockerfile: d.dockerfile.clone(),
                    target: d.target.clone(),
                    args: d.args.clone(),
                }),
            })
        }
        MachineShape::Vm => m.vm.as_ref().map(|vm| Image::Vm { os: vm.os.clone() }),
        MachineShape::Supplied | MachineShape::External => None,
    }
}

fn controller(spec: &Spec) -> Option<Controller> {
    let needed = !spec.provision.is_empty() || spec.checks.iter().any(isoloom_spec::Check::is_playbook);
    needed.then(|| {
        let r = spec.controller.as_ref().and_then(|c| c.resources.as_ref());
        Controller {
            size: Size {
                cpus: Some(r.and_then(|r| r.cpus).unwrap_or(1)),
                memory_mb: Some(r.and_then(|r| r.memory_mb).unwrap_or(512)),
                disk_gb: r.and_then(|r| r.disk_gb),
            },
        }
    })
}
