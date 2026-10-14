//! Drafts an `isoloom.yml` from a Vagrantfile, from what the Vagrantfile set when it ran against
//! a stand-in `Vagrant` module (see the CLI's `vagrant_record.rb`): every setting and call, with
//! loops and variables already evaluated. What the format expresses goes into the draft;
//! everything else becomes a note.

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use serde_json::Value;

use super::yaml::{DraftYaml, Header, ServiceEntry};
use super::{Draft, NoteKind, Notes, cidr_text, label, network_names};
use crate::validate::Cidr;

struct Machine {
    name: String,
    os: String,
    image: Option<(String, Option<String>)>,
    networks: Vec<(String, u8)>,
    /// (guest port, host port, UDP).
    publish: Vec<(u16, u16, bool)>,
    cpus: Option<u32>,
    memory_mb: Option<u32>,
    provision: Vec<String>,
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

/// The machines a recording defines, by name, with their configuration.
fn defined_machines(recorded: &Value) -> Vec<(String, Value)> {
    let defines: Vec<&Value> = calls(&recorded["children"]["vm"]).filter(|c| c["name"] == "define").collect();
    // Without `define`, the Vagrantfile is one machine: the top configuration itself.
    if defines.is_empty() {
        vec![("default".into(), recorded.clone())]
    } else {
        defines
            .iter()
            .map(|c| (text(c["args"].get(0)).unwrap_or_default(), c["block"].clone()))
            .collect()
    }
}

/// A recorded call (`config.vm.network "private_network", ip: ...`): its name, first argument,
/// options, and the block it was given.
struct Call<'a> {
    name: &'a str,
    first: String,
    opts: Value,
    block: &'a Value,
}

impl<'a> Call<'a> {
    fn new(c: &'a Value) -> Self {
        Call {
            name: c["name"].as_str().unwrap_or_default(),
            first: text(c["args"].get(0)).unwrap_or_default(),
            opts: c["args"].get(1).cloned().unwrap_or(Value::Null),
            block: &c["block"],
        }
    }

    /// An option, given as an argument or set in the block.
    fn opt(&self, k: &str) -> Option<&Value> {
        self.opts.get(k).filter(|v| !v.is_null()).or_else(|| setting(self.block, k))
    }
}

/// What the import collects across the machines.
struct Import<'a> {
    /// The top configuration's `vm`: what every machine inherits.
    global: &'a Value,
    notes: Notes,
    /// Private networks by block (`192.168.33.0/24`), with the name the Vagrantfile gave them
    /// (the VirtualBox internal network's), if any.
    nets: IndexMap<String, String>,
    /// Host-side Ansible playbooks: run once for the environment.
    env_playbooks: Vec<String>,
}

/// Drafts a spec from a recorded Vagrantfile (`recorded` is the recorder's JSON).
pub fn draft(recorded: &Value, fallback_name: &str, source: &str) -> Result<Draft, String> {
    let mut import = Import {
        global: &recorded["children"]["vm"],
        notes: Notes::default(),
        nets: IndexMap::new(),
        env_playbooks: Vec::new(),
    };
    let machines: Vec<Machine> = defined_machines(recorded)
        .iter()
        .filter_map(|(raw_name, node)| import.machine(raw_name, node))
        .collect();
    if machines.is_empty() {
        return Err("the Vagrantfile defines no machine".into());
    }
    if !import.env_playbooks.is_empty() {
        import.notes.add(
            "provision ansible",
            NoteKind::Changed,
            "host-side Ansible became environment-level `provision:`, run from a controller VM; check its inventory groups",
        );
    }
    let yaml = import.write(&machines, &label(fallback_name), source);
    Ok(import.notes.draft(yaml))
}

impl<'a> Import<'a> {
    /// One defined machine, or none for Isoloom's own (its router and controller).
    fn machine(&mut self, raw_name: &str, node: &Value) -> Option<Machine> {
        let at = format!("define {raw_name}");
        if raw_name.starts_with("isoloom-") {
            self.notes.add(at, NoteKind::Equivalent, "Isoloom adds its own router and controller");
            return None;
        }
        let name = label(raw_name);
        if name != raw_name {
            self.notes.add(at.clone(), NoteKind::Changed, format!("renamed `{name}` (machine names are DNS names)"));
        }
        let vm = &node["children"]["vm"];
        let (os, image) = self.os_and_image(&at, vm);
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
        let global = self.global;
        for c in calls(global).filter(|c| c["name"] != "define").chain(calls(vm)) {
            self.call(&at, &mut m, &Call::new(c));
        }
        if let Some(h) = text(self.pick(vm, "hostname")).filter(|h| h != raw_name && h != &name) {
            self.notes.add(
                format!("{at}.vm.hostname"),
                NoteKind::Equivalent,
                format!("`{h}`: the hostname is the machine's name, `{name}`"),
            );
        }
        if m.networks.is_empty() {
            self.notes.add(at, NoteKind::Changed, "no private network: put the machine on a network");
        }
        Some(m)
    }

    /// A setting of the machine's `vm`, else the top configuration's.
    fn pick<'v>(&self, vm: &'v Value, key: &str) -> Option<&'v Value>
    where
        'a: 'v,
    {
        setting(vm, key).or_else(|| setting(self.global, key))
    }

    /// The box: an OS name, plus the box itself when it isn't the one Isoloom would use.
    fn os_and_image(&mut self, at: &str, vm: &Value) -> (&'static str, Option<(String, Option<String>)>) {
        let boxname = text(self.pick(vm, "box")).unwrap_or_default();
        let windows_hint =
            text(self.pick(vm, "communicator")).as_deref() == Some("winrm") || text(self.pick(vm, "guest")).as_deref() == Some("windows");
        let (mut os, builtin) = os_for_box(&boxname);
        if windows_hint && !os.starts_with("windows") {
            os = "windows-server-2019";
        }
        let version = text(self.pick(vm, "box_version"));
        let image = if boxname.is_empty() {
            self.notes.add(format!("{at}.vm.box"), NoteKind::Changed, format!("no box; assumed {os}"));
            None
        } else if builtin && version.is_none() {
            None
        } else {
            if !builtin {
                self.notes.add(
                    format!("{at}.vm.box"),
                    NoteKind::Changed,
                    format!("`{boxname}` kept as the machine's image; its OS read as {os}: check it"),
                );
            }
            Some((boxname, version))
        };
        (os, image)
    }

    /// What one call (`vm.network`, `vm.provider`, `vm.provision`...) adds to the machine.
    fn call(&mut self, at: &str, m: &mut Machine, c: &Call) {
        match (c.name, c.first.as_str()) {
            ("network", "private_network") => self.private_network(at, m, c),
            ("network", "forwarded_port") => self.forwarded_port(at, m, c),
            ("network", "public_network") => self.notes.add(
                format!("{at}.vm.network"),
                NoteKind::NotYet,
                "a network bridged to the outside (planned in the format)",
            ),
            ("provider", p) => self.provider(&format!("{at}.vm.provider {p}"), m, c.block),
            ("provision", _) => self.provision(at, m, c),
            ("synced_folder", _) if c.opts.get("disabled").and_then(Value::as_bool) != Some(true) => {
                self.notes.add(
                    format!("{at}.vm.synced_folder"),
                    NoteKind::Equivalent,
                    "no shared folders: Isoloom copies the project into each Linux VM",
                );
            }
            _ => {}
        }
    }

    /// A private network with a fixed address: the machine joins its block.
    fn private_network(&mut self, at: &str, m: &mut Machine, c: &Call) {
        if text(c.opt("type")).as_deref() == Some("dhcp") || c.opt("ip").is_none() {
            self.notes.add(
                format!("{at}.vm.network"),
                NoteKind::Changed,
                "a DHCP private network: give the machine a fixed address",
            );
            return;
        }
        let Some(ip) = text(c.opt("ip")).and_then(|i| i.parse::<Ipv4Addr>().ok()) else {
            return;
        };
        let mask: u32 = text(c.opt("netmask"))
            .and_then(|m| m.parse::<Ipv4Addr>().ok().map(|m| u32::from(m).count_ones()).or_else(|| m.parse().ok()))
            .unwrap_or(24)
            .clamp(24, 29);
        let base = u32::from(ip) & (u32::MAX << (32 - mask));
        let key = cidr_text(Cidr { base, len: mask as u8 });
        self.nets
            .entry(key.clone())
            .or_insert_with(|| text(c.opt("virtualbox__intnet")).map(|n| label(&n)).unwrap_or_default());
        m.networks.push((key, ip.octets()[3]));
    }

    fn forwarded_port(&mut self, at: &str, m: &mut Machine, c: &Call) {
        if c.opt("disabled").and_then(Value::as_bool) == Some(true) {
            return;
        }
        match (number(c.opt("guest")), number(c.opt("host"))) {
            (Some(g), Some(h)) => {
                let udp = text(c.opt("protocol")).is_some_and(|p| p.eq_ignore_ascii_case("udp"));
                m.publish.push((g as u16, h as u16, udp))
            }
            _ => self.notes.add(
                format!("{at}.vm.network"),
                NoteKind::Changed,
                "a forwarded port without both ports: add it by hand",
            ),
        }
    }

    /// A provider block: the CPUs and memory it gives the machine, whichever hypervisor it's for.
    fn provider(&mut self, at: &str, m: &mut Machine, b: &Value) {
        let cpus = number(setting(b, "cpus")).or_else(|| number(b["children"]["vmx"].get("settings").and_then(|s| s.get("numvcpus"))));
        let mem = memory_mb(setting(b, "memory")).or_else(|| memory_mb(setting(b, "memsize")));
        let smp = text(setting(b, "smp")).and_then(|s| s.split(',').find_map(|kv| kv.strip_prefix("cpus=").and_then(|n| n.parse().ok())));
        m.cpus = m.cpus.or(cpus).or(smp);
        m.memory_mb = m.memory_mb.or(mem);
        if calls(b).any(|c| c["name"] == "customize") {
            self.notes.add(
                at,
                NoteKind::ByDesign,
                "`customize`: raw commands for one hypervisor, which no other target understands",
            );
        }
    }

    fn provision(&mut self, at: &str, m: &mut Machine, c: &Call) {
        let at = format!("{at}.vm.provision");
        match c.first.as_str() {
            "shell" => self.provision_shell(&at, m, c),
            "ansible_local" => match text(c.opt("playbook")) {
                Some(p) => m.provision.push(p.trim_start_matches("./").into()),
                None => self.notes.add(at, NoteKind::Changed, "ansible_local without a playbook"),
            },
            "ansible" => {
                if let Some(p) = text(c.opt("playbook"))
                    && !self.env_playbooks.contains(&p)
                {
                    self.env_playbooks.push(p);
                }
            }
            "file" => self.notes.add(
                at,
                NoteKind::Equivalent,
                "Isoloom copies the project into each Linux VM (/opt/isoloom)",
            ),
            other => self.notes.add(format!("{at} {other}"), NoteKind::Equivalent, "a `.sh` step can run any tool"),
        }
    }

    fn provision_shell(&mut self, at: &str, m: &mut Machine, c: &Call) {
        match (text(c.opt("path")), c.opt("inline")) {
            (Some(path), _) if path.ends_with(".sh") || path.ends_with(".ps1") => m.provision.push(path.trim_start_matches("./").into()),
            (Some(path), _) => self.notes.add(
                at,
                NoteKind::Changed,
                format!("`{path}`: steps are .sh (Linux) or .ps1 (Windows) files"),
            ),
            (None, Some(_)) => self.notes.add(
                at,
                NoteKind::InImage,
                "an inline script: put it in a file and list it in `vm.provision`",
            ),
            _ => {}
        }
        // `reboot: true`: Vagrant restarts the guest after the script (if any).
        if c.opt("reboot").and_then(Value::as_bool) == Some(true) {
            if crate::images::is_windows(&m.os) {
                self.notes.add(
                    at,
                    NoteKind::Changed,
                    "`reboot: true` on Windows: restart it from the environment's playbooks (`ansible.windows.win_reboot`)",
                );
            } else {
                m.provision.push(crate::model::REBOOT_STEP.into());
            }
        }
    }

    fn write(&self, machines: &[Machine], name: &str, source: &str) -> String {
        // Network names: the VirtualBox internal network's, else lab, lab-2...
        let names = network_names(
            self.nets
                .iter()
                .map(|(key, named)| (key.clone(), (!named.is_empty()).then(|| named.trim_start_matches("isoloom-").to_string()))),
        );
        let mut y = DraftYaml::new(&Header {
            tool: "vagrant",
            source,
            review: "Review it with the notes the import printed.",
            next: "add `docker:` to machines that can be containers, and `checks` that prove the behavior.",
            name,
        });
        for (key, n) in &names {
            y.network(n, 0, key, true);
        }
        y.machines();
        for m in machines {
            y.machine(&m.name);
            y.machine_networks(m.networks.iter().map(|(k, o)| (&names[k], *o)));
            y.services(m.publish.iter().map(|&(port, host, udp)| ServiceEntry {
                port,
                udp,
                publish: Some(host),
            }));
            y.resources(m.cpus, m.memory_mb);
            y.vm_os(&m.os);
            if let Some((b, v)) = &m.image {
                y.line(format!("      image:\n        vagrant: \"{b}\""));
                if let Some(v) = v {
                    y.line(format!("        vagrant_version: \"{v}\""));
                }
            }
            if !m.provision.is_empty() {
                y.line(format!("      provision: [{}]", m.provision.join(", ")));
            }
        }
        if !self.env_playbooks.is_empty() {
            y.line("\nprovision:");
            for p in &self.env_playbooks {
                y.line(format!("  - ansible: {}", p.trim_start_matches("./")));
            }
        }
        y.finish()
    }
}
