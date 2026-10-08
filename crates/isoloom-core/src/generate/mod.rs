//! Generators: turn a spec into the files a target runs. Each generator is a pure function
//! of the spec (no file system access), so `isoloom check` can regenerate in memory and
//! compare with what's committed. Output lives under `.isoloom/<target>/` in the project.

mod appliances;
/// The network appliances' management network (see `docker.appliance`).
pub const APPLIANCE_MGMT_CIDR: &str = appliances::MGMT_CIDR;
mod cloud_docker;
mod cloud_vm;
mod cloud_vm_others;
mod docker;
pub use docker::{ENVIRONMENT_LABEL, MANAGED_LABEL, StartPlan, exec_runner, leaf_jobs, start_commands, start_plan};
mod docker_vm;
mod external;
mod hybrid;
mod kubernetes;
mod proxmox;
pub mod resolved;
mod router;
mod trunks;
mod vagrant;
pub use vagrant::qemu_refusal;

use std::fmt;
use std::net::Ipv4Addr;

use indexmap::IndexMap;

use crate::model::{Spec, Target};
use crate::targets::effective;
use crate::validate::Cidr;

/// The name of the router Isoloom adds (a Compose service, a VM) when `reach` rules need one.
pub fn router_name() -> &'static str {
    router::NAME
}

/// Where generated files go, relative to the project folder.
pub const OUTPUT_DIR: &str = ".isoloom";

/// One generated file: its path relative to the project folder, and its contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerateError {
    /// The spec's implementations don't allow this target (see `isoloom targets`).
    NotPossible(Target),
    /// The target is possible, but this generator doesn't support a feature the spec uses yet.
    Unsupported { target: Target, what: String },
    /// No generator exists for this target yet.
    NoGenerator(Target),
}

impl fmt::Display for GenerateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenerateError::NotPossible(t) => write!(f, "{}: not possible for this spec (see `isoloom targets`)", t.id()),
            GenerateError::Unsupported { target, what } => write!(f, "{}: {what}", target.id()),
            GenerateError::NoGenerator(t) => write!(f, "{}: no generator yet", t.id()),
        }
    }
}

impl std::error::Error for GenerateError {}

/// Targets that have a generator today.
pub const GENERATED_TARGETS: &[Target] = &[
    Target::Docker,
    Target::Hosted,
    Target::DockerVm,
    Target::CloudDocker,
    Target::Kubernetes,
    Target::Hybrid,
    Target::Vagrant,
    Target::Proxmox,
    Target::CloudVm,
    Target::External,
];

/// The files for one target.
pub fn generate(spec: &Spec, target: Target) -> Result<Vec<GeneratedFile>, GenerateError> {
    let mut files = target_files(spec, target)?;
    // The resolved snapshot goes with every target (the same file each time).
    files.push(resolved::file(spec));
    Ok(files)
}

/// The files for one target as instance `n` of the spec (see [`crate::instance`]): names
/// suffixed, Docker blocks and published ports moved, everything under `.isoloom-<n>/`.
pub fn generate_instance(spec: &Spec, target: Target, n: u8) -> Result<Vec<GeneratedFile>, GenerateError> {
    let applied = crate::instance::apply(spec, n).map_err(|what| GenerateError::Unsupported { target, what })?;
    let mut files = target_files(&applied, target)?;
    files.push(resolved::file_with(&applied, Some(n)));
    Ok(files
        .into_iter()
        .map(|f| {
            let (path, contents) = crate::instance::relocate(&f.path, &f.contents, n);
            GeneratedFile { path, contents }
        })
        .collect())
}

/// A target's own files, without the snapshot (which asks every generator whether it would
/// refuse the spec, so it can't be part of what a generator produces).
fn target_files(spec: &Spec, target: Target) -> Result<Vec<GeneratedFile>, GenerateError> {
    if !effective(spec).contains(&target) {
        return Err(GenerateError::NotPossible(target));
    }
    // Network appliances run on Docker Compose only (their wiring is Docker's).
    if !matches!(target, Target::Docker | Target::Hosted | Target::CloudDocker | Target::DockerVm)
        && let Some((name, _, _)) = appliances::appliances(spec).first()
    {
        return Err(GenerateError::Unsupported {
            target,
            what: format!("machine `{name}` is a network appliance (`docker.appliance`), which runs on the Docker targets"),
        });
    }
    match target {
        // A hosting service runs the same Compose file.
        Target::Docker | Target::Hosted => docker::generate(&on_docker(spec), spec),
        // Docker on one VM runs the Compose file: both are generated.
        Target::CloudDocker => {
            let mut files = docker::generate(&on_docker(spec), spec)?;
            files.extend(cloud_docker::generate(spec)?);
            Ok(files)
        }
        Target::DockerVm => {
            let mut files = docker::generate(&on_docker(spec), spec)?;
            files.extend(docker_vm::generate(spec)?);
            Ok(files)
        }
        Target::Kubernetes => kubernetes::generate(spec),
        Target::Hybrid => hybrid::generate(spec),
        Target::Vagrant => vagrant::generate(spec),
        Target::Proxmox => proxmox::generate(spec),
        Target::CloudVm => cloud_vm::generate(spec),
        Target::External => external::generate(spec),
    }
}

/// Every target the spec allows and that has a generator: the files, and why the others
/// weren't generated.
pub fn generate_all(spec: &Spec) -> (Vec<GeneratedFile>, Vec<GenerateError>) {
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for target in effective(spec) {
        if !GENERATED_TARGETS.contains(&target) {
            skipped.push(GenerateError::NoGenerator(target));
            continue;
        }
        match generate(spec, target) {
            Ok(f) => {
                // Outputs share files (docker-vm runs the Compose file): each once.
                for file in f {
                    if !files.iter().any(|x: &GeneratedFile| x.path == file.path) {
                        files.push(file);
                    }
                }
            }
            Err(e) => skipped.push(e),
        }
    }
    (files, skipped)
}

/// The first lines of every generated file (`comment` is the target language's marker).
/// The first non-access machine on arm64, if any: the cloud and Proxmox targets refuse these
/// for now (they still assume x86-64 images and instance types).
fn arm64_machine(spec: &Spec) -> Option<&str> {
    spec.machines
        .iter()
        // Only machines the cloud actually builds: an access machine with no implementation isn't
        // instantiated, so its arch is moot, but one that declares a container or VM is built and
        // must not be silently given an x86 image.
        .filter(|(_, m)| m.docker.is_some() || m.vm.is_some())
        .find(|(_, m)| m.arch == crate::model::Arch::Arm64)
        .map(|(n, _)| n.as_str())
}

fn header(comment: &str) -> String {
    format!(
        "{comment} Generated by isoloom from isoloom.yml. Don't edit: change isoloom.yml and run\n{comment} `isoloom generate`. `isoloom check` fails when this file is out of date.\n"
    )
}

/// A machine's address on a network (validated specs only).
fn address(spec: &Spec, network: &str, octet: u8) -> Ipv4Addr {
    let cidr = Cidr::parse(&spec.networks[network].cidr).expect("validated cidr");
    cidr.host(octet)
        .or_else(|| (octet == cidr.gateway_octet()).then(|| cidr.gateway()))
        .expect("validated address")
}

/// Dotted netmask of a network.
fn netmask(spec: &Spec, network: &str) -> Ipv4Addr {
    let cidr = Cidr::parse(&spec.networks[network].cidr).expect("validated cidr");
    Ipv4Addr::from(if cidr.len == 0 { 0 } else { u32::MAX << (32 - cidr.len) })
}

/// Why `target`'s generator would refuse this spec, if it would: the thing it doesn't support
/// yet (a Windows machine on Proxmox, environment-level provisioning on Docker, ...). `None`
/// when `generate` produces the target. Lets `isoloom targets` tell the truth: a target can be
/// possible by its machines' editions and still not be generated.
pub fn refusal(spec: &Spec, target: Target) -> Option<String> {
    match target_files(spec, target) {
        Err(GenerateError::Unsupported { what, .. }) => Some(what),
        _ => None,
    }
}

/// Features no generator supports yet, shared by both (none today: kept as the place to
/// refuse a spec feature before a generator learns it).
fn common_unsupported(spec: &Spec, target: Target) -> Result<(), GenerateError> {
    if target == Target::Docker && !spec.provision.is_empty() {
        return Err(GenerateError::Unsupported {
            target,
            what: "environment-level provisioning (`provision:`) runs on VM targets for now".into(),
        });
    }
    if target == Target::Docker
        && let Some(what) = container_checks_unsupported(spec)
    {
        return Err(GenerateError::Unsupported { target, what });
    }
    Ok(())
}

/// What the container outputs (Compose, Kubernetes) can't run as checks yet: Ansible playbooks
/// (they run from the VM targets' controller) and `exec` from a machine with no container of its
/// own (`exec` checks are piped into the machine: `docker compose exec`, `kubectl exec`).
pub(crate) fn container_checks_unsupported(spec: &Spec) -> Option<String> {
    if spec.checks.iter().any(|c| c.is_playbook()) {
        return Some("Ansible checks (.yml) run on VM targets for now".into());
    }
    for c in crate::checks::plan(spec) {
        if !matches!(c.probe, crate::checks::Probe::Exec { .. }) {
            continue;
        }
        let in_container = match &c.position {
            crate::checks::Position::Machine(m) => spec.machines.get(m).is_some_and(|m| m.docker.is_some() && !m.supplied),
            _ => false,
        };
        if !in_container {
            return Some(format!(
                "`exec` check \"{}\" needs `from:` a machine with its own container (it runs inside it)",
                c.name
            ));
        }
    }
    None
}

/// A machine's names for a hosts line: its own, then its `aliases`.
pub(crate) fn names_of(spec: &Spec, name: &str) -> String {
    std::iter::once(name)
        .chain(spec.machines.get(name).map(|m| m.aliases.iter().map(String::as_str)).into_iter().flatten())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Machines in start order: each after the machines it depends on (validated: no cycles).
fn start_order(spec: &Spec) -> Vec<&str> {
    let mut order: Vec<&str> = Vec::new();
    fn visit<'a>(spec: &'a Spec, name: &'a str, order: &mut Vec<&'a str>) {
        if order.contains(&name) {
            return;
        }
        for dep in crate::validate::starts_after(spec, name, &spec.machines[name]) {
            visit(spec, dep, order);
        }
        order.push(name);
    }
    for name in spec.machines.keys() {
        visit(spec, name, &mut order);
    }
    order
}

/// The address `from` should use to reach `to`: on a network they share if any, else
/// `to`'s first address.
fn address_for(spec: &Spec, from: &str, to: &str) -> Ipv4Addr {
    let a = &spec.machines[from];
    let b = &spec.machines[to];
    let (net, octet) = b
        .networks
        .iter()
        .find(|(n, _)| a.networks.contains_key(*n))
        .or_else(|| b.networks.first())
        .expect("validated: machines have a network");
    address(spec, net, *octet)
}

/// The host's own address on a network (Docker's bridge): the gateway address, unless a
/// machine is the network's gateway; then the router's address, unused on such networks.
fn host_address(spec: &Spec, network: &str) -> Ipv4Addr {
    let cidr = Cidr::parse(&spec.networks[network].cidr).expect("validated cidr");
    if spec.networks[network].gateway.is_some() {
        cidr.router()
    } else {
        cidr.gateway()
    }
}

/// Each network's block on the Docker target: its `docker.cidr` when set; its `cidr` when it's
/// inside 10.0.0.0/8; otherwise moved there automatically, keeping its size (so each machine
/// keeps its last octet): 192.168.X.0 -> 10.192.X.0, 172.N.X.0 -> 10.N.X.0, or the next free
/// block when that one is taken. Validated specs only.
pub fn docker_cidrs(spec: &Spec) -> Vec<(String, Cidr)> {
    use crate::validate::DOCKER_BLOCK;
    let parse = |s: &str| Cidr::parse(s).expect("validated cidr");
    let mut fixed: Vec<(String, Cidr)> = Vec::new();
    let mut moved: Vec<(String, Cidr)> = Vec::new();
    for (name, n) in &spec.networks {
        let c = parse(&n.cidr);
        match &n.docker {
            Some(d) => fixed.push((name.clone(), parse(&d.cidr))),
            None if DOCKER_BLOCK.contains(c) => fixed.push((name.clone(), c)),
            None => moved.push((name.clone(), c)),
        }
    }
    let mut taken: Vec<Cidr> = fixed.iter().map(|(_, c)| *c).collect();
    let mut out: IndexMap<String, Cidr> = fixed.into_iter().collect();
    for (name, c) in moved {
        let [a, b, x, y] = std::net::Ipv4Addr::from(c.base).octets();
        let first = if a == 192 {
            u32::from_be_bytes([10, 192, x, y])
        } else {
            u32::from_be_bytes([10, b, x, y])
        };
        let size = 1u32 << (32 - c.len);
        // The mapped block, else the next free one of the same size scanning 10.240.0.0 upward
        // within 10.0.0.0/8. A bounded, checked successor sequence: it can't overflow, and it
        // ends at the top of the block so `find` terminates (and `expect` fires only on genuine
        // exhaustion, which needs thousands of auto-moved networks).
        let scan = std::iter::successors(Some(0x0af0_0000u32), move |&b| b.checked_add(size).filter(|&n| n <= 0x0aff_ffff));
        let candidates = std::iter::once(first).chain(scan);
        let pick = candidates
            .map(|base| Cidr { base, len: c.len })
            .find(|cand| DOCKER_BLOCK.contains(*cand) && !taken.iter().any(|t| t.overlaps(*cand)))
            .expect("10.0.0.0/8 has room for the Docker networks");
        taken.push(pick);
        out.insert(name, pick);
    }
    spec.networks.keys().map(|n| (n.clone(), out[n])).collect()
}

/// The spec as the Docker target lays it out: every network on its Docker block.
fn on_docker(spec: &Spec) -> Spec {
    let mut s = spec.clone();
    for (name, c) in docker_cidrs(spec) {
        let n = s.networks.get_mut(&name).expect("same networks");
        n.cidr = format!("{}/{}", Ipv4Addr::from(c.base), c.len);
    }
    s
}
