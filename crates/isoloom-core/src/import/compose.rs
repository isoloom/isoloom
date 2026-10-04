//! Drafts an `isoloom.yml` from a Compose file. What Compose says that the format can express
//! goes into the draft; everything else becomes a note, with the coverage table's verdict
//! (belongs in the image, not expressible yet, by design...), so nothing is dropped silently.

use std::collections::BTreeSet;
use std::fmt::Write;

use indexmap::IndexMap;
use serde_yaml_ng::{Mapping, Value};

use super::{Draft, Note, NoteKind};
use crate::coverage::{Support, compose};
use crate::validate::Cidr;

/// Where re-addressed networks go: 10.88.<n>.0/24.
const READDRESS_BASE: &str = "10.88";
/// The network Compose attaches services to when they name none.
const DEFAULT_NETWORK: &str = "default";

struct Net {
    name: String,
    cidr: Cidr,
    cidr_text: String,
    internet: bool,
    taken: BTreeSet<u8>,
}

struct Machine {
    name: String,
    networks: Vec<(String, u8)>,
    ports: BTreeSet<u16>,
    inputs: BTreeSet<String>,
    cpus: Option<u32>,
    memory_mb: Option<u32>,
    depends_on: Vec<String>,
    image: Option<String>,
    build: Option<String>,
}

/// Turns a Compose name into a DNS label (`my_app` -> `my-app`).
pub fn label(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    match out.chars().next() {
        None => "env".into(),
        Some(c) if c.is_ascii_digit() => format!("m-{out}"),
        _ => out.chars().take(63).collect(),
    }
}

fn str_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Variables a Compose value reads from the shell: `${NAME}`, `${NAME:-x}`, `$NAME`.
fn variables(s: &str) -> Vec<String> {
    let mut found = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'$' && i + 1 < b.len() {
            if b[i + 1] == b'$' {
                i += 2;
                continue;
            }
            let (start, braced) = if b[i + 1] == b'{' { (i + 2, true) } else { (i + 1, false) };
            let end = (start..b.len()).find(|&j| !(b[j].is_ascii_alphanumeric() || b[j] == b'_')).unwrap_or(b.len());
            if end > start && (!braced || end < b.len()) {
                found.push(s[start..end].to_string());
            }
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    found
}

/// An input name (UPPER_SNAKE_CASE) from an environment variable name.
fn input_name(s: &str) -> Option<String> {
    let up = s.to_ascii_uppercase();
    let ok = !up.is_empty() && up.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') && !up.starts_with(|c: char| c.is_ascii_digit());
    ok.then_some(up)
}

/// Container ports from Compose `ports` (`"8080:80"`, `"127.0.0.1:8080:80/tcp"`, `80`,
/// `{ target: 80 }`) and `expose`; ranges are expanded up to 32 ports.
fn container_ports(v: &Value) -> Result<Vec<u16>, String> {
    let mut out = Vec::new();
    for item in v.as_sequence().into_iter().flatten() {
        let spec = match item {
            Value::Mapping(m) => m.get("target").and_then(str_of).unwrap_or_default(),
            other => str_of(other).unwrap_or_default(),
        };
        let container = spec.rsplit(':').next().unwrap_or_default();
        let container = container.split('/').next().unwrap_or_default();
        let (lo, hi) = match container.split_once('-') {
            Some((a, b)) => (a.parse::<u16>(), b.parse::<u16>()),
            None => (container.parse::<u16>(), container.parse::<u16>()),
        };
        match (lo, hi) {
            (Ok(lo), Ok(hi)) if lo <= hi && hi - lo < 32 => out.extend(lo..=hi),
            _ => return Err(spec),
        }
    }
    Ok(out)
}

/// Memory like `512m`, `1g`, `1.5G`, `268435456` in MB.
fn memory_mb(s: &str) -> Option<u32> {
    let s = s.trim().to_ascii_lowercase();
    let s = s.strip_suffix('b').unwrap_or(&s);
    let (num, factor) = match s.chars().last()? {
        'k' => (&s[..s.len() - 1], 1.0 / 1024.0),
        'm' => (&s[..s.len() - 1], 1.0),
        'g' => (&s[..s.len() - 1], 1024.0),
        _ => (s, 1.0 / (1024.0 * 1024.0)),
    };
    let mb = num.parse::<f64>().ok()? * factor;
    (mb >= 1.0).then(|| mb.ceil() as u32)
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
        Support::Planned { .. } | Support::Open { .. } => NoteKind::NotYet,
        Support::ByDesign { .. } => NoteKind::ByDesign,
        Support::Tooling { .. } => NoteKind::Tooling,
        Support::Unclassified => NoteKind::NotYet,
    };
    Some((kind, s.reason()))
}

fn note(notes: &mut Vec<Note>, at: String, kind: NoteKind, text: String) {
    notes.push(Note { at, kind, text });
}

/// A Compose value on one line, for notes.
fn flat(v: &Value) -> String {
    match v {
        Value::Sequence(items) => items.iter().map(flat).collect::<Vec<_>>().join(" "),
        Value::Mapping(m) => m.iter().map(|(k, v)| format!("{}={}", flat(k), flat(v))).collect::<Vec<_>>().join(" "),
        other => str_of(other).unwrap_or_default(),
    }
}

fn yaml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Drafts a spec from Compose YAML. `fallback_name` names the environment when the file
/// doesn't (usually its folder); `source` is how the file is named in the draft's header.
pub fn draft(compose_yaml: &str, fallback_name: &str, source: &str) -> Result<Draft, String> {
    let doc: Value = serde_yaml_ng::from_str(compose_yaml).map_err(|e| format!("not a Compose file: {e}"))?;
    let root = doc.as_mapping().ok_or("not a Compose file: expected a mapping")?;
    let services = root
        .get("services")
        .and_then(Value::as_mapping)
        .filter(|s| !s.is_empty())
        .ok_or("the Compose file has no services")?;
    let mut notes = Vec::new();

    // Top level.
    let name = root.get("name").and_then(str_of).map(|n| label(&n)).unwrap_or_else(|| label(fallback_name));
    for (k, _) in root {
        let k = k.as_str().unwrap_or_default();
        if matches!(k, "name" | "services" | "networks") || k.starts_with("x-") {
            continue;
        }
        if let Some((kind, text)) = verdict(k) {
            note(&mut notes, k.to_string(), kind, text);
        }
    }

    // Networks: declared ones, then the default network for services that name none.
    let declared: Mapping = root.get("networks").and_then(Value::as_mapping).cloned().unwrap_or_default();
    let mut nets: IndexMap<String, Net> = IndexMap::new();
    let mut next_block = 1u32;
    let mut add_net = |nets: &mut IndexMap<String, Net>, compose_name: &str, def: Option<&Value>, notes: &mut Vec<Note>| {
        let at = format!("networks.{compose_name}");
        let subnet = def
            .and_then(|d| d.get("ipam"))
            .and_then(|i| i.get("config"))
            .and_then(Value::as_sequence)
            .and_then(|c| c.first())
            .and_then(|c| c.get("subnet"))
            .and_then(str_of);
        let usable = subnet
            .as_deref()
            .and_then(Cidr::parse)
            .filter(|c| c.len >= 24 && c.len <= 29 && (c.base >> 24) == 10);
        let (cidr, text) = match usable {
            Some(c) => (c, subnet.clone().unwrap()),
            None => {
                let text = format!("{READDRESS_BASE}.{next_block}.0/24");
                next_block += 1;
                let why = match &subnet {
                    Some(s) => format!("{s} isn't a /24 to /29 inside 10.0.0.0/8; re-addressed to {text}"),
                    None => format!("no subnet given; addressed as {text}"),
                };
                notes.push(Note {
                    at: format!("{at}.ipam"),
                    kind: NoteKind::Changed,
                    text: why,
                });
                (Cidr::parse(&text).expect("valid"), text)
            }
        };
        let internal = def.and_then(|d| d.get("internal")).and_then(Value::as_bool).unwrap_or(false);
        for (k, _) in def.and_then(Value::as_mapping).into_iter().flatten() {
            let k = k.as_str().unwrap_or_default();
            if matches!(k, "ipam" | "internal") || k.starts_with("x-") {
                continue;
            }
            if let Some((kind, text)) = verdict(&format!("networks.*.{k}")) {
                notes.push(Note {
                    at: format!("{at}.{k}"),
                    kind,
                    text,
                });
            }
        }
        let name = if compose_name == DEFAULT_NETWORK {
            "lab".to_string()
        } else {
            label(compose_name)
        };
        nets.insert(
            compose_name.to_string(),
            Net {
                name,
                cidr,
                cidr_text: text,
                internet: !internal,
                taken: BTreeSet::new(),
            },
        );
    };
    for (k, def) in &declared {
        let k = k.as_str().unwrap_or_default().to_string();
        add_net(&mut nets, &k, Some(def), &mut notes);
    }

    // Services others wait to *complete* run once and exit: Isoloom machines stay up.
    let mut one_shot: BTreeSet<String> = BTreeSet::new();
    for svc in services.values() {
        for (dep, cond) in svc.get("depends_on").and_then(Value::as_mapping).into_iter().flatten() {
            if cond.get("condition").and_then(str_of).as_deref() == Some("service_completed_successfully")
                && let Some(d) = str_of(dep)
            {
                one_shot.insert(d);
            }
        }
    }

    // Services.
    let names: IndexMap<String, String> = services.keys().filter_map(|k| k.as_str()).map(|k| (k.to_string(), label(k))).collect();
    let mut machines: Vec<Machine> = Vec::new();
    let mut spec_inputs: BTreeSet<String> = BTreeSet::new();
    for (svc_key, svc) in services {
        let svc_name = svc_key.as_str().unwrap_or_default();
        let at = format!("services.{svc_name}");
        let s = svc.as_mapping().cloned().unwrap_or_default();
        // Isoloom machines stay up (and restart): a job that exits, or a service Compose only
        // starts on request, would change behavior as a machine.
        if one_shot.contains(svc_name) {
            note(
                &mut notes,
                at,
                NoteKind::Changed,
                "left out: it runs once and exits (others wait for it to complete); one-shot work is a `docker.init` job of the machine it prepares".into(),
            );
            continue;
        }
        if let Some(p) = s.get("profiles") {
            note(
                &mut notes,
                at,
                NoteKind::Changed,
                format!(
                    "left out: only started with profile {}; if it's a test, make it a check script (`checks:`)",
                    flat(p)
                ),
            );
            continue;
        }
        let mut m = Machine {
            name: names[svc_name].clone(),
            networks: Vec::new(),
            ports: BTreeSet::new(),
            inputs: BTreeSet::new(),
            cpus: None,
            memory_mb: None,
            depends_on: Vec::new(),
            image: None,
            build: None,
        };
        if m.name != svc_name {
            // Other services may reach it by its old name (in their settings): list where.
            let users: Vec<String> = services
                .iter()
                .filter(|(_, other)| other.get("environment").is_some_and(|e| flat(e).contains(svc_name)))
                .filter_map(|(k, _)| k.as_str().map(String::from))
                .collect();
            let refs = if users.is_empty() {
                String::new()
            } else {
                format!("; the environment of {} still says `{svc_name}`: update it", users.join(", "))
            };
            note(
                &mut notes,
                at.clone(),
                NoteKind::Changed,
                format!("renamed `{}` (machine names are DNS names){refs}", m.name),
            );
        }

        // Networks and addresses.
        let mut wanted: Vec<(String, Option<String>)> = Vec::new();
        match s.get("networks") {
            Some(Value::Sequence(list)) => wanted.extend(list.iter().filter_map(str_of).map(|n| (n, None))),
            Some(Value::Mapping(map)) => {
                for (n, opts) in map {
                    let n = n.as_str().unwrap_or_default().to_string();
                    let ip = opts.get("ipv4_address").and_then(str_of);
                    if opts.get("aliases").is_some() {
                        note(
                            &mut notes,
                            format!("{at}.networks.{n}.aliases"),
                            NoteKind::NotYet,
                            "aliases: a machine has one name".into(),
                        );
                    }
                    wanted.push((n, ip));
                }
            }
            _ => wanted.push((DEFAULT_NETWORK.to_string(), None)),
        }
        if matches!(s.get("network_mode").and_then(str_of).as_deref(), Some(mode) if mode != "bridge") {
            wanted.clear();
            note(
                &mut notes,
                format!("{at}.network_mode"),
                NoteKind::ByDesign,
                "a machine has its own addresses; attached to the default network instead".into(),
            );
            wanted.push((DEFAULT_NETWORK.to_string(), None));
        }
        for (n, ip) in wanted {
            if !nets.contains_key(&n) {
                add_net(&mut nets, &n, None, &mut notes);
            }
            let net = nets.get_mut(&n).expect("added");
            let asked = ip.as_deref().and_then(|ip| ip.parse::<std::net::Ipv4Addr>().ok()).map(|ip| ip.octets()[3]);
            let octet = match asked.filter(|o| net.cidr.host(*o).is_some() && !net.taken.contains(o)) {
                Some(o) => o,
                None => {
                    let o = (10..=250u8)
                        .find(|o| net.cidr.host(*o).is_some() && !net.taken.contains(o))
                        .ok_or_else(|| format!("network `{n}` has no free address left for `{}`", m.name))?;
                    if let Some(ip) = &ip {
                        note(
                            &mut notes,
                            format!("{at}.networks.{n}.ipv4_address"),
                            NoteKind::Changed,
                            format!("{ip} isn't usable here; given .{o}"),
                        );
                    }
                    o
                }
            };
            net.taken.insert(octet);
            m.networks.push((net.name.clone(), octet));
        }

        // Every other key.
        for (k, v) in &s {
            let k = k.as_str().unwrap_or_default();
            let kat = format!("{at}.{k}");
            match k {
                "networks" | "network_mode" => {}
                "image" if s.contains_key("build") => {}
                "image" => {
                    let image = str_of(v).unwrap_or_default();
                    if image.contains('$') {
                        note(
                            &mut notes,
                            kat,
                            NoteKind::Changed,
                            format!("`{image}` reads shell variables; kept as written, fix it to one image"),
                        );
                    }
                    m.image = Some(image);
                }
                "build" => {
                    let (context, extra) = match v {
                        Value::Mapping(b) => (
                            b.get("context").and_then(str_of).unwrap_or_else(|| ".".into()),
                            b.iter()
                                .filter_map(|(k, v)| Some((k.as_str()?, v)))
                                // `dockerfile: Dockerfile` is the default: nothing lost.
                                .filter(|(k, v)| *k != "context" && !(*k == "dockerfile" && str_of(v).as_deref() == Some("Dockerfile")))
                                .map(|(k, _)| k.to_string())
                                .collect::<Vec<_>>(),
                        ),
                        other => (str_of(other).unwrap_or_else(|| ".".into()), Vec::new()),
                    };
                    let context = context.trim_start_matches("./").trim_end_matches('/').to_string();
                    let context = if context.is_empty() { ".".to_string() } else { context };
                    if context.starts_with("..") || context.starts_with('/') || context.contains("://") {
                        note(
                            &mut notes,
                            kat.clone(),
                            NoteKind::Changed,
                            format!("context `{context}` is outside the project; copy it in"),
                        );
                    }
                    if !extra.is_empty() {
                        note(
                            &mut notes,
                            kat,
                            NoteKind::InImage,
                            format!("only the context is kept, not: {} (build settings belong in the Dockerfile)", extra.join(", ")),
                        );
                    }
                    m.build = Some(context);
                    if s.contains_key("image") {
                        note(
                            &mut notes,
                            format!("{at}.image"),
                            NoteKind::Equivalent,
                            "names the built image; Isoloom names it".into(),
                        );
                    }
                }
                "ports" | "expose" => match container_ports(v) {
                    Ok(ports) => {
                        m.ports.extend(ports);
                        if k == "ports" {
                            note(
                                &mut notes,
                                kat,
                                NoteKind::ByDesign,
                                "not published on the host: the environment is reached from its own networks; the container ports became services".into(),
                            );
                        }
                    }
                    Err(bad) => note(
                        &mut notes,
                        kat,
                        NoteKind::Changed,
                        format!("couldn't read port `{bad}`; add it to `services` by hand"),
                    ),
                },
                "environment" => {
                    let mut fixed = Vec::new();
                    let entries: Vec<(String, Option<String>)> = match v {
                        Value::Mapping(env) => env.iter().map(|(k, v)| (str_of(k).unwrap_or_default(), str_of(v))).collect(),
                        Value::Sequence(list) => list
                            .iter()
                            .filter_map(str_of)
                            .map(|e| match e.split_once('=') {
                                Some((k, v)) => (k.to_string(), Some(v.to_string())),
                                None => (e, None),
                            })
                            .collect(),
                        _ => Vec::new(),
                    };
                    for (key, value) in entries {
                        let from_shell = match &value {
                            None => input_name(&key).into_iter().collect(),
                            Some(v) => variables(v).iter().filter_map(|v| input_name(v)).collect::<Vec<_>>(),
                        };
                        if from_shell.is_empty() {
                            fixed.push(format!("{key}={}", value.unwrap_or_default()));
                        }
                        for i in from_shell {
                            m.inputs.insert(i.clone());
                            spec_inputs.insert(i);
                        }
                    }
                    if !fixed.is_empty() {
                        note(
                            &mut notes,
                            kat,
                            NoteKind::InImage,
                            format!("fixed values belong in the image (ENV): {}", fixed.join(", ")),
                        );
                    }
                }
                "depends_on" => {
                    let deps: Vec<String> = match v {
                        Value::Sequence(list) => list.iter().filter_map(str_of).collect(),
                        Value::Mapping(map) => map.keys().filter_map(str_of).collect(),
                        _ => Vec::new(),
                    };
                    m.depends_on.extend(deps.iter().filter_map(|d| names.get(d).cloned()));
                }
                "cpus" => m.cpus = str_of(v).and_then(|c| c.parse::<f64>().ok()).map(|c| c.ceil().max(1.0) as u32),
                "mem_limit" => m.memory_mb = str_of(v).and_then(|s| memory_mb(&s)),
                "deploy" => {
                    let limits = v.get("resources").and_then(|r| r.get("limits"));
                    if let Some(c) = limits.and_then(|l| l.get("cpus")).and_then(str_of).and_then(|c| c.parse::<f64>().ok()) {
                        m.cpus = Some(c.ceil().max(1.0) as u32);
                    }
                    if let Some(mb) = limits.and_then(|l| l.get("memory")).and_then(str_of).and_then(|s| memory_mb(&s)) {
                        m.memory_mb = Some(mb);
                    }
                    let other: Vec<&str> = v
                        .as_mapping()
                        .into_iter()
                        .flatten()
                        .filter_map(|(k, _)| k.as_str())
                        .filter(|k| *k != "resources")
                        .collect();
                    if !other.is_empty() {
                        note(
                            &mut notes,
                            kat,
                            NoteKind::NotYet,
                            format!("only resource limits are kept; not: {}", other.join(", ")),
                        );
                    }
                }
                "command" | "entrypoint" => note(
                    &mut notes,
                    kat,
                    NoteKind::InImage,
                    format!("set it in the image (CMD / ENTRYPOINT): {}", flat(v)),
                ),
                "volumes" => {
                    let (mounts, data): (Vec<String>, Vec<String>) = v
                        .as_sequence()
                        .into_iter()
                        .flatten()
                        .map(flat)
                        .partition(|m| m.starts_with('.') || m.starts_with('/'));
                    if !mounts.is_empty() {
                        note(
                            &mut notes,
                            kat.clone(),
                            NoteKind::InImage,
                            format!(
                                "project files mounted in: copy them into the image, or run scripts as `docker.init` jobs: {}",
                                mounts.join(", ")
                            ),
                        );
                    }
                    if !data.is_empty() {
                        note(
                            &mut notes,
                            kat,
                            NoteKind::NotYet,
                            format!("persistent or shared data (no concept in the format yet): {}", data.join(", ")),
                        );
                    }
                }
                "hostname" | "container_name" => note(
                    &mut notes,
                    kat,
                    NoteKind::Equivalent,
                    format!("the hostname is the machine's name, `{}`", m.name),
                ),
                "healthcheck" => note(
                    &mut notes,
                    kat,
                    NoteKind::Equivalent,
                    "Isoloom probes every service port; a custom check belongs in `checks`".into(),
                ),
                "env_file" => note(
                    &mut notes,
                    kat,
                    NoteKind::Equivalent,
                    "values given at launch are `inputs`: list the ones the machine needs".into(),
                ),
                _ if k.starts_with("x-") => note(&mut notes, kat, NoteKind::Tooling, "extension field, ignored".into()),
                _ => match verdict(&format!("services.*.{k}")) {
                    Some((kind, text)) => note(&mut notes, kat, kind, text),
                    None => note(&mut notes, kat, NoteKind::Changed, "not a Compose key Isoloom knows; ignored".into()),
                },
            }
        }
        if m.image.is_none() && m.build.is_none() {
            note(&mut notes, at.clone(), NoteKind::Changed, "no image or build; add one to `docker:`".into());
        }
        machines.push(m);
    }

    // depends_on needs something to wait for.
    let with_ports: BTreeSet<String> = machines.iter().filter(|m| !m.ports.is_empty()).map(|m| m.name.clone()).collect();
    let drafted: BTreeSet<String> = machines.iter().map(|m| m.name.clone()).collect();
    for m in &mut machines {
        let (keep, drop): (Vec<String>, Vec<String>) = m.depends_on.drain(..).partition(|d| with_ports.contains(d));
        m.depends_on = keep;
        for d in drop {
            if !drafted.contains(&d) {
                continue; // left out above, with its note
            }
            note(
                &mut notes,
                format!("machines.{}.depends_on", m.name),
                NoteKind::Changed,
                format!("`{d}` has no known port, so there's nothing to wait for; add its `services` and the dependency back"),
            );
        }
    }

    // The draft.
    let mut y = String::new();
    let _ = writeln!(y, "# Drafted by `isoloom import compose` from {source}. Review it with the notes the");
    y.push_str("# import printed: what Compose said that this file doesn't (yet), and why.\n");
    y.push_str("# Next: add `vm:` to each machine for the VM targets, and `checks` that prove the behavior.\n");
    y.push_str("version: 1\n");
    let _ = writeln!(y, "name: {name}\n");
    y.push_str("networks:\n");
    let width = nets.values().map(|n| n.name.len()).max().unwrap_or(0);
    for n in nets.values() {
        let pad = " ".repeat(width - n.name.len());
        let internet = if n.internet { String::new() } else { ", internet: false".into() };
        let _ = writeln!(y, "  {}:{pad} {{ cidr: {}{internet} }}", n.name, n.cidr_text);
    }
    if !spec_inputs.is_empty() {
        let _ = writeln!(y, "\ninputs: [{}]", spec_inputs.iter().cloned().collect::<Vec<_>>().join(", "));
    }
    y.push_str("\nmachines:\n");
    for (i, m) in machines.iter().enumerate() {
        if i > 0 {
            y.push('\n');
        }
        let _ = writeln!(y, "  {}:", m.name);
        let nets = m.networks.iter().map(|(n, o)| format!("{n}: {o}")).collect::<Vec<_>>().join(", ");
        let _ = writeln!(y, "    networks: {{ {nets} }}");
        if !m.ports.is_empty() {
            let ports = m.ports.iter().map(|p| format!("{{ port: {p} }}")).collect::<Vec<_>>().join(", ");
            let _ = writeln!(y, "    services: [{ports}]");
        }
        if !m.inputs.is_empty() {
            let _ = writeln!(y, "    inputs: [{}]", m.inputs.iter().cloned().collect::<Vec<_>>().join(", "));
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
        if !m.depends_on.is_empty() {
            let _ = writeln!(y, "    depends_on: [{}]", m.depends_on.join(", "));
        }
        match (&m.build, &m.image) {
            (Some(b), _) => {
                let _ = writeln!(y, "    docker: {{ build: {} }}", yaml_str(b));
            }
            (None, Some(img)) => {
                let _ = writeln!(y, "    docker: {{ image: {} }}", yaml_str(img));
            }
            (None, None) => y.push_str("    docker: { image: \"\" }\n"),
        }
    }
    Ok(Draft { yaml: y, notes })
}
