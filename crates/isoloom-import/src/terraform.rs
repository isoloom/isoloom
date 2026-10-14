//! Drafts an `isoloom.yml` from Terraform configuration (the `.tf` files of one folder), read
//! statically: `variable` defaults and `locals` are evaluated, `for_each` and `count` expanded,
//! and references between resources followed (a VM to its network interface to its subnet).
//! VMs and subnets of AWS, Azure, Google Cloud and Proxmox (bpg) become machines and networks;
//! every other resource type gets the coverage table's verdict as a note.

mod eval;
mod placement;

use std::collections::BTreeSet;
use std::net::Ipv4Addr;

use indexmap::IndexMap;
use serde_json::Value as Json;

use super::{Draft, NoteKind, Notes, label};
use crate::coverage::Support;
use crate::validate::{Cidr, PRIVATE};
use eval::{instances, resolve};
use placement::Placement;

/// One resource instance, after evaluation: its type, name (with its key when expanded), and
/// attributes (JSON; unresolved expressions as their text, e.g. `aws_subnet.lab.id`).
#[derive(Debug, Clone)]
struct Instance {
    kind: String,
    name: String,
    key: Option<String>,
    attrs: Json,
}

impl Instance {
    /// How other resources refer to it: `aws_subnet.lab`, `aws_instance.vm["dc01"]`, `x.y[0]`.
    fn address(&self) -> String {
        match &self.key {
            Some(k) if k.parse::<usize>().is_ok() => format!("{}.{}[{k}]", self.kind, self.name),
            Some(k) => format!("{}.{}[\"{k}\"]", self.kind, self.name),
            None => format!("{}.{}", self.kind, self.name),
        }
    }
    fn get(&self, path: &[&str]) -> Option<&Json> {
        let mut v = &self.attrs;
        for p in path {
            v = match v {
                Json::Array(a) => a.first()?.get(*p)?,
                other => other.get(*p)?,
            };
        }
        Some(v).filter(|v| !v.is_null())
    }
    fn str(&self, path: &[&str]) -> Option<String> {
        match self.get(path)? {
            Json::String(s) => Some(s.clone()),
            Json::Number(n) => Some(n.to_string()),
            Json::Array(a) => a.first().and_then(|x| x.as_str()).map(String::from),
            _ => None,
        }
    }
}

/// An address and its network block, from `10.0.1.10/24` or a bare address in a known subnet.
fn parse_ip(s: &str) -> Option<(Ipv4Addr, Option<u8>)> {
    let (ip, len) = match s.split_once('/') {
        Some((ip, l)) => (ip, l.parse().ok()),
        None => (s, None),
    };
    Some((ip.trim().parse().ok()?, len))
}

struct Machine {
    name: String,
    os: &'static str,
    addresses: Vec<Ipv4Addr>,
    cpus: Option<u32>,
    memory_mb: Option<u32>,
}

/// CPUs and memory of common cloud sizes (the rest: a note).
// A lookup table: sizes with the same shape stay on their own cloud's line, not merged.
#[allow(clippy::match_same_arms)]
fn size(instance_type: &str) -> Option<(u32, u32)> {
    Some(match instance_type {
        "t2.micro" | "t3.micro" | "t3a.micro" => (2, 1024),
        "t2.small" | "t3.small" | "t3a.small" => (2, 2048),
        "t2.medium" | "t3.medium" | "t3a.medium" => (2, 4096),
        "t2.large" | "t3.large" | "t3a.large" | "m5.large" | "m6i.large" => (2, 8192),
        "t2.xlarge" | "t3.xlarge" | "m5.xlarge" | "m6i.xlarge" => (4, 16384),
        "Standard_B1s" => (1, 1024),
        "Standard_B2s" => (2, 4096),
        "Standard_B2ms" | "Standard_D2s_v3" | "Standard_D2s_v5" => (2, 8192),
        "Standard_D4s_v3" | "Standard_D4s_v5" => (4, 16384),
        "e2-small" => (2, 2048),
        "e2-medium" => (2, 4096),
        "e2-standard-2" => (2, 8192),
        "e2-standard-4" => (4, 16384),
        _ => return None,
    })
}

/// An OS name from what a resource says about its image.
fn os_from(hint: &str) -> &'static str {
    let l = hint.to_ascii_lowercase();
    if l.contains("windows") || l.contains("2019-datacenter") || l.contains("2022-datacenter") {
        if l.contains("2022") { "windows-server-2022" } else { "windows-server-2019" }
    } else if l.contains("ubuntu") {
        "ubuntu-24.04"
    } else if l.contains("kali") {
        "kali"
    } else {
        "debian-12"
    }
}

/// Drafts a spec from the `.tf` files of a folder (`files`: name and text).
pub fn draft(files: &[(String, String)], fallback_name: &str, source: &str) -> Result<Draft, crate::ImportError> {
    let mut notes = Notes::default();
    let mut bodies = Vec::new();
    for (name, text) in files {
        bodies.push(hcl::parse(text).map_err(|source| crate::ImportError::Hcl { file: name.clone(), source })?);
    }
    let all = instances(&bodies, &mut notes);
    let nets = subnets(&all, &mut notes);
    let machines: Vec<Machine> = all.iter().filter_map(|i| machine(i, &all, &mut notes)).collect();
    if machines.is_empty() {
        return Err(crate::ImportError::NoTerraformVm);
    }
    other_resources(&all, &mut notes);
    let placement = Placement::new(&machines, &nets);
    Ok(notes.draft(placement.write(&machines, &label(fallback_name), source)))
}

/// The block a subnet resource declares, for the resource types whose subnets become networks.
// Outer `None`: not a subnet resource; inner `None`: a subnet whose block isn't a literal.
#[allow(clippy::option_option)]
fn subnet_block(i: &Instance) -> Option<Option<String>> {
    Some(match i.kind.as_str() {
        "aws_subnet" | "oci_core_subnet" => i.str(&["cidr_block"]),
        "azurerm_subnet" => i.str(&["address_prefixes"]),
        "google_compute_subnetwork" => i.str(&["ip_cidr_range"]),
        "digitalocean_vpc" => i.str(&["ip_range"]),
        "linode_vpc_subnet" => i.str(&["ipv4"]),
        _ => return None,
    })
}

/// Networks: subnets with a private block, by address (`aws_subnet.lab`), with their name.
fn subnets(all: &[Instance], notes: &mut Notes) -> IndexMap<String, (Cidr, String)> {
    let mut nets = IndexMap::new();
    for i in all {
        let Some(block) = subnet_block(i) else {
            continue;
        };
        let at = format!("resource {}", i.address());
        match block.as_deref().and_then(Cidr::parse) {
            Some(c) if PRIVATE.iter().any(|r| r.contains(c)) => {
                // Isoloom networks are /24 to /29: a bigger subnet keeps its first /24.
                let kept = if c.len < 24 { Cidr { base: c.base, len: 24 } } else { c };
                nets.insert(i.address(), (kept, label(&i.key.clone().unwrap_or_else(|| i.name.clone()))));
                if c.len < 24 {
                    let text = format!("{} is bigger than a /24: kept as {}/24", block.unwrap_or_default(), Ipv4Addr::from(kept.base));
                    notes.add(at, NoteKind::Changed, text);
                }
            }
            _ => notes.add(
                at,
                NoteKind::Changed,
                format!("subnet `{}` couldn't be read as a private block", block.unwrap_or_default()),
            ),
        }
    }
    nets
}

/// What a VM resource says about its machine, before the machine is sized and placed.
#[derive(Default)]
struct VmFacts {
    /// Text that names the OS (an image, a tag, the resource's name...).
    os_hint: String,
    /// The provider's instance size (`t3.large`), when it has sizes.
    size: Option<String>,
    cpus: Option<u32>,
    memory_mb: Option<u32>,
    /// Its private addresses, directly or through its network interfaces.
    ips: Vec<String>,
}

/// The facts of a VM resource, or None when the resource isn't a VM.
fn vm_facts(i: &Instance, all: &[Instance]) -> Option<VmFacts> {
    match i.kind.as_str() {
        "aws_instance" => Some(aws_facts(i, all)),
        "azurerm_linux_virtual_machine" | "azurerm_windows_virtual_machine" => Some(azure_facts(i, all)),
        "google_compute_instance" => Some(VmFacts {
            os_hint: i.str(&["boot_disk", "initialize_params", "image"]).unwrap_or_default(),
            size: i.str(&["machine_type"]),
            ips: i.str(&["network_interface", "network_ip"]).into_iter().collect(),
            ..VmFacts::default()
        }),
        "proxmox_virtual_environment_vm" | "proxmox_vm" => Some(proxmox_facts(i)),
        _ => None,
    }
}

fn aws_facts(i: &Instance, all: &[Instance]) -> VmFacts {
    let mut ips: Vec<String> = i.str(&["private_ip"]).into_iter().collect();
    for nic in i.get(&["network_interface"]).and_then(Json::as_array).into_iter().flatten() {
        if let Some(n) = nic.get("network_interface_id").and_then(Json::as_str).and_then(|r| resolve(all, r))
            && let Some(Json::Array(p)) = n.get(&["private_ips"])
        {
            ips.extend(p.iter().filter_map(|x| x.as_str().map(String::from)));
        }
    }
    let os_hint = [i.str(&["ami"]), i.str(&["tags", "OS"]), Some(i.name.clone())]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    VmFacts {
        os_hint,
        size: i.str(&["instance_type"]),
        ips,
        ..VmFacts::default()
    }
}

fn azure_facts(i: &Instance, all: &[Instance]) -> VmFacts {
    let mut ips = Vec::new();
    if let Some(Json::Array(ids)) = i.get(&["network_interface_ids"]) {
        for id in ids.iter().filter_map(Json::as_str) {
            if let Some(n) = resolve(all, id) {
                ips.extend(n.str(&["ip_configuration", "private_ip_address"]));
            }
        }
    }
    let os_hint = if i.kind.contains("windows") {
        "windows".to_string()
    } else {
        i.str(&["source_image_reference", "offer"]).unwrap_or_default()
    };
    VmFacts {
        os_hint,
        size: i.str(&["size"]),
        ips,
        ..VmFacts::default()
    }
}

fn proxmox_facts(i: &Instance) -> VmFacts {
    let os = [i.str(&["operating_system", "type"]), Some(i.name.clone()), i.str(&["name"])]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    VmFacts {
        os_hint: if os.contains("win") { "windows".into() } else { os },
        size: None,
        cpus: i.get(&["cpu", "cores"]).and_then(Json::as_u64).and_then(|n| u32::try_from(n).ok()),
        memory_mb: i.get(&["memory", "dedicated"]).and_then(Json::as_u64).and_then(|n| u32::try_from(n).ok()),
        ips: i.str(&["initialization", "ip_config", "ipv4", "address"]).into_iter().collect(),
    }
}

/// CPUs and memory of an instance size, with a note when the size isn't a known one.
fn sized(at: &str, size_hint: Option<&str>, notes: &mut Notes) -> (Option<u32>, Option<u32>) {
    match size_hint.map(|s| (s, size(s))) {
        Some((_, Some((c, m)))) => (Some(c), Some(m)),
        Some((s, None)) => {
            notes.add(format!("{at}.size"), NoteKind::Changed, format!("`{s}`: set `resources` by hand"));
            (None, None)
        }
        None => (None, None),
    }
}

/// A machine from a VM resource, and the addresses it has; None for every other resource.
fn machine(i: &Instance, all: &[Instance], notes: &mut Notes) -> Option<Machine> {
    let mut facts = vm_facts(i, all)?;
    let at = format!("resource {}", i.address());
    facts.ips.retain(|ip| !ip.is_empty());
    let (sz_cpus, sz_mem) = sized(&at, facts.size.as_deref(), notes);
    let addresses: Vec<Ipv4Addr> = facts.ips.iter().filter_map(|s| parse_ip(s).map(|(ip, _)| ip)).collect();
    if addresses.is_empty() {
        notes.add(
            at.clone(),
            NoteKind::Changed,
            "no fixed private address (DHCP or a public one): put on a network at the next free address",
        );
    }
    let raw = i.key.clone().unwrap_or_else(|| i.str(&["name"]).unwrap_or_else(|| i.name.clone()));
    let os = os_from(&facts.os_hint);
    notes.add(at, NoteKind::Changed, format!("OS read as {os}: check it"));
    Some(Machine {
        name: label(&raw),
        os,
        addresses,
        cpus: facts.cpus.or(sz_cpus),
        memory_mb: facts.memory_mb.or(sz_mem),
    })
}

/// The resource types read above, as machines, networks or a machine's network interfaces.
const HANDLED: [&str; 11] = [
    "aws_instance",
    "aws_subnet",
    "aws_network_interface",
    "azurerm_linux_virtual_machine",
    "azurerm_windows_virtual_machine",
    "azurerm_subnet",
    "azurerm_network_interface",
    "google_compute_instance",
    "google_compute_subnetwork",
    "proxmox_virtual_environment_vm",
    "proxmox_vm",
];

/// Every other resource type, once each: the coverage table's verdict.
fn other_resources(all: &[Instance], notes: &mut Notes) {
    let cov = crate::coverage::terraform::formats();
    let mut seen = BTreeSet::new();
    for i in all {
        if HANDLED.contains(&i.kind.as_str()) || !seen.insert(i.kind.clone()) {
            continue;
        }
        let row = format!("resource {}", i.kind);
        let support = cov.iter().flat_map(|f| f.rows.iter()).find(|(k, _)| k == &row).map(|(_, s)| *s);
        let (kind, text) = verdict(support);
        notes.add(row, kind, text);
    }
}

/// A resource type's coverage row as a note kind and text.
fn verdict(support: Option<Support>) -> (NoteKind, String) {
    match support {
        Some(Support::NotPortable { why }) => (NoteKind::ByDesign, why.to_string()),
        Some(Support::Tooling { note }) => (NoteKind::Tooling, note.to_string()),
        // Networks, firewalls, addresses, disks: what Isoloom builds itself on that cloud.
        Some(Support::Planned { .. } | Support::Emitted { .. } | Support::Partial { .. }) => (
            NoteKind::Equivalent,
            "part of what Isoloom builds itself for the environment (networks, firewalls, addresses, disks)".into(),
        ),
        Some(s) => (NoteKind::NotYet, s.note()),
        None => (NoteKind::Tooling, "not a resource Isoloom knows: left out".into()),
    }
}
