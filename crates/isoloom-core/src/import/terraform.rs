//! Drafts an `isoloom.yml` from Terraform configuration (the `.tf` files of one folder), read
//! statically: `variable` defaults and `locals` are evaluated, `for_each` and `count` expanded,
//! and references between resources followed (a VM to its network interface to its subnet).
//! VMs and subnets of AWS, Azure, Google Cloud and Proxmox (bpg) become machines and networks;
//! every other resource type gets the coverage table's verdict as a note.

use std::fmt::Write;
use std::net::Ipv4Addr;

use hcl::eval::{Context, Evaluate};
use hcl::{Body, Expression, Value};
use indexmap::IndexMap;
use serde_json::{Value as Json, json};

use super::compose::label;
use super::{Draft, Note, NoteKind};
use crate::validate::{Cidr, PRIVATE};

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

fn to_json(v: &Value) -> Json {
    serde_json::to_value(v).unwrap_or(Json::Null)
}

/// An expression as JSON when it's a value, else its text (an unresolved reference).
fn expr_json(e: &Expression) -> Json {
    match e {
        Expression::Null => Json::Null,
        Expression::Bool(b) => json!(b),
        Expression::Number(n) => serde_json::to_value(n).unwrap_or(Json::Null),
        Expression::String(s) => json!(s),
        Expression::Array(a) => Json::Array(a.iter().map(expr_json).collect()),
        Expression::Object(o) => Json::Object(o.iter().map(|(k, v)| (k.to_string().trim_matches('"').to_string(), expr_json(v))).collect()),
        other => json!(hcl::format::to_string(other).unwrap_or_default()),
    }
}

/// A body's attributes and nested blocks as JSON (blocks of the same name collected in a list).
fn body_json(body: &Body) -> Json {
    let mut out = serde_json::Map::new();
    for a in body.attributes() {
        out.insert(a.key().to_string(), expr_json(a.expr()));
    }
    for b in body.blocks() {
        let entry = out.entry(b.identifier().to_string()).or_insert_with(|| Json::Array(Vec::new()));
        if let Json::Array(list) = entry {
            list.push(body_json(b.body()));
        }
    }
    Json::Object(out)
}

/// One instance of an expanded resource: its key, and the `each` or `count` variable it sees.
type Expansion = (Option<String>, Option<(&'static str, Value)>);

/// Evaluates every resource of the configuration, expanding `for_each` and `count`.
fn instances(bodies: &[Body], notes: &mut Vec<Note>) -> Vec<Instance> {
    // Variables (their defaults) and locals, in order.
    let mut vars = hcl::Map::new();
    for body in bodies {
        for b in body.blocks().filter(|b| b.identifier() == "variable") {
            if let (Some(name), Some(d)) = (b.labels().first(), b.body().attributes().find(|a| a.key() == "default")) {
                if let Ok(v) = d.expr().evaluate(&Context::new()) {
                    vars.insert(name.as_str().to_string(), v);
                }
            }
        }
    }
    let mut ctx = Context::new();
    ctx.declare_var("var", Value::Object(vars));
    let mut locals = hcl::Map::new();
    for body in bodies {
        for b in body.blocks().filter(|b| b.identifier() == "locals") {
            for a in b.body().attributes() {
                let mut c = ctx.clone();
                c.declare_var("local", Value::Object(locals.clone()));
                if let Ok(v) = a.expr().evaluate(&c) {
                    locals.insert(a.key().to_string(), v);
                }
            }
        }
    }
    ctx.declare_var("local", Value::Object(locals));
    ctx.declare_var("path", Value::Object([("module".to_string(), Value::from("."))].into_iter().collect()));

    let mut out = Vec::new();
    for body in bodies {
        for b in body.blocks().filter(|b| b.identifier() == "resource") {
            let (Some(kind), Some(name)) = (b.labels().first(), b.labels().get(1)) else {
                continue;
            };
            let (kind, name) = (kind.as_str().to_string(), name.as_str().to_string());
            let attr = |k: &str| b.body().attributes().find(|a| a.key() == k).map(|a| a.expr().clone());
            // Each instance: (key, `each`/`count` for its context).
            let expansions: Vec<Expansion> = if let Some(fe) = attr("for_each") {
                match fe.evaluate(&ctx) {
                    Ok(Value::Object(m)) => m
                        .into_iter()
                        .map(|(k, v)| {
                            let each: hcl::Map<String, Value> = [("key".to_string(), Value::from(k.clone())), ("value".to_string(), v)].into_iter().collect();
                            (Some(k), Some(("each", Value::Object(each))))
                        })
                        .collect(),
                    Ok(Value::Array(a)) => a
                        .into_iter()
                        .map(|v| {
                            let k = match &v {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            let each: hcl::Map<String, Value> = [("key".to_string(), Value::from(k.clone())), ("value".to_string(), v)].into_iter().collect();
                            (Some(k), Some(("each", Value::Object(each))))
                        })
                        .collect(),
                    _ => {
                        notes.push(Note {
                            at: format!("resource {kind}.{name}"),
                            kind: NoteKind::Changed,
                            text: "its `for_each` couldn't be read without running Terraform: add these machines by hand".into(),
                        });
                        continue;
                    }
                }
            } else if let Some(c) = attr("count") {
                match c.evaluate(&ctx) {
                    Ok(Value::Number(n)) => (0..n.as_u64().unwrap_or(0))
                        .map(|i| {
                            let count: hcl::Map<String, Value> = [("index".to_string(), Value::from(i))].into_iter().collect();
                            (Some(i.to_string()), Some(("count", Value::Object(count))))
                        })
                        .collect(),
                    _ => {
                        notes.push(Note {
                            at: format!("resource {kind}.{name}"),
                            kind: NoteKind::Changed,
                            text: "its `count` couldn't be read without running Terraform".into(),
                        });
                        continue;
                    }
                }
            } else {
                vec![(None, None)]
            };
            for (key, extra) in expansions {
                let mut c = ctx.clone();
                if let Some((n, v)) = extra {
                    c.declare_var(n, v);
                }
                let mut body = b.body().clone();
                let _ = body.evaluate_in_place(&c); // what can't be resolved stays as text
                out.push(Instance {
                    kind: kind.clone(),
                    name: name.clone(),
                    key,
                    attrs: body_json(&body),
                });
            }
        }
    }
    let _ = to_json;
    out
}

/// The instance a reference text points at (`aws_subnet.lab.id`, `aws_network_interface.nic["dc01"].id`).
fn resolve<'a>(all: &'a [Instance], reference: &str) -> Option<&'a Instance> {
    let r = reference.trim().trim_start_matches("${").trim_end_matches('}');
    all.iter().find(|i| {
        let a = i.address();
        r == a || r.starts_with(&format!("{a}.")) || r.replace("\\\"", "\"").starts_with(&format!("{a}."))
    })
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
pub fn draft(files: &[(String, String)], fallback_name: &str, source: &str) -> Result<Draft, String> {
    let mut notes = Vec::new();
    let mut bodies = Vec::new();
    for (name, text) in files {
        bodies.push(hcl::parse(text).map_err(|e| format!("{name}: not valid Terraform: {e}"))?);
    }
    let all = instances(&bodies, &mut notes);

    // Networks: subnets with a private block.
    let mut nets: IndexMap<String, (Cidr, String)> = IndexMap::new(); // address -> (block, name)
    for i in &all {
        let block = match i.kind.as_str() {
            "aws_subnet" => i.str(&["cidr_block"]),
            "azurerm_subnet" => i.str(&["address_prefixes"]),
            "google_compute_subnetwork" => i.str(&["ip_cidr_range"]),
            "digitalocean_vpc" => i.str(&["ip_range"]),
            "linode_vpc_subnet" => i.str(&["ipv4"]),
            "oci_core_subnet" => i.str(&["cidr_block"]),
            _ => continue,
        };
        let at = format!("resource {}", i.address());
        match block.as_deref().and_then(Cidr::parse) {
            Some(c) if PRIVATE.iter().any(|r| r.contains(c)) => {
                // Isoloom networks are /24 to /29: a bigger subnet keeps its first /24.
                let c = if c.len < 24 { Cidr { base: c.base, len: 24 } } else { c };
                nets.insert(i.address(), (c, label(&i.key.clone().unwrap_or_else(|| i.name.clone()))));
                if block.as_deref().and_then(Cidr::parse).is_some_and(|o| o.len < 24) {
                    notes.push(Note {
                        at,
                        kind: NoteKind::Changed,
                        text: format!("{} is bigger than a /24: kept as {}/24", block.unwrap_or_default(), Ipv4Addr::from(c.base)),
                    });
                }
            }
            _ => notes.push(Note {
                at,
                kind: NoteKind::Changed,
                text: format!("subnet `{}` couldn't be read as a private block", block.unwrap_or_default()),
            }),
        }
    }

    // Machines, and the addresses each has (directly, or through its network interfaces).
    let mut machines = Vec::new();
    for i in &all {
        let at = format!("resource {}", i.address());
        let (os_hint, size_hint, cpus, memory, mut ips): (String, Option<String>, Option<u32>, Option<u32>, Vec<String>) = match i.kind.as_str() {
            "aws_instance" => {
                let mut ips: Vec<String> = i.str(&["private_ip"]).into_iter().collect();
                for nic in i.get(&["network_interface"]).and_then(Json::as_array).into_iter().flatten() {
                    if let Some(n) = nic.get("network_interface_id").and_then(Json::as_str).and_then(|r| resolve(&all, r)) {
                        if let Some(Json::Array(p)) = n.get(&["private_ips"]) {
                            ips.extend(p.iter().filter_map(|x| x.as_str().map(String::from)));
                        }
                    }
                }
                let os = [i.str(&["ami"]), i.str(&["tags", "OS"]), Some(i.name.clone())]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ");
                (os, i.str(&["instance_type"]), None, None, ips)
            }
            "azurerm_linux_virtual_machine" | "azurerm_windows_virtual_machine" => {
                let mut ips = Vec::new();
                if let Some(Json::Array(ids)) = i.get(&["network_interface_ids"]) {
                    for id in ids.iter().filter_map(Json::as_str) {
                        if let Some(n) = resolve(&all, id) {
                            ips.extend(n.str(&["ip_configuration", "private_ip_address"]));
                        }
                    }
                }
                let os = if i.kind.contains("windows") {
                    "windows".to_string()
                } else {
                    i.str(&["source_image_reference", "offer"]).unwrap_or_default()
                };
                (os, i.str(&["size"]), None, None, ips)
            }
            "google_compute_instance" => {
                let ips = i.str(&["network_interface", "network_ip"]).into_iter().collect();
                (
                    i.str(&["boot_disk", "initialize_params", "image"]).unwrap_or_default(),
                    i.str(&["machine_type"]),
                    None,
                    None,
                    ips,
                )
            }
            "proxmox_virtual_environment_vm" | "proxmox_vm" => {
                let ips = i.str(&["initialization", "ip_config", "ipv4", "address"]).into_iter().collect();
                let cpus = i.get(&["cpu", "cores"]).and_then(Json::as_u64).map(|n| n as u32);
                let mem = i.get(&["memory", "dedicated"]).and_then(Json::as_u64).map(|n| n as u32);
                let os = [i.str(&["operating_system", "type"]), Some(i.name.clone()), i.str(&["name"])]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ");
                let os = if os.contains("win") { "windows".into() } else { os };
                (os, None, cpus, mem, ips)
            }
            _ => continue,
        };
        ips.retain(|ip| !ip.is_empty());
        let (sz_cpus, sz_mem) = match size_hint.as_deref().map(|s| (s, size(s))) {
            Some((_, Some((c, m)))) => (Some(c), Some(m)),
            Some((s, None)) => {
                notes.push(Note {
                    at: format!("{at}.size"),
                    kind: NoteKind::Changed,
                    text: format!("`{s}`: set `resources` by hand"),
                });
                (None, None)
            }
            None => (None, None),
        };
        let addresses: Vec<Ipv4Addr> = ips.iter().filter_map(|s| parse_ip(s).map(|(ip, _)| ip)).collect();
        if addresses.is_empty() {
            notes.push(Note {
                at: at.clone(),
                kind: NoteKind::Changed,
                text: "no fixed private address (DHCP or a public one): put on a network at the next free address".into(),
            });
        }
        let raw = i.key.clone().unwrap_or_else(|| i.str(&["name"]).unwrap_or_else(|| i.name.clone()));
        let name = label(&raw);
        machines.push(Machine {
            name,
            os: os_from(&os_hint),
            addresses,
            cpus: cpus.or(sz_cpus),
            memory_mb: memory.or(sz_mem),
        });
        notes.push(Note {
            at,
            kind: NoteKind::Changed,
            text: format!("OS read as {}: check it", os_from(&os_hint)),
        });
    }
    if machines.is_empty() {
        return Err("no VM resource found (aws_instance, azurerm_*_virtual_machine, google_compute_instance, proxmox_virtual_environment_vm)".into());
    }

    // Every other resource type: the coverage table's verdict.
    let cov = crate::coverage::terraform::formats();
    let handled = [
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
    let mut seen = std::collections::BTreeSet::new();
    for i in &all {
        if handled.contains(&i.kind.as_str()) || !seen.insert(i.kind.clone()) {
            continue;
        }
        let verdict = cov
            .iter()
            .flat_map(|f| f.rows.iter())
            .find(|(k, _)| k == &format!("resource {}", i.kind))
            .map(|(_, s)| *s);
        let (kind, text) = match verdict {
            Some(crate::coverage::Support::NotPortable { why }) => (NoteKind::ByDesign, why.to_string()),
            Some(crate::coverage::Support::Tooling { note }) => (NoteKind::Tooling, note.to_string()),
            // Networks, firewalls, addresses, disks: what Isoloom builds itself on that cloud.
            Some(crate::coverage::Support::Planned { .. } | crate::coverage::Support::Emitted { .. } | crate::coverage::Support::Partial { .. }) => (
                NoteKind::Equivalent,
                "part of what Isoloom builds itself for the environment (networks, firewalls, addresses, disks)".into(),
            ),
            Some(s) => (NoteKind::NotYet, s.note()),
            None => (NoteKind::Tooling, "not a resource Isoloom knows: left out".into()),
        };
        notes.push(Note {
            at: format!("resource {}", i.kind),
            kind,
            text,
        });
    }

    // Each machine joins the networks its addresses fall in; addresses outside every subnet
    // make a /24 network of their own.
    let mut used: IndexMap<String, (Cidr, String)> = IndexMap::new();
    let mut placed: Vec<Vec<(String, u8)>> = Vec::new();
    for m in &machines {
        let mut joins = Vec::new();
        for ip in &m.addresses {
            let ipn = u32::from(*ip);
            let hit = nets
                .values()
                .find(|(c, _)| c.contains(Cidr { base: ipn, len: 32 }))
                .cloned()
                .unwrap_or_else(|| {
                    let c = Cidr {
                        base: ipn & 0xffff_ff00,
                        len: 24,
                    };
                    (c, String::new())
                });
            let key = format!("{}/{}", Ipv4Addr::from(hit.0.base), hit.0.len);
            used.entry(key.clone()).or_insert(hit);
            joins.push((key, ip.octets()[3]));
        }
        placed.push(joins);
    }
    // Machines without an address: on the first network (or a `lab` one), at the next free octet.
    if placed.iter().any(Vec::is_empty) {
        let key = match used.keys().next() {
            Some(k) => k.clone(),
            None => {
                used.insert(
                    "10.88.1.0/24".into(),
                    (
                        Cidr {
                            base: u32::from(Ipv4Addr::new(10, 88, 1, 0)),
                            len: 24,
                        },
                        "lab".into(),
                    ),
                );
                "10.88.1.0/24".into()
            }
        };
        let mut taken: Vec<u8> = placed.iter().flatten().filter(|(k, _)| *k == key).map(|(_, o)| *o).collect();
        let mut next = 10u8;
        for joins in placed.iter_mut().filter(|j| j.is_empty()) {
            while taken.contains(&next) {
                next += 1;
            }
            joins.push((key.clone(), next));
            taken.push(next);
        }
    }
    let mut names: IndexMap<String, String> = IndexMap::new();
    for (i, (key, (_, named))) in used.iter().enumerate() {
        let mut n = if named.is_empty() {
            if i == 0 { "lab".into() } else { format!("lab-{}", i + 1) }
        } else {
            named.clone()
        };
        while names.values().any(|x| x == &n) {
            n.push_str("-2");
        }
        names.insert(key.clone(), n);
    }

    let mut y = String::new();
    let _ = writeln!(y, "{}", crate::schema::MODELINE);
    let _ = writeln!(
        y,
        "# Drafted by `isoloom import terraform` from {source}. Review it with the notes the import printed."
    );
    y.push_str("# Next: add `vm.provision` (or `provision:`) to configure the machines, and `checks` that prove the behavior.\n");
    let _ = writeln!(y, "version: 1\nname: {}\n\nnetworks:", label(fallback_name));
    for (key, n) in &names {
        let _ = writeln!(y, "  {n}: {{ cidr: {key} }}");
    }
    y.push_str("\nmachines:\n");
    let mut taken = std::collections::BTreeSet::new();
    for (i, (m, joins)) in machines.iter().zip(&placed).enumerate() {
        if i > 0 {
            y.push('\n');
        }
        let mut name = m.name.clone();
        while !taken.insert(name.clone()) {
            name.push_str("-2");
        }
        let _ = writeln!(y, "  {name}:");
        if !joins.is_empty() {
            let j = joins.iter().map(|(k, o)| format!("{}: {o}", names[k])).collect::<Vec<_>>().join(", ");
            let _ = writeln!(y, "    networks: {{ {j} }}");
        }
        let mut res = Vec::new();
        if let Some(c) = m.cpus {
            res.push(format!("cpus: {c}"));
        }
        if let Some(mb) = m.memory_mb {
            res.push(format!("memory_mb: {}", mb.max(256)));
        }
        if !res.is_empty() {
            let _ = writeln!(y, "    resources: {{ {} }}", res.join(", "));
        }
        let _ = writeln!(y, "    vm:\n      os: {}", m.os);
    }
    Ok(Draft { yaml: y, notes })
}
