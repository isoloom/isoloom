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
    /// Reserved on every target: `.1` (gateway) and the last usable address (router).
    pub fn host(self, last_octet: u8) -> Option<Ipv4Addr> {
        if self.len < 24 || self.len > 29 {
            return None;
        }
        let size = 1u32 << (32 - self.len);
        let offset = u32::from(last_octet) & 0xff;
        let addr = (self.base & !0xff) | offset;
        let first = self.base + 2; // .0 network, .1 gateway
        let last = self.base + size - 3; // the router (last usable) and broadcast excluded
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
    !s.is_empty() && s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') && !s.starts_with(|c: char| c.is_ascii_digit())
}

/// Docker's default address pools (172.17.0.0/12 and 192.168.0.0/16) and the usual home LANs:
/// networks stay inside 10.0.0.0/8 to avoid clashes.
const LAB_BLOCK: Cidr = Cidr { base: 0x0a00_0000, len: 8 };

/// Checks the spec itself (no file system access). See [`validate_files`] for paths.
pub fn validate(spec: &Spec) -> Vec<Problem> {
    let mut p = Vec::new();
    let mut add = |at: &str, message: String| p.push(Problem { at: at.to_string(), message });

    if spec.version != 1 {
        add("version", format!("unsupported version {}; this isoloom reads version 1", spec.version));
    }
    if !kebab(&spec.name) {
        add("name", "use kebab-case: lowercase letters, digits and dashes (e.g. supplier-portal-api)".into());
    }

    // Networks: valid, inside 10.0.0.0/8, sized /24 to /29, not overlapping each other.
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
                if !LAB_BLOCK.contains(c) {
                    add(
                        &format!("{at}.cidr"),
                        "use a block inside 10.0.0.0/8 (Docker's pools and home LANs use 172.16/12 and 192.168/16)".into(),
                    );
                } else if c.len < 24 || c.len > 29 {
                    add(&format!("{at}.cidr"), "use a /24 to /29 block".into());
                }
                for (other, oc) in &cidrs {
                    if c.overlaps(*oc) {
                        add(&format!("{at}.cidr"), format!("overlaps network `{other}`"));
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
            add(&format!("inputs[{i}]"), format!("`{input}`: inputs are UPPER_SNAKE_CASE environment names"));
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
    for (name, m) in &spec.machines {
        let at = format!("machines.{name}");
        if !dns_label(name) {
            add(&at, "machine names are DNS names: kebab-case, starting with a letter".into());
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
                        "{octet} isn't a usable address in {} (reserved: .0, .1 for the gateway, the last address for the router, and the broadcast address)",
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
            }
            if !path.starts_with('/') || path == "/" || path.split('/').any(|s| s == "..") {
                add(
                    &vat,
                    format!("`{path}` isn't an absolute path inside the machine, like /var/lib/postgresql/data"),
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
            if r.cpus == Some(0) || r.memory_mb.is_some_and(|v| v < 256) || r.disk_gb.is_some_and(|v| v < 5) {
                add(&format!("{at}.resources"), "too small: at least 1 cpu, 256 MB and 5 GB".into());
            }
        }
        if m.docker.is_none() && m.vm.is_none() && !m.access {
            add(&at, "give the machine at least one implementation: `docker:` and/or `vm:`".into());
        }
        if let Some(d) = &m.docker {
            match (&d.image, &d.build) {
                (Some(_), Some(_)) => add(&format!("{at}.docker"), "use `image` or `build`, not both".into()),
                (None, None) => add(
                    &format!("{at}.docker"),
                    "set `image` (a published image) or `build` (a folder in the project)".into(),
                ),
                _ => {}
            }
        }
        if let Some(v) = &m.vm {
            if !KNOWN_OS.contains(&v.os.as_str()) {
                add(&format!("{at}.vm.os"), format!("unknown OS `{}`; use one of: {}", v.os, KNOWN_OS.join(", ")));
            }
            if v.provision.is_empty() && !m.access {
                add(&format!("{at}.vm.provision"), "list the steps that install the machine's services".into());
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
    for (i, c) in spec.checks.iter().enumerate() {
        check(format!("checks[{i}]"), c);
    }
    p
}
