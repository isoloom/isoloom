//! Drafts an `isoloom.yml` from a Compose file. What Compose says that the format can express
//! goes into the draft; everything else becomes a note, with the coverage table's verdict
//! (belongs in the image, not expressible yet, by design...), so nothing is dropped silently.

mod keys;
mod lenient;
mod networks;
mod values;
mod write;

use std::collections::BTreeSet;

use indexmap::IndexMap;
use serde_yaml_ng::{Mapping, Value};

pub use super::label;
use super::{Draft, ImportError, NoteKind, Notes};
use crate::coverage::{Support, compose};
use lenient::{Duplicate, lenient_yaml};
use networks::Networks;
use values::{PortKey, VolumeMount, flat, str_of, volume_mount};

/// The network Compose attaches services to when they name none.
const DEFAULT_NETWORK: &str = "default";

#[derive(Default)]
struct Machine {
    name: String,
    networks: Vec<(String, u8)>,
    ports: BTreeSet<PortKey>,
    /// Container port -> host port it's published on.
    publish: IndexMap<PortKey, u16>,
    inputs: BTreeSet<String>,
    cpus: Option<u32>,
    memory_mb: Option<u32>,
    depends_on: Vec<String>,
    volumes: Vec<(String, String)>,
    image: Option<String>,
    build: Option<String>,
    dockerfile: Option<String>,
    target: Option<String>,
    args: Vec<(String, String)>,
}

/// The coverage verdict for a Compose key path, as a note kind and text.
fn verdict(path: &str) -> Option<(NoteKind, String)> {
    let (_, s) = compose::format().rows.into_iter().find(|(k, _)| *k == path)?;
    let kind = match s {
        Support::Emitted { .. } => NoteKind::Written,
        // Isoloom writes the key for some uses; the source's use is the part it doesn't cover.
        Support::Partial { gap, .. } => return Some((NoteKind::NotYet, gap.to_string())),
        Support::Equivalent { .. } => NoteKind::Equivalent,
        Support::InImage { .. } => NoteKind::InImage,
        Support::NotPortable { .. } => NoteKind::ByDesign,
        Support::Tooling { .. } => NoteKind::Tooling,
        Support::Planned { .. } | Support::Unclassified => NoteKind::NotYet,
    };
    Some((kind, s.note()))
}

/// A service being read: its Compose name, where its notes go, and its keys.
struct Service<'a> {
    name: &'a str,
    at: String,
    keys: Mapping,
}

/// What the import knows about the whole file, and what it collects while it reads the services.
struct Import<'a> {
    services: &'a Mapping,
    notes: Notes,
    nets: Networks,
    /// Compose service name -> machine name.
    names: IndexMap<String, String>,
    /// Services others wait to *complete*: they run once and exit.
    one_shot: BTreeSet<String>,
    /// Named volume -> how many services mount it.
    volume_users: IndexMap<String, usize>,
    /// Every input a machine reads, for the spec's own `inputs`.
    spec_inputs: BTreeSet<String>,
}

/// Drafts a spec from Compose YAML. `fallback_name` names the environment when the file
/// doesn't (usually its folder); `source` is how the file is named in the draft's header.
pub fn draft(compose_yaml: &str, fallback_name: &str, source: &str) -> Result<Draft, ImportError> {
    let (doc, dups) = lenient_yaml(compose_yaml)?;
    let root = doc.as_mapping().ok_or(ImportError::ComposeNotMapping)?;
    let services = root
        .get("services")
        .and_then(Value::as_mapping)
        .filter(|s| !s.is_empty())
        .ok_or(ImportError::NoServices)?;
    let mut notes = Notes::default();
    note_duplicates(&mut notes, dups);
    let name = root.get("name").and_then(str_of).map_or_else(|| label(fallback_name), |n| label(&n));
    note_top_level(&mut notes, root);
    let mut import = Import::new(services, notes);
    // Networks: declared ones, then the default network for services that name none.
    let declared: Mapping = root.get("networks").and_then(Value::as_mapping).cloned().unwrap_or_default();
    for (k, def) in &declared {
        import.nets.declare(k.as_str().unwrap_or_default(), def, &mut import.notes)?;
    }
    let mut machines = Vec::new();
    for (svc_key, svc) in services {
        if let Some(m) = import.service(svc_key.as_str().unwrap_or_default(), svc)? {
            machines.push(m);
        }
    }
    prune_depends_on(&mut machines, &mut import.notes);
    let env = write::Environment {
        name: &name,
        source,
        nets: &import.nets,
        inputs: &import.spec_inputs,
    };
    let yaml = write::draft_yaml(&env, &machines);
    Ok(import.notes.draft(yaml))
}

fn note_duplicates(notes: &mut Notes, dups: Vec<Duplicate>) {
    for d in dups {
        let at = if d.at.is_empty() { d.key.clone() } else { d.at };
        let kept = if d.kept.is_empty() { String::new() } else { format!(" (`{}`)", d.kept) };
        notes.add(
            at,
            NoteKind::Changed,
            format!("`{}` is set more than once; the last value{kept} is kept, as Compose does", d.key),
        );
    }
}

/// The top-level keys besides the ones read: the coverage table's verdict.
fn note_top_level(notes: &mut Notes, root: &Mapping) {
    for (k, _) in root {
        let k = k.as_str().unwrap_or_default();
        if matches!(k, "name" | "services" | "networks" | "volumes") || k.starts_with("x-") {
            continue;
        }
        if let Some((kind, text)) = verdict(k) {
            notes.add(k, kind, text);
        }
    }
}

/// Services others wait to *complete* run once and exit: Isoloom machines stay up.
fn one_shot_services(services: &Mapping) -> BTreeSet<String> {
    let mut one_shot = BTreeSet::new();
    for svc in services.values() {
        for (dep, cond) in svc.get("depends_on").and_then(Value::as_mapping).into_iter().flatten() {
            if cond.get("condition").and_then(str_of).as_deref() == Some("service_completed_successfully")
                && let Some(d) = str_of(dep)
            {
                one_shot.insert(d);
            }
        }
    }
    one_shot
}

/// Named volumes used by more than one service: shared data, not expressible yet.
fn volume_users(services: &Mapping) -> IndexMap<String, usize> {
    let mut users: IndexMap<String, usize> = IndexMap::new();
    for svc in services.values() {
        for item in svc.get("volumes").and_then(Value::as_sequence).into_iter().flatten() {
            if let Some((VolumeMount::Named(n), _)) = volume_mount(item) {
                *users.entry(n).or_default() += 1;
            }
        }
    }
    users
}

/// depends_on needs something to wait for. A UDP port has no handshake to wait for.
fn prune_depends_on(machines: &mut [Machine], notes: &mut Notes) {
    let with_ports: BTreeSet<String> = machines
        .iter()
        .filter(|m| m.ports.iter().any(|(_, p)| p.is_tcp()))
        .map(|m| m.name.clone())
        .collect();
    let drafted: BTreeSet<String> = machines.iter().map(|m| m.name.clone()).collect();
    for m in machines {
        let (keep, drop): (Vec<String>, Vec<String>) = m.depends_on.drain(..).partition(|d| with_ports.contains(d));
        m.depends_on = keep;
        for d in drop {
            if !drafted.contains(&d) {
                continue; // left out above, with its note
            }
            notes.add(
                format!("machines.{}.depends_on", m.name),
                NoteKind::Changed,
                format!("`{d}` has no known TCP port (UDP has no handshake), so there's nothing to wait for; add its `ports` and the dependency back"),
            );
        }
    }
}

impl<'a> Import<'a> {
    fn new(services: &'a Mapping, notes: Notes) -> Self {
        Import {
            services,
            notes,
            nets: Networks::new(),
            names: services.keys().filter_map(|k| k.as_str()).map(|k| (k.to_string(), label(k))).collect(),
            one_shot: one_shot_services(services),
            volume_users: volume_users(services),
            spec_inputs: BTreeSet::new(),
        }
    }

    /// A service as a machine, or none when it's left out.
    fn service(&mut self, name: &str, svc: &Value) -> Result<Option<Machine>, ImportError> {
        let svc = Service {
            name,
            at: format!("services.{name}"),
            keys: svc.as_mapping().cloned().unwrap_or_default(),
        };
        if self.left_out(&svc) {
            return Ok(None);
        }
        let mut m = Machine {
            name: self.names[name].clone(),
            ..Machine::default()
        };
        if m.name != name {
            self.note_rename(&svc, &m.name);
        }
        for (n, ip) in self.wanted_networks(&svc) {
            self.join(&svc, &mut m, &n, ip.as_deref())?;
        }
        for (k, v) in &svc.keys {
            self.key(&svc, k.as_str().unwrap_or_default(), v, &mut m);
        }
        if m.image.is_none() && m.build.is_none() {
            self.notes.add(svc.at, NoteKind::Changed, "no image or build; add one to `docker:`");
        }
        Ok(Some(m))
    }

    /// Isoloom machines stay up (and restart): a job that exits, or a service Compose only
    /// starts on request, would change behavior as a machine.
    fn left_out(&mut self, svc: &Service) -> bool {
        if self.one_shot.contains(svc.name) {
            self.notes.add(
                &svc.at,
                NoteKind::Changed,
                "left out: it runs once and exits (others wait for it to complete); one-shot work is a `docker.init` job of the machine it prepares",
            );
            return true;
        }
        if let Some(p) = svc.keys.get("profiles") {
            self.notes.add(
                &svc.at,
                NoteKind::Changed,
                format!(
                    "left out: only started with profile {}; if it's a test, make it a check script (`checks:`)",
                    flat(p)
                ),
            );
            return true;
        }
        false
    }

    fn note_rename(&mut self, svc: &Service, renamed: &str) {
        // Other services may reach it by its old name (in their settings): list where.
        let users: Vec<String> = self
            .services
            .iter()
            .filter(|(_, other)| other.get("environment").is_some_and(|e| flat(e).contains(svc.name)))
            .filter_map(|(k, _)| k.as_str().map(String::from))
            .collect();
        let refs = if users.is_empty() {
            String::new()
        } else {
            format!("; the environment of {} still says `{}`: update it", users.join(", "), svc.name)
        };
        self.notes
            .add(&svc.at, NoteKind::Changed, format!("renamed `{renamed}` (machine names are DNS names){refs}"));
    }

    /// The networks a service joins, each with the address it asks for, if any.
    fn wanted_networks(&mut self, svc: &Service) -> Vec<(String, Option<String>)> {
        let mut wanted: Vec<(String, Option<String>)> = Vec::new();
        match svc.keys.get("networks") {
            Some(Value::Sequence(list)) => wanted.extend(list.iter().filter_map(str_of).map(|n| (n, None))),
            Some(Value::Mapping(map)) => {
                for (n, opts) in map {
                    let n = n.as_str().unwrap_or_default().to_string();
                    let ip = opts.get("ipv4_address").and_then(str_of);
                    if opts.get("aliases").is_some() {
                        self.notes
                            .add(format!("{}.networks.{n}.aliases", svc.at), NoteKind::NotYet, "aliases: a machine has one name");
                    }
                    wanted.push((n, ip));
                }
            }
            _ => wanted.push((DEFAULT_NETWORK.to_string(), None)),
        }
        if matches!(svc.keys.get("network_mode").and_then(str_of).as_deref(), Some(mode) if mode != "bridge") {
            self.notes.add(
                format!("{}.network_mode", svc.at),
                NoteKind::ByDesign,
                "a machine has its own addresses; attached to the default network instead",
            );
            wanted = vec![(DEFAULT_NETWORK.to_string(), None)];
        }
        wanted
    }

    /// Puts the machine on network `n`, at the address it asked for when that's usable.
    fn join(&mut self, svc: &Service, m: &mut Machine, n: &str, ip: Option<&str>) -> Result<(), ImportError> {
        let net = self.nets.get(n, &mut self.notes)?;
        let asked = ip.and_then(|ip| ip.parse::<std::net::Ipv4Addr>().ok()).map(|ip| ip.octets()[3]);
        let (octet, moved) = net.take(asked).ok_or_else(|| ImportError::NetworkFull {
            network: n.to_string(),
            machine: m.name.clone(),
        })?;
        m.networks.push((net.name.clone(), octet));
        if moved && let Some(ip) = ip {
            self.notes.add(
                format!("{}.networks.{n}.ipv4_address", svc.at),
                NoteKind::Changed,
                format!("{ip} isn't usable here; given .{octet}"),
            );
        }
        Ok(())
    }
}
