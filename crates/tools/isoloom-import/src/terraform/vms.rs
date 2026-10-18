//! The VM resources that become the draft's machines: what each says about its OS, its size and
//! its private addresses (directly, or through its network interfaces).

use std::net::Ipv4Addr;

use serde_json::Value as Json;

use super::eval::resolve;
use super::instance::Instance;
use crate::draft::{NoteKind, Notes};
use crate::naming::label;

/// A machine drafted from a VM resource, before it is placed on the networks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Machine {
    pub(super) name: String,
    pub(super) os: &'static str,
    pub(super) addresses: Vec<Ipv4Addr>,
    pub(super) cpus: Option<u32>,
    pub(super) memory_mb: Option<u32>,
}

/// The resource types read as machines, or as a machine's network interfaces.
const VM_TYPES: [&str; 8] = [
    "aws_instance",
    "aws_network_interface",
    "azurerm_linux_virtual_machine",
    "azurerm_windows_virtual_machine",
    "azurerm_network_interface",
    "google_compute_instance",
    "proxmox_virtual_environment_vm",
    "proxmox_vm",
];

/// CPUs and memory (MB) of common cloud sizes.
const SIZES: [(&str, u32, u32); 29] = [
    ("t2.micro", 2, 1024),
    ("t3.micro", 2, 1024),
    ("t3a.micro", 2, 1024),
    ("t2.small", 2, 2048),
    ("t3.small", 2, 2048),
    ("t3a.small", 2, 2048),
    ("t2.medium", 2, 4096),
    ("t3.medium", 2, 4096),
    ("t3a.medium", 2, 4096),
    ("t2.large", 2, 8192),
    ("t3.large", 2, 8192),
    ("t3a.large", 2, 8192),
    ("m5.large", 2, 8192),
    ("m6i.large", 2, 8192),
    ("t2.xlarge", 4, 16384),
    ("t3.xlarge", 4, 16384),
    ("m5.xlarge", 4, 16384),
    ("m6i.xlarge", 4, 16384),
    ("Standard_B1s", 1, 1024),
    ("Standard_B2s", 2, 4096),
    ("Standard_B2ms", 2, 8192),
    ("Standard_D2s_v3", 2, 8192),
    ("Standard_D2s_v5", 2, 8192),
    ("Standard_D4s_v3", 4, 16384),
    ("Standard_D4s_v5", 4, 16384),
    ("e2-small", 2, 2048),
    ("e2-medium", 2, 4096),
    ("e2-standard-2", 2, 8192),
    ("e2-standard-4", 4, 16384),
];

/// Whether the import reads resources of this type, as machines or their network interfaces.
pub(super) fn reads(kind: &str) -> bool {
    VM_TYPES.contains(&kind)
}

/// CPUs and memory of an instance size, when it is a common one.
fn size(instance_type: &str) -> Option<(u32, u32)> {
    SIZES
        .iter()
        .find(|(name, ..)| *name == instance_type)
        .map(|&(_, cpus, memory_mb)| (cpus, memory_mb))
}

/// An OS name from what a resource says about its image.
fn os_from(hint: &str) -> &'static str {
    let l = hint.to_ascii_lowercase();
    if l.contains("windows") || l.contains("2019-datacenter") || l.contains("2022-datacenter") {
        if l.contains("2022") { "windows-server@2022" } else { "windows-server@2019" }
    } else if l.contains("ubuntu") {
        "ubuntu@24.04"
    } else if l.contains("kali") {
        "kali"
    } else {
        "debian@12"
    }
}

/// An address from `10.0.1.10/24` or a bare `10.0.1.10`.
fn address(text: &str) -> Option<Ipv4Addr> {
    text.split_once('/').map_or(text, |(ip, _)| ip).trim().parse().ok()
}

/// What a VM resource says about its machine, before the machine is sized and placed.
#[derive(Debug, Default)]
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
        "windows".to_owned()
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
pub(super) fn machine(i: &Instance, all: &[Instance], notes: &mut Notes) -> Option<Machine> {
    let mut facts = vm_facts(i, all)?;
    let at = format!("resource {}", i.address());
    facts.ips.retain(|ip| !ip.is_empty());
    let (sz_cpus, sz_mem) = sized(&at, facts.size.as_deref(), notes);
    let addresses: Vec<Ipv4Addr> = facts.ips.iter().filter_map(|s| address(s)).collect();
    if addresses.is_empty() {
        notes.add(
            at.clone(),
            NoteKind::Changed,
            "no fixed private address (DHCP or a public one): put on a network at the next free address",
        );
    }
    let os = os_from(&facts.os_hint);
    notes.add(at, NoteKind::Changed, format!("OS read as {os}: check it"));
    Some(Machine {
        name: label(&i.name_or(i.str(&["name"]))),
        os,
        addresses,
        cpus: facts.cpus.or(sz_cpus),
        memory_mb: facts.memory_mb.or(sz_mem),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_common_size_has_its_cpus_and_memory() {
        assert_eq!(size("t3.large"), Some((2, 8192)));
        assert_eq!(size("e2-standard-4"), Some((4, 16384)));
        assert_eq!(size("x9.huge"), None);
    }

    #[test]
    fn the_os_is_read_from_the_image_it_names() {
        assert_eq!(os_from("ami-0windows2022 windows"), "windows-server@2022");
        assert_eq!(os_from("2019-Datacenter"), "windows-server@2019");
        assert_eq!(os_from("ubuntu-2404-lts"), "ubuntu@24.04");
        assert_eq!(os_from("anything"), "debian@12");
    }

    #[test]
    fn an_address_is_read_with_or_without_its_block() {
        assert_eq!(address("10.0.1.10/24"), Some(Ipv4Addr::new(10, 0, 1, 10)));
        assert_eq!(address(" 10.0.1.11"), Some(Ipv4Addr::new(10, 0, 1, 11)));
        assert_eq!(address("dhcp"), None);
    }
}
