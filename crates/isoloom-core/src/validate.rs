//! Checks a spec for mistakes the generators would turn into broken files. Every problem
//! names the field it's about (`machines.web.networks.dmz`) and says what to do, so an
//! author (human or AI) can fix it without reading the generators.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::net::Ipv4Addr;
use std::path::Path;

use crate::model::{KNOWN_OS, Shape, Spec};
use crate::targets::{derive, missing};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Where in the file, e.g. `machines.web.depends_on`.
    pub at: String,
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.at, self.message)
    }
}

/// An IPv4 block, parsed from `a.b.c.d/len`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    pub base: u32,
    pub len: u8,
}

impl Cidr {
    pub fn parse(s: &str) -> Option<Cidr> {
        let (ip, len) = s.split_once('/')?;
        let ip: Ipv4Addr = ip.parse().ok()?;
        let len: u8 = len.parse().ok()?;
        if len > 32 {
            return None;
        }
        let mask = Self::mask(len);
        let base = u32::from(ip);
        // The address must be the network address itself (10.20.0.0/24, not 10.20.0.5/24).
        (base & mask == base).then_some(Cidr { base, len })
    }

    fn mask(len: u8) -> u32 {
        if len == 0 { 0 } else { u32::MAX << (32 - len) }
    }

    pub fn contains(self, other: Cidr) -> bool {
        self.len <= other.len && other.base & Self::mask(self.len) == self.base
    }

    pub fn overlaps(self, other: Cidr) -> bool {
        self.contains(other) || other.contains(self)
    }

    /// The address with this last octet, if it is a usable host address in the block.
    /// Reserved on every target: `.1` (gateway) and the two last usable addresses (controller, router).
    pub fn host(self, last_octet: u8) -> Option<Ipv4Addr> {
        if self.len < 24 || self.len > 29 {
            return None;
        }
        let size = 1u32 << (32 - self.len);
        let offset = u32::from(last_octet) & 0xff;
        let addr = (self.base & !0xff) | offset;
        let first = self.base + 2; // .0 network, .1 gateway
        let last = self.base + size - 4; // the controller, the router (last usable) and broadcast excluded
        (addr >= first && addr <= last).then(|| Ipv4Addr::from(addr))
    }

    /// The gateway's address: the first usable address of the block (e.g. .1 in a /24).
    pub fn gateway(self) -> Ipv4Addr {
        Ipv4Addr::from(self.base + 1)
    }

    /// The gateway's last octet (1 in a /24, 9 in 10.0.0.8/29).
    pub fn gateway_octet(self) -> u8 {
        ((self.base + 1) & 0xff) as u8
    }

    /// The controller's address: the second-to-last usable one (e.g. .253 in a /24), where
    /// environment-level provisioning runs from.
    pub fn controller(self) -> Ipv4Addr {
        Ipv4Addr::from(self.base + (1u32 << (32 - self.len)) - 3)
    }

    /// The address of the `i`th tool (0-based): just below the controller (.252, .251, .250 in
    /// a /24), reserved for tools when the spec has any.
    pub fn tool(self, i: usize) -> Ipv4Addr {
        Ipv4Addr::from(self.base + (1u32 << (32 - self.len)) - 4 - i as u32)
    }

    /// The router's address: the last usable address of the block (e.g. .254 in a /24).
    pub fn router(self) -> Ipv4Addr {
        Ipv4Addr::from(self.base + (1u32 << (32 - self.len)) - 2)
    }
}

fn kebab(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 63
        && s.split('-')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

/// Machine and network names become DNS names (machines resolve each other by name).
fn dns_label(s: &str) -> bool {
    kebab(s) && !s.starts_with(|c: char| c.is_ascii_digit())
}

fn input_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !s.starts_with(|c: char| c.is_ascii_digit())
}

/// The private ranges a network can use (RFC 1918).
pub const PRIVATE: [Cidr; 3] = [
    Cidr { base: 0x0a00_0000, len: 8 },
    Cidr { base: 0xac10_0000, len: 12 },
    Cidr { base: 0xc0a8_0000, len: 16 },
];

/// Where the Docker target keeps its networks: 172.16.0.0/12 and 192.168.0.0/16 clash with
/// Docker's own pools, Docker Desktop's network and home LANs.
pub const DOCKER_BLOCK: Cidr = Cidr { base: 0x0a00_0000, len: 8 };

/// Checks the spec itself (no file system access). See [`validate_files`] for paths.
pub fn validate(spec: &Spec) -> Vec<Problem> {
    let mut p = Vec::new();
    let mut add = |at: &str, message: String| p.push(Problem { at: at.to_string(), message });

    if spec.version != 1 {
        add("version", format!("unsupported version {}; this isoloom reads version 1", spec.version));
    }
    if !kebab(&spec.name) {
        add("name", "use kebab-case: lowercase letters, digits and dashes (e.g. supplier-portal-api)".into());
    } else if spec.name.len() > 55 {
        // The name becomes the Kubernetes namespace `isoloom-<name>`, which must be a 63-character
        // DNS label; 8 characters are the prefix.
        add("name", "at most 55 characters: it becomes the Kubernetes namespace `isoloom-<name>`".into());
    }

    // Networks: valid, private, sized /24 to /29, not overlapping each other.
    let mut cidrs: Vec<(String, Cidr)> = Vec::new();
    if spec.networks.is_empty() {
        add("networks", "declare at least one network".into());
    }
    for (name, net) in &spec.networks {
        let at = format!("networks.{name}");
        if !dns_label(name) {
            add(&at, "network names are kebab-case and start with a letter".into());
        }
        match Cidr::parse(&net.cidr) {
            None => add(&format!("{at}.cidr"), format!("`{}` isn't a network address like 10.20.0.0/24", net.cidr)),
            Some(c) => {
                if !PRIVATE.iter().any(|r| r.contains(c)) {
                    add(
                        &format!("{at}.cidr"),
                        "use a private block: inside 10.0.0.0/8, 172.16.0.0/12 or 192.168.0.0/16".into(),
                    );
                } else if c.len < 24 || c.len > 29 {
                    add(&format!("{at}.cidr"), "use a /24 to /29 block".into());
                }
                for (other, oc) in &cidrs {
                    if c.overlaps(*oc) {
                        add(&format!("{at}.cidr"), format!("overlaps network `{other}`"));
                    }
                }
                if let Some(d) = &net.docker {
                    match Cidr::parse(&d.cidr) {
                        Some(dc) if dc.len == c.len && DOCKER_BLOCK.contains(dc) => {}
                        Some(_) => add(
                            &format!("{at}.docker.cidr"),
                            format!(
                                "use a /{} block inside 10.0.0.0/8 (the same size as `cidr`, so addresses keep their last octet)",
                                c.len
                            ),
                        ),
                        None => add(&format!("{at}.docker.cidr"), format!("`{}` isn't a network address like 10.20.0.0/24", d.cidr)),
                    }
                }
                cidrs.push((name.clone(), c));
            }
        }
    }
    let cidr_of: HashMap<&str, Cidr> = cidrs.iter().map(|(n, c)| (n.as_str(), *c)).collect();

    for (name, net) in &spec.networks {
        let Some(gw) = &net.gateway else { continue };
        let at = format!("networks.{name}.gateway");
        match spec.machines.get(gw) {
            None => add(&at, format!("no machine named `{gw}`")),
            Some(m) if !m.networks.contains_key(name) => {
                let octet = cidr_of.get(name.as_str()).map(|c| c.gateway_octet()).unwrap_or(1);
                add(
                    &at,
                    format!("attach `{gw}` to `{name}` at the gateway address: `networks: {{ {name}: {octet} }}`"),
                );
            }
            Some(_) => {}
        }
    }

    for (i, r) in spec.reach.iter().enumerate() {
        for (field, net) in [("from", &r.from), ("to", &r.to)] {
            if !spec.networks.contains_key(net) {
                add(&format!("reach[{i}].{field}"), format!("no network named `{net}`"));
            }
        }
        if r.from == r.to {
            add(
                &format!("reach[{i}]"),
                "machines on the same network always reach each other; remove this rule".into(),
            );
        }
        if r.ports.contains(&0) {
            add(&format!("reach[{i}].ports"), "port 0 isn't a port".into());
        }
    }

    for (i, input) in spec.inputs.iter().enumerate() {
        if !input_name(input) {
            add(
                &format!("inputs[{i}]"),
                format!("`{input}`: inputs are environment variable names (letters, digits and `_`, not starting with a digit)"),
            );
        }
    }

    // Machines.
    if spec.machines.is_empty() {
        add("machines", "declare at least one machine".into());
    }
    let mut taken: HashMap<(String, u8), String> = HashMap::new();
    let mut published: HashMap<u16, String> = HashMap::new();
    let access: Vec<&String> = spec.machines.iter().filter(|(_, m)| m.access).map(|(n, _)| n).collect();
    if access.len() > 1 {
        add("machines", format!("only one machine can be the access machine (found {})", access.len()));
    }
    // Network appliances (`docker.appliance`): Docker machines Isoloom wires as their image expects.
    let mgmt = Cidr::parse(crate::generate::APPLIANCE_MGMT_CIDR).expect("constant");
    let any_appliance = spec.machines.values().any(|m| m.docker.as_ref().is_some_and(|d| d.appliance.is_some()));
    for (name, m) in &spec.machines {
        let Some(d) = m.docker.as_ref().filter(|d| d.appliance.is_some()) else {
            continue;
        };
        let at = format!("machines.{name}");
        if m.vm.is_some() {
            add(
                &format!("{at}.vm"),
                "a network appliance runs on the Docker targets only: leave `vm` out".into(),
            );
        }
        if m.access {
            add(&format!("{at}.access"), "a network appliance can't be the access machine".into());
        }
        if d.idle || !d.init.is_empty() {
            add(&format!("{at}.docker"), "a network appliance runs its own OS: no `idle` or `init`".into());
        }
        if let Some(c) = &d.config
            && (c.starts_with('/') || c.split('/').any(|p| p == ".."))
        {
            add(&format!("{at}.docker.config"), "a path in the project".into());
        }
    }
    if any_appliance {
        for (net, n) in &spec.networks {
            if Cidr::parse(&n.cidr).is_some_and(|c| c.overlaps(mgmt)) {
                add(
                    &format!("networks.{net}.cidr"),
                    format!("overlaps {}, the network appliances' management network", crate::generate::APPLIANCE_MGMT_CIDR),
                );
            }
        }
    }
    for (name, m) in &spec.machines {
        let at = format!("machines.{name}");
        if !dns_label(name) {
            add(&at, "machine names are DNS names: kebab-case, starting with a letter".into());
        } else if name.len() > 53 {
            // The machine's published entry point is the Kubernetes Service `<name>-published`,
            // a 63-character DNS label; 10 characters are the suffix.
            add(&at, "at most 53 characters: it becomes the Kubernetes Service `<name>-published`".into());
        }
        if m.networks.is_empty() {
            add(&format!("{at}.networks"), "attach the machine to at least one network".into());
        }
        for (net, octet) in &m.networks {
            let nat = format!("{at}.networks.{net}");
            let Some(c) = cidr_of.get(net.as_str()) else {
                add(&nat, format!("no network named `{net}`"));
                continue;
            };
            let is_gateway = spec.networks[net].gateway.as_deref() == Some(name.as_str());
            if is_gateway {
                if *octet != c.gateway_octet() {
                    add(
                        &nat,
                        format!("`{name}` is this network's gateway, so it takes the gateway address: use {}", c.gateway_octet()),
                    );
                } else {
                    taken.insert((net.clone(), *octet), name.clone());
                }
            } else if c.host(*octet).is_none() {
                add(
                    &nat,
                    format!(
                        "{octet} isn't a usable address in {} (reserved: .0, .1 for the gateway, the two last addresses for the controller and the router, and the broadcast address)",
                        spec.networks[net].cidr
                    ),
                );
            } else if let Some(other) = taken.insert((net.clone(), *octet), name.clone()) {
                add(&nat, format!("address .{octet} is already used by `{other}`"));
            }
        }
        let mut paths = BTreeSet::new();
        for (v, path) in &m.volumes {
            let vat = format!("{at}.volumes.{v}");
            if !kebab(v) {
                add(&vat, "volume names are kebab-case: lowercase letters, digits and dashes".into());
            } else if name.len() + 1 + v.len() > 63 {
                // The volume's Kubernetes claim is named `<machine>-<volume>`, a 63-character label.
                add(
                    &vat,
                    "machine name and volume name together are at most 62 characters: the Kubernetes claim is `<machine>-<volume>`".into(),
                );
            }
            // No spaces or ':' : the Docker short mount syntax (`<vol>:<path>`) and the `mkdir -p`
            // the VM/Proxmox provisioners run both split on those, so either would corrupt the mount.
            if !path.starts_with('/') || path == "/" || path.split('/').any(|s| s == "..") || path.contains([':', ' ', '\t']) {
                add(
                    &vat,
                    format!("`{path}` isn't a safe absolute path inside the machine (no spaces or ':'), like /var/lib/postgresql/data"),
                );
            } else if !paths.insert(path.trim_end_matches('/')) {
                add(&vat, format!("`{path}` is already a volume of this machine"));
            }
        }
        let mut ports = BTreeSet::new();
        for (i, s) in m.services.iter().enumerate() {
            if s.port == 0 {
                add(&format!("{at}.services[{i}].port"), "port 0 isn't a port".into());
            } else if !ports.insert(s.port) {
                add(&format!("{at}.services[{i}].port"), format!("port {} is listed twice", s.port));
            }
            match s.publish {
                Some(0) => add(&format!("{at}.services[{i}].publish"), "port 0 isn't a port".into()),
                Some(p) => {
                    if let Some(other) = published.insert(p, name.clone()) {
                        add(&format!("{at}.services[{i}].publish"), format!("port {p} is already published by `{other}`"));
                    }
                }
                None if s.fixed => add(&format!("{at}.services[{i}].fixed"), "`fixed` keeps the `publish` port: give one".into()),
                None => {}
            }
        }
        for (i, input) in m.inputs.iter().enumerate() {
            if !spec.inputs.contains(input) {
                add(&format!("{at}.inputs[{i}]"), format!("`{input}` isn't declared in the spec's `inputs`"));
            }
        }
        for (i, dep) in m.depends_on.iter().enumerate() {
            if dep == name {
                add(&format!("{at}.depends_on[{i}]"), "a machine can't depend on itself".into());
            } else if !spec.machines.contains_key(dep) {
                add(&format!("{at}.depends_on[{i}]"), format!("no machine named `{dep}`"));
            } else if spec.machines[dep].services.is_empty() {
                add(
                    &format!("{at}.depends_on[{i}]"),
                    format!("`{dep}` has no services, so there's nothing to wait for; give it a service"),
                );
            }
        }
        if let Some(r) = m.resources {
            // A VM needs room to boot; a container only what its process uses (a Go service
            // capped at 64 MB is common), so the VM floor applies to machines that can be VMs.
            if m.vm.is_some() {
                if r.cpus == Some(0) || r.memory_mb.is_some_and(|v| v < 256) || r.disk_gb.is_some_and(|v| v < 5) {
                    add(&format!("{at}.resources"), "too small for a VM: at least 1 cpu, 256 MB and 5 GB".into());
                }
            } else if r.cpus == Some(0) || r.memory_mb.is_some_and(|v| v < 16) {
                add(&format!("{at}.resources"), "too small: at least 1 cpu and 16 MB".into());
            }
        }
        for (i, a) in m.aliases.iter().enumerate() {
            let ok = a.len() <= 253
                && a.split('.').all(|l| {
                    !l.is_empty()
                        && l.len() <= 63
                        && l.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                        && !l.starts_with('-')
                        && !l.ends_with('-')
                });
            if !ok {
                add(
                    &format!("{at}.aliases[{i}]"),
                    format!("`{a}` isn't a DNS name like api.example.com (lowercase)"),
                );
            } else if spec.machines.contains_key(a) || spec.machines.iter().any(|(o, om)| o != name && om.aliases.contains(a)) {
                add(&format!("{at}.aliases[{i}]"), format!("`{a}` already names another machine"));
            }
        }
        if !m.aliases.is_empty() && m.count.is_some() {
            add(
                &format!("{at}.aliases"),
                "clones (`count`) can't share names: give aliases to single machines".into(),
            );
        }
        if m.docker.is_none() && m.vm.is_none() && !m.access {
            add(&at, "give the machine at least one implementation: `docker:` and/or `vm:`".into());
        }
        if let Some(d) = &m.docker {
            // A Dynamips router: Isoloom builds the emulator; the IOS image is the firmware.
            let emulated = d.appliance == Some(crate::model::Appliance::CiscoDynamips);
            if emulated {
                if d.image.is_some() || d.build.is_some() {
                    add(
                        &format!("{at}.docker"),
                        "a Dynamips router's container is Isoloom's: give its IOS image as `firmware`, not `image` or `build`".into(),
                    );
                }
                if d.firmware.is_none() {
                    add(&format!("{at}.docker.firmware"), "the IOS image (.bin) to boot, a file in the project".into());
                }
            } else if d.firmware.is_some() {
                add(
                    &format!("{at}.docker.firmware"),
                    "only a Dynamips router (`appliance: cisco-dynamips`) boots a firmware".into(),
                );
            }
            match (&d.image, &d.build) {
                _ if emulated => {}
                (Some(_), Some(_)) => add(&format!("{at}.docker"), "use `image` or `build`, not both".into()),
                (None, None) => add(
                    &format!("{at}.docker"),
                    "set `image` (a published image) or `build` (a folder in the project)".into(),
                ),
                _ => {}
            }
            if d.build.is_none() && (d.dockerfile.is_some() || !d.args.is_empty()) {
                add(&format!("{at}.docker"), "`dockerfile` and `args` go with `build` (the context)".into());
            }
        }
        if let Some(v) = &m.vm {
            if v.os.is_empty() {
                add(
                    &format!("{at}.vm.os"),
                    format!("name the machine's OS (or share one through `common:` or a group): {}", KNOWN_OS.join(", ")),
                );
            } else if !KNOWN_OS.contains(&v.os.as_str()) {
                add(&format!("{at}.vm.os"), format!("unknown OS `{}`; use one of: {}", v.os, KNOWN_OS.join(", ")));
            }
            if v.provision.is_empty() && !m.access && spec.provision.is_empty() {
                add(
                    &format!("{at}.vm.provision"),
                    "list the steps that install the machine's services (or provision the environment with `provision:`)".into(),
                );
            }
        }
    }
    if let Some(cycle) = dependency_cycle(spec) {
        let hint = if spec.networks.values().any(|n| n.gateway.is_some()) {
            " (machines on a gateway's network also start after it)"
        } else {
            ""
        };
        add("machines", format!("depends_on forms a cycle: {}{hint}", cycle.join(" -> ")));
    }

    // Environment-level provisioning: its groups name machines that have a VM form.
    for (i, step) in spec.provision.iter().enumerate() {
        if !(step.ansible.ends_with(".yml") || step.ansible.ends_with(".yaml")) {
            add(&format!("provision[{i}].ansible"), "an Ansible playbook: a .yml or .yaml file".into());
        }
        for (group, members) in &step.groups {
            if matches!(group.as_str(), "all" | "linux" | "windows") {
                add(&format!("provision[{i}].groups.{group}"), "Isoloom fills this group itself".into());
            }
            for (j, member) in members.iter().enumerate() {
                if !spec.machines.contains_key(member) {
                    add(&format!("provision[{i}].groups.{group}[{j}]"), format!("no machine named `{member}`"));
                }
            }
        }
        for (machine, vars) in &step.host_vars {
            if !spec.machines.contains_key(machine) {
                add(&format!("provision[{i}].host_vars.{machine}"), format!("no machine named `{machine}`"));
            }
            for k in vars.keys() {
                let ident = k.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !ident {
                    add(
                        &format!("provision[{i}].host_vars.{machine}.{k}"),
                        "a variable name: letters, digits and `_`, not starting with a digit".into(),
                    );
                }
            }
        }
    }

    // Tools take the addresses just below the controller on every network: no machine there.
    if !spec.tools.is_empty() {
        for (mname, m) in &spec.machines {
            for (net, octet) in &m.networks {
                if let Some(c) = cidr_of.get(net.as_str())
                    && (0..spec.tools.len()).any(|i| c.tool(i).octets()[3] == *octet)
                {
                    add(
                        &format!("machines.{mname}.networks.{net}"),
                        format!(
                            ".{octet} is reserved for the tools (the {} addresses below .{}) when `tools:` is set",
                            spec.tools.len(),
                            c.controller().octets()[3]
                        ),
                    );
                }
            }
        }
    }

    // Tools: names, a recipe or an image, ports.
    for (name, t) in &spec.tools {
        let at = format!("tools.{name}");
        if !dns_label(name) {
            add(&at, "tool names are kebab-case and start with a letter".into());
        }
        if spec.machines.contains_key(name) || spec.groups.contains_key(name) {
            add(&at, format!("`{name}` is already a machine's or a group's name"));
        }
        let recipe = crate::model::TOOL_RECIPES.contains(&name.as_str());
        if !recipe && t.image.is_none() {
            add(
                &format!("{at}.image"),
                format!("give a container image, or use a recipe: {}", crate::model::TOOL_RECIPES.join(", ")),
            );
        }
        if recipe && t.image.is_some() {
            add(
                &format!("{at}.image"),
                format!("`{name}` is a recipe with its own image; leave `image` out or choose another name"),
            );
        }
        if t.publish.is_some() && t.port.is_none() {
            add(&format!("{at}.publish"), "say which `port` the tool serves".into());
        }
        if let Some(p) = t.publish
            && spec.machines.values().any(|m| m.services.iter().any(|s| s.publish == Some(p)))
        {
            add(&format!("{at}.publish"), format!("host port {p} is already published by a machine"));
        }
        if spec.tools.len() > 3 {
            add(
                "tools",
                "at most 3 tools (each takes one of the addresses Isoloom reserves on every network)".into(),
            );
            break;
        }
    }

    // External machines: an address, a sane port, a user name.
    for (name, m) in &spec.machines {
        let Some(e) = &m.external else { continue };
        let at = format!("machines.{name}.external");
        if e.address.trim().is_empty() || e.address.chars().any(|c| c.is_whitespace() || c == '@' || c == '\'' || c == '"') {
            add(
                &format!("{at}.address"),
                "an address or hostname reachable from here, like 192.168.1.20 or dc01.lab.local".into(),
            );
        }
        if e.port == Some(0) {
            add(&format!("{at}.port"), "1 to 65535".into());
        }
        if e.user.as_deref().is_some_and(|u| u.trim().is_empty() || u.contains(['@', ' '])) {
            add(&format!("{at}.user"), "a user name".into());
        }
    }

    // Link impairment: well-formed. It applies on the machines' interfaces and the router's.
    for (name, net) in &spec.networks {
        let Some(tc) = &net.tc else { continue };
        let at = format!("networks.{name}.tc");
        let time_ok = |v: &str| {
            let digits = v.trim_end_matches(|c: char| c.is_ascii_alphabetic());
            let unit = &v[digits.len()..];
            !digits.is_empty() && digits.parse::<f64>().is_ok() && ["ms", "s", "us"].contains(&unit)
        };
        if tc.delay.is_none() && tc.jitter.is_none() && tc.loss.is_none() && tc.rate.is_none() {
            add(&at, "say what to impair: `delay`, `jitter`, `loss` or `rate`".into());
        }
        if let Some(d) = &tc.delay
            && !time_ok(d)
        {
            add(&format!("{at}.delay"), format!("`{d}` isn't a time like 50ms or 1s"));
        }
        if let Some(j) = &tc.jitter {
            if !time_ok(j) {
                add(&format!("{at}.jitter"), format!("`{j}` isn't a time like 5ms"));
            }
            if tc.delay.is_none() {
                add(&format!("{at}.jitter"), "jitter varies a `delay`; set one".into());
            }
        }
        if let Some(l) = tc.loss
            && !(0.0..=100.0).contains(&l)
        {
            add(&format!("{at}.loss"), "a percentage, 0 to 100".into());
        }
        if let Some(r) = &tc.rate {
            let digits = r.trim_end_matches(|c: char| c.is_ascii_alphabetic());
            let unit = &r[digits.len()..];
            if digits.is_empty() || digits.parse::<f64>().is_err() || !["bit", "kbit", "mbit", "gbit", "bps", "kbps", "mbps", "gbps"].contains(&unit) {
                add(&format!("{at}.rate"), format!("`{r}` isn't a rate like 10mbit or 512kbit"));
            }
        }
        if net.gateway.is_some() {
            add(
                &at,
                format!("`{name}` is routed by its gateway machine, which owns its link; `tc` applies on Isoloom's router"),
            );
        }
    }

    // The message: balanced placeholders with a path inside.
    if let Some(m) = &spec.message {
        let mut rest = m.as_str();
        while let Some(i) = rest.find("{{") {
            match rest[i + 2..].find("}}") {
                None => {
                    add("message", "a `{{` without its `}}`".into());
                    break;
                }
                Some(j) => {
                    let path = rest[i + 2..i + 2 + j].trim();
                    if path.is_empty() || path.split('.').any(|p| p.is_empty()) {
                        add("message", format!("`{{{{ {path} }}}}` isn't a path like machines.web.addresses.front"));
                    }
                    rest = &rest[i + 2 + j + 2..];
                }
            }
        }
    }

    // Groups: names and members.
    for (at, message) in crate::groups::problems(spec) {
        add(&at, message);
    }

    // Declared checks: one probe each, from a machine that can run it.
    for (i, c) in spec.checks.iter().enumerate() {
        match c {
            crate::model::Check::Script(path) if path.trim().is_empty() => add(&format!("checks[{i}]"), "give the script's path in the project".into()),
            crate::model::Check::Script(_) => {}
            crate::model::Check::Declared(d) => validate_check(spec, i, d, &mut add),
        }
    }

    // Targets: requested ones must be possible.
    if let Some(requested) = &spec.targets {
        let possible = derive(spec);
        for (i, t) in requested.iter().enumerate() {
            if !possible.contains(t) {
                let lacking = missing(spec, t.needs());
                add(
                    &format!("targets[{i}]"),
                    format!(
                        "`{}` needs a `{}:` implementation on every machine; missing on: {}",
                        t.id(),
                        t.needs().key(),
                        lacking.join(", ")
                    ),
                );
            }
        }
    }
    if derive(spec).is_empty() && !spec.machines.is_empty() {
        add(
            "machines",
            format!(
                "no target is possible: no shape is implemented by every machine (docker missing on: {}; vm missing on: {})",
                missing(spec, Shape::Docker).join(", "),
                missing(spec, Shape::Vm).join(", ")
            ),
        );
    }
    // On Docker, the networks (moved or not) mustn't overlap either.
    if p.is_empty() {
        let docker = crate::generate::docker_cidrs(spec);
        for (i, (a, ca)) in docker.iter().enumerate() {
            for (b, cb) in docker.iter().skip(i + 1) {
                if ca.overlaps(*cb) {
                    p.push(Problem {
                        at: format!("networks.{b}.docker.cidr"),
                        message: format!("on Docker, overlaps network `{a}`; choose another block"),
                    });
                }
            }
        }
    }
    p
}

fn dependency_cycle(spec: &Spec) -> Option<Vec<String>> {
    fn visit<'a>(spec: &'a Spec, n: &'a str, stack: &mut Vec<&'a str>, done: &mut BTreeSet<&'a str>) -> Option<Vec<String>> {
        if let Some(pos) = stack.iter().position(|s| *s == n) {
            let mut c: Vec<String> = stack[pos..].iter().map(|s| s.to_string()).collect();
            c.push(n.to_string());
            return Some(c);
        }
        if done.contains(n) {
            return None;
        }
        stack.push(n);
        for dep in spec.machines.get(n).map(|m| starts_after(spec, n, m)).unwrap_or_default() {
            if spec.machines.contains_key(dep)
                && let Some(c) = visit(spec, dep, stack, done)
            {
                return Some(c);
            }
        }
        stack.pop();
        done.insert(n);
        None
    }
    let mut done = BTreeSet::new();
    spec.machines.keys().find_map(|n| visit(spec, n, &mut Vec::new(), &mut done))
}

/// The machines a machine starts after: its `depends_on`, then the gateways of its networks.
pub fn starts_after<'a>(spec: &'a Spec, name: &str, m: &'a crate::model::Machine) -> Vec<&'a str> {
    let mut after: Vec<&str> = m.depends_on.iter().map(String::as_str).collect();
    for net in m.networks.keys() {
        if let Some(gw) = spec.networks.get(net).and_then(|n| n.gateway.as_deref())
            && gw != name
            && !after.contains(&gw)
        {
            after.push(gw);
        }
    }
    after
}

/// Checks that every path the spec mentions exists in the project folder.
pub fn validate_files(spec: &Spec, lab_dir: &Path) -> Vec<Problem> {
    let mut p = Vec::new();
    let mut check = |at: String, path: &str| {
        if path.starts_with('/') || path.split('/').any(|s| s == "..") {
            p.push(Problem {
                at,
                message: format!("`{path}` must be a path inside the project folder"),
            });
        } else if !lab_dir.join(path).exists() {
            p.push(Problem {
                at,
                message: format!("`{path}` doesn't exist in the project folder"),
            });
        }
    };
    for (name, m) in &spec.machines {
        if let Some(d) = &m.docker {
            if let Some(b) = &d.build {
                check(format!("machines.{name}.docker.build"), b);
            }
            if let Some(f) = &d.dockerfile {
                check(format!("machines.{name}.docker.dockerfile"), f);
            }
            if let Some(c) = &d.config {
                check(format!("machines.{name}.docker.config"), c);
            }
            if let Some(f) = &d.firmware {
                check(format!("machines.{name}.docker.firmware"), f);
            }
            for (i, s) in d.init.iter().enumerate() {
                check(format!("machines.{name}.docker.init[{i}]"), s);
            }
        }
        if let Some(v) = &m.vm {
            for (i, s) in v.provision.iter().enumerate() {
                check(format!("machines.{name}.vm.provision[{i}]"), s);
            }
        }
    }
    for (i, step) in spec.provision.iter().enumerate() {
        check(format!("provision[{i}].ansible"), &step.ansible);
        for (j, inv) in step.inventory.iter().enumerate() {
            check(format!("provision[{i}].inventory[{j}]"), inv);
        }
        if let Some(r) = &step.requirements {
            check(format!("provision[{i}].requirements"), r);
        }
    }
    for (i, c) in spec.checks.iter().enumerate() {
        if let Some(path) = c.path() {
            let at = match c {
                crate::model::Check::Script(_) => format!("checks[{i}]"),
                crate::model::Check::Declared(_) => format!("checks[{i}].script"),
            };
            check(at, path);
        }
    }
    p
}

/// A declared check: one probe, a machine it can run from, an `expect` its probe understands.
fn validate_check(spec: &Spec, i: usize, d: &crate::model::Declared, add: &mut dyn FnMut(&str, String)) {
    use crate::checks::Url;
    use crate::model::Expect;
    let at = format!("checks[{i}]");
    let kinds = [d.http.is_some(), d.tcp.is_some(), d.exec.is_some(), d.script.is_some()]
        .iter()
        .filter(|k| **k)
        .count();
    if kinds != 1 {
        add(&at, "say what to check with exactly one of `http`, `tcp`, `exec` or `script`".into());
        return;
    }
    if let Some(n) = &d.name
        && n.trim().is_empty()
    {
        add(&format!("{at}.name"), "give the check a name, or leave `name` out".into());
    }
    match &d.from {
        Some(f) => match spec.machines.get(f) {
            None => add(&format!("{at}.from"), format!("no machine named `{f}`")),
            Some(m) if m.docker.as_ref().is_some_and(|d| d.appliance.is_some()) => add(
                &format!("{at}.from"),
                format!("`{f}` is a network appliance: checks run from the machines around it"),
            ),
            Some(m) if !crate::checks::can_run_checks(m) => add(&format!("{at}.from"), format!("`{f}` runs Windows; checks run from Linux machines for now")),
            Some(_) => {}
        },
        None if d.exec.is_some() => add(&at, "`exec` runs inside a machine: say which with `from`".into()),
        None => {}
    }
    if let Some(w) = d.wait
        && w > 3600
    {
        add(&format!("{at}.wait"), "at most 3600 seconds".into());
    }
    let extras = d.method.is_some() || !d.headers.is_empty() || d.body.is_some() || d.contains.is_some();
    if extras && d.http.is_none() {
        add(&at, "`method`, `headers`, `body` and `contains` go with `http`".into());
    }
    if extras && matches!(&d.expect, Some(Expect::Text(t)) if t == "blocked") {
        add(
            &at,
            "a `blocked` check sends nothing to look at: leave out `method`, `headers`, `body` and `contains`".into(),
        );
    }
    if let Some(m) = &d.method
        && (m.is_empty() || !m.chars().all(|c| c.is_ascii_alphabetic()))
    {
        add(&format!("{at}.method"), format!("`{m}` isn't an HTTP method like GET or POST"));
    }
    if let Some(u) = &d.http {
        if Url::parse(u).is_none() {
            add(&format!("{at}.http"), format!("`{u}` isn't a URL like http://web:8080/path"));
        }
        match &d.expect {
            None | Some(Expect::Status(100..=599)) => {}
            Some(Expect::Text(t)) if t == "any" || t == "blocked" => {}
            Some(_) => add(&format!("{at}.expect"), "for `http`: a status code (100-599), `any` or `blocked`".into()),
        }
    }
    if let Some(t) = &d.tcp {
        let ok = t
            .rsplit_once(':')
            .and_then(|(h, p)| p.parse::<u16>().ok().filter(|p| *p > 0).map(|_| h))
            .is_some_and(|h| !h.is_empty() && !h.chars().any(|c| c.is_whitespace() || c == '\'' || c == '"'));
        if !ok {
            add(&format!("{at}.tcp"), format!("`{t}` isn't `host:port`, like cache:6379"));
        }
        match &d.expect {
            None => {}
            Some(Expect::Text(t)) if t == "open" || t == "blocked" => {}
            Some(_) => add(&format!("{at}.expect"), "for `tcp`: `open` or `blocked`".into()),
        }
    }
    if d.exec.is_some() {
        if d.exec.as_deref().is_some_and(|c| c.trim().is_empty()) {
            add(&format!("{at}.exec"), "give a command to run".into());
        }
        if matches!(d.expect, Some(Expect::Status(_))) {
            add(&format!("{at}.expect"), "for `exec`: the text the output must contain".into());
        }
    }
    if d.script.is_some() && d.expect.is_some() {
        add(
            &format!("{at}.expect"),
            "a script says whether it passed by exiting 0; leave `expect` out".into(),
        );
    }
}
