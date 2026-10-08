//! Drafts an `isoloom.yml` from a Vagrantfile, from what the Vagrantfile set when it ran against
//! a stand-in `Vagrant` module (see the CLI's `vagrant_record.rb`): every setting and call, with
//! loops and variables already evaluated. What the format expresses goes into the draft;
//! everything else becomes a note.

use std::collections::BTreeSet;
use std::fmt::Write;

use indexmap::IndexMap;
use serde_json::Value;

use super::compose::label;
use super::{Draft, Note, NoteKind};
use crate::validate::Cidr;

struct Machine {
    name: String,
    os: String,
    image: Option<(String, Option<String>)>,
    networks: Vec<(String, u8)>,
    publish: Vec<(u16, u16)>,
    cpus: Option<u32>,
    memory_mb: Option<u32>,
    provision: Vec<String>,
}

fn note(notes: &mut Vec<Note>, at: String, kind: NoteKind, text: String) {
    notes.push(Note { at, kind, text });
}

fn calls(node: &Value) -> impl Iterator<Item = &Value> {
    node["calls"].as_array().into_iter().flatten()
}

fn setting<'a>(node: &'a Value, key: &str) -> Option<&'a Value> {
    node["settings"].get(key).filter(|v| !v.is_null())
}

fn text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn number(v: Option<&Value>) -> Option<u32> {
    match v? {
        Value::Number(n) => n.as_u64().map(|n| n as u32),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// An OS name for a box, and whether the box is the one Isoloom would pick for it (otherwise
/// the draft keeps it with `vm.image`).
fn os_for_box(b: &str) -> (&'static str, bool) {
    let l = b.to_ascii_lowercase();
    match l.as_str() {
        "bento/debian-12" | "generic/debian12" => ("debian-12", true),
        "bento/ubuntu-24.04" => ("ubuntu-24.04", true),
        "kalilinux/rolling" => ("kali", true),
        "stefanscherer/windows_2019" => ("windows-server-2019", true),
        _ if l.contains("windows") && l.contains("2022") => ("windows-server-2022", false),
        _ if l.contains("windows") && (l.contains("11") || l.contains("win11")) => ("windows-11", false),
        _ if l.contains("windows") || l.contains("win2019") => ("windows-server-2019", false),
        _ if l.contains("kali") => ("kali", false),
        _ if l.contains("ubuntu") => ("ubuntu-24.04", false),
        _ => ("debian-12", false),
    }
}

/// Memory like `4096`, `"4G"`, `"2048M"` in MB.
fn memory_mb(v: Option<&Value>) -> Option<u32> {
    let s = text(v)?;
    let s = s.trim().to_ascii_uppercase();
    match s.chars().last()? {
        'G' => s[..s.len() - 1].parse::<f64>().ok().map(|g| (g * 1024.0) as u32),
        'M' => s[..s.len() - 1].parse().ok(),
        _ => s.parse().ok(),
    }
}

/// Drafts a spec from a recorded Vagrantfile (`recorded` is the recorder's JSON).
pub fn draft(recorded: &Value, fallback_name: &str, source: &str) -> Result<Draft, String> {
    let mut notes = Vec::new();
    let global = &recorded["children"]["vm"];
    let defines: Vec<&Value> = calls(global).filter(|c| c["name"] == "define").collect();
    // Without `define`, the Vagrantfile is one machine: the top configuration itself.
    let machines_src: Vec<(String, Value)> = if defines.is_empty() {
        vec![("default".into(), recorded.clone())]
    } else {
        defines
            .iter()
            .map(|c| (text(c["args"].get(0)).unwrap_or_default(), c["block"].clone()))
            .collect()
    };

    let mut nets: IndexMap<String, (Cidr, String)> = IndexMap::new(); // cidr text -> (cidr, name)
    let mut machines = Vec::new();
    let mut env_playbooks: Vec<String> = Vec::new();

    for (raw_name, node) in &machines_src {
        let at = format!("define {raw_name}");
        if raw_name.starts_with("isoloom-") {
            note(&mut notes, at, NoteKind::Equivalent, "Isoloom adds its own router and controller".into());
            continue;
        }
        let name = label(raw_name);
        if &name != raw_name {
            note(
                &mut notes,
                at.clone(),
                NoteKind::Changed,
                format!("renamed `{name}` (machine names are DNS names)"),
            );
        }
        let vm = &node["children"]["vm"];
        let pick = |key: &str| setting(vm, key).or_else(|| setting(global, key));

        // The box: an OS name, plus the box itself when it isn't the one Isoloom would use.
        let boxname = text(pick("box")).unwrap_or_default();
        let windows_hint = text(pick("communicator")).as_deref() == Some("winrm") || text(pick("guest")).as_deref() == Some("windows");
        let (mut os, builtin) = os_for_box(&boxname);
        if windows_hint && !os.starts_with("windows") {
            os = "windows-server-2019";
        }
        let version = text(pick("box_version"));
        let image = if boxname.is_empty() {
            note(&mut notes, format!("{at}.vm.box"), NoteKind::Changed, format!("no box; assumed {os}"));
            None
        } else if builtin && version.is_none() {
            None
        } else {
            if !builtin {
                note(
                    &mut notes,
                    format!("{at}.vm.box"),
                    NoteKind::Changed,
                    format!("`{boxname}` kept as the machine's image; its OS read as {os}: check it"),
                );
            }
            Some((boxname.clone(), version))
        };

        let mut m = Machine {
            name: name.clone(),
            os: os.into(),
            image,
            networks: Vec::new(),
            publish: Vec::new(),
            cpus: None,
            memory_mb: None,
            provision: Vec::new(),
        };

        // Calls: the machine's own, after the global ones (Vagrant runs global provisioners first).
        let all_calls: Vec<&Value> = calls(global).filter(|c| c["name"] != "define").chain(calls(vm)).collect();
        for c in all_calls {
            let kind = c["name"].as_str().unwrap_or_default();
            let first = text(c["args"].get(0)).unwrap_or_default();
            let opts = c["args"].get(1).cloned().unwrap_or(Value::Null);
            let block = &c["block"];
            let opt = |k: &str| opts.get(k).filter(|v| !v.is_null()).or_else(|| setting(block, k));
            match (kind, first.as_str()) {
                ("network", "private_network") => {
                    if text(opt("type")).as_deref() == Some("dhcp") || opt("ip").is_none() {
                        note(
                            &mut notes,
                            format!("{at}.vm.network"),
                            NoteKind::Changed,
                            "a DHCP private network: give the machine a fixed address".into(),
                        );
                        continue;
                    }
                    let ip: std::net::Ipv4Addr = match text(opt("ip")).and_then(|i| i.parse().ok()) {
                        Some(ip) => ip,
                        None => continue,
                    };
                    let mask: u32 = text(opt("netmask"))
                        .and_then(|m| {
                            m.parse::<std::net::Ipv4Addr>()
                                .ok()
                                .map(|m| u32::from(m).count_ones())
                                .or_else(|| m.parse().ok())
                        })
                        .unwrap_or(24)
                        .clamp(24, 29);
                    let base = u32::from(ip) & (u32::MAX << (32 - mask));
                    let cidr = Cidr { base, len: mask as u8 };
                    let key = format!("{}/{mask}", std::net::Ipv4Addr::from(base));
                    let netname = nets
                        .entry(key.clone())
                        .or_insert_with(|| {
                            let named = text(opt("virtualbox__intnet")).map(|n| label(&n));
                            (cidr, named.unwrap_or_default())
                        })
                        .1
                        .clone();
                    m.networks.push((key, ip.octets()[3]));
                    let _ = netname;
                }
                ("network", "forwarded_port") => {
                    if opt("disabled").and_then(Value::as_bool) == Some(true) {
                        continue;
                    }
                    match (number(opt("guest")), number(opt("host"))) {
                        (Some(g), Some(h)) => m.publish.push((g as u16, h as u16)),
                        _ => note(
                            &mut notes,
                            format!("{at}.vm.network"),
                            NoteKind::Changed,
                            "a forwarded port without both ports: add it by hand".into(),
                        ),
                    }
                }
                ("network", "public_network") => note(
                    &mut notes,
                    format!("{at}.vm.network"),
                    NoteKind::NotYet,
                    "a network bridged to the outside (planned in the format)".into(),
                ),
                ("provider", p) => {
                    let b = block;
                    let cpus = number(setting(b, "cpus")).or_else(|| number(b["children"]["vmx"].get("settings").and_then(|s| s.get("numvcpus"))));
                    let mem = memory_mb(setting(b, "memory")).or_else(|| memory_mb(setting(b, "memsize")));
                    let smp = text(setting(b, "smp")).and_then(|s| s.split(',').find_map(|kv| kv.strip_prefix("cpus=").and_then(|n| n.parse().ok())));
                    m.cpus = m.cpus.or(cpus).or(smp);
                    m.memory_mb = m.memory_mb.or(mem);
                    if calls(b).any(|c| c["name"] == "customize") {
                        note(
                            &mut notes,
                            format!("{at}.vm.provider {p}"),
                            NoteKind::ByDesign,
                            "`customize`: raw commands for one hypervisor, which no other target understands".into(),
                        );
                    }
                }
                ("provision", "shell") => {
                    match (text(opt("path")), opt("inline")) {
                        (Some(path), _) if path.ends_with(".sh") || path.ends_with(".ps1") => m.provision.push(path.trim_start_matches("./").into()),
                        (Some(path), _) => note(
                            &mut notes,
                            format!("{at}.vm.provision"),
                            NoteKind::Changed,
                            format!("`{path}`: steps are .sh (Linux) or .ps1 (Windows) files"),
                        ),
                        (None, Some(_)) => note(
                            &mut notes,
                            format!("{at}.vm.provision"),
                            NoteKind::InImage,
                            "an inline script: put it in a file and list it in `vm.provision`".into(),
                        ),
                        _ => {}
                    }
                    // `reboot: true`: Vagrant restarts the guest after the script (if any).
                    if opt("reboot").and_then(Value::as_bool) == Some(true) {
                        if crate::images::is_windows(&m.os) {
                            note(
                                &mut notes,
                                format!("{at}.vm.provision"),
                                NoteKind::Changed,
                                "`reboot: true` on Windows: restart it from the environment's playbooks (`ansible.windows.win_reboot`)".into(),
                            );
                        } else {
                            m.provision.push(crate::model::REBOOT_STEP.into());
                        }
                    }
                }
                ("provision", "ansible_local") => match text(opt("playbook")) {
                    Some(p) => m.provision.push(p.trim_start_matches("./").into()),
                    None => note(
                        &mut notes,
                        format!("{at}.vm.provision"),
                        NoteKind::Changed,
                        "ansible_local without a playbook".into(),
                    ),
                },
                ("provision", "ansible") => {
                    if let Some(p) = text(opt("playbook")) {
                        if !env_playbooks.contains(&p) {
                            env_playbooks.push(p);
                        }
                    }
                }
                ("provision", "file") => note(
                    &mut notes,
                    format!("{at}.vm.provision"),
                    NoteKind::Equivalent,
                    "Isoloom copies the project into each Linux VM (/opt/isoloom)".into(),
                ),
                ("provision", other) => note(
                    &mut notes,
                    format!("{at}.vm.provision {other}"),
                    NoteKind::Equivalent,
                    "a `.sh` step can run any tool".into(),
                ),
                ("synced_folder", _) if opts.get("disabled").and_then(Value::as_bool) != Some(true) => {
                    note(
                        &mut notes,
                        format!("{at}.vm.synced_folder"),
                        NoteKind::Equivalent,
                        "no shared folders: Isoloom copies the project into each Linux VM".into(),
                    );
                }
                _ => {}
            }
        }
        if let Some(h) = text(pick("hostname")).filter(|h| h != raw_name && h != &name) {
            note(
                &mut notes,
                format!("{at}.vm.hostname"),
                NoteKind::Equivalent,
                format!("`{h}`: the hostname is the machine's name, `{name}`"),
            );
        }
        if m.networks.is_empty() {
            note(
                &mut notes,
                at.clone(),
                NoteKind::Changed,
                "no private network: put the machine on a network".into(),
            );
        }
        machines.push(m);
    }
    if machines.is_empty() {
        return Err("the Vagrantfile defines no machine".into());
    }
    if !env_playbooks.is_empty() {
        note(
            &mut notes,
            "provision ansible".into(),
            NoteKind::Changed,
            "host-side Ansible became environment-level `provision:`, run from a controller VM; check its inventory groups".into(),
        );
    }

    // Network names: the VirtualBox internal network's, else lab, lab-2...
    let mut taken = BTreeSet::new();
    let mut names: IndexMap<String, String> = IndexMap::new();
    for (i, (key, (_, named))) in nets.iter().enumerate() {
        let mut n = if named.is_empty() {
            if i == 0 { "lab".to_string() } else { format!("lab-{}", i + 1) }
        } else {
            named.trim_start_matches("isoloom-").to_string()
        };
        while !taken.insert(n.clone()) {
            n.push_str("-2");
        }
        names.insert(key.clone(), n);
    }

    let name = label(fallback_name);
    let mut y = String::new();
    let _ = writeln!(y, "{}", crate::schema::MODELINE);
    let _ = writeln!(
        y,
        "# Drafted by `isoloom import vagrant` from {source}. Review it with the notes the import printed."
    );
    y.push_str("# Next: add `docker:` to machines that can be containers, and `checks` that prove the behavior.\n");
    let _ = writeln!(y, "version: 1\nname: {name}\n\nnetworks:");
    for (key, n) in &names {
        let _ = writeln!(y, "  {n}: {{ cidr: {key} }}");
    }
    y.push_str("\nmachines:\n");
    for (i, m) in machines.iter().enumerate() {
        if i > 0 {
            y.push('\n');
        }
        let _ = writeln!(y, "  {}:", m.name);
        let nets = m.networks.iter().map(|(k, o)| format!("{}: {o}", names[k])).collect::<Vec<_>>().join(", ");
        let _ = writeln!(y, "    networks: {{ {nets} }}");
        if !m.publish.is_empty() {
            let svcs = m
                .publish
                .iter()
                .map(|(g, h)| format!("{{ port: {g}, publish: {h} }}"))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(y, "    services: [{svcs}]");
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
        if let Some((b, v)) = &m.image {
            let _ = writeln!(y, "      image:\n        vagrant: \"{b}\"");
            if let Some(v) = v {
                let _ = writeln!(y, "        vagrant_version: \"{v}\"");
            }
        }
        if !m.provision.is_empty() {
            let _ = writeln!(y, "      provision: [{}]", m.provision.join(", "));
        }
    }
    if !env_playbooks.is_empty() {
        y.push_str("\nprovision:\n");
        for p in &env_playbooks {
            let _ = writeln!(y, "  - ansible: {}", p.trim_start_matches("./"));
        }
    }
    Ok(Draft { yaml: y, notes })
}
