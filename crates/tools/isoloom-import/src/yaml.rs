//! Writing a draft `isoloom.yml`: the parts every import writes the same way (the header,
//! networks, a machine's networks, ports and resources). Each import writes the rest itself.

use std::fmt::Display;

/// A draft being written, top to bottom.
pub(super) struct DraftYaml {
    out: String,
    machines: usize,
}

/// What the header says: which import drafted it, from what, and what to do next.
pub(super) struct Header<'a> {
    /// The `isoloom import` subcommand, e.g. `compose`.
    pub tool: &'a str,
    /// How the source is named, e.g. `docker-compose.yml`.
    pub source: &'a str,
    /// The sentence after "Review it", up to the end of the comment.
    pub review: &'a str,
    /// What to add next.
    pub next: &'a str,
    /// The environment's name.
    pub name: &'a str,
}

/// One entry of a machine's `ports` map.
pub(super) struct PortEntry {
    pub port: u16,
    pub udp: bool,
    pub publish: Option<u16>,
}

/// The names of well-known ports; any other is `p<port>`.
const KNOWN: &[(u16, &str)] = &[
    (22, "ssh"),
    (25, "smtp"),
    (53, "dns"),
    (69, "tftp"),
    (80, "http"),
    (123, "ntp"),
    (161, "snmp"),
    (389, "ldap"),
    (443, "https"),
    (445, "smb"),
    (514, "syslog"),
    (3306, "mysql"),
    (3389, "rdp"),
    (5432, "postgres"),
    (5672, "amqp"),
    (6379, "redis"),
    (8080, "http-alt"),
    (9200, "elasticsearch"),
    (27017, "mongodb"),
];

impl PortEntry {
    /// The port's name: a well-known one, `-udp` for UDP, never one `taken` already.
    fn name(&self, taken: &[String]) -> String {
        let base = KNOWN
            .iter()
            .find(|(p, _)| *p == self.port)
            .map_or_else(|| format!("p{}", self.port), |(_, n)| (*n).to_owned());
        let base = if self.udp { format!("{base}-udp") } else { base };
        let mut name = base.clone();
        let mut n = 2;
        while taken.contains(&name) {
            name = format!("{base}-{n}");
            n += 1;
        }
        name
    }

    fn flow(&self) -> String {
        let udp = if self.udp { ", protocol: udp" } else { "" };
        match self.publish {
            Some(h) => format!("{{ port: {}{udp}, publish: {h} }}", self.port),
            None if self.udp => format!("{{ port: {}{udp} }}", self.port),
            None => self.port.to_string(),
        }
    }
}

impl DraftYaml {
    /// The header comment, `version`, `name`, and the `networks:` key.
    pub fn new(h: &Header) -> Self {
        let mut d = DraftYaml {
            out: String::new(),
            machines: 0,
        };
        d.line(crate::schema::MODELINE);
        d.line(format!("# Drafted by `isoloom import {}` from {}. {}", h.tool, h.source, h.review));
        d.line(format!("# Next: {}", h.next));
        d.line(format!("version: 1\nname: {}\n\nnetworks:", h.name));
        d
    }

    /// Text as is, then a line break.
    pub fn line(&mut self, text: impl AsRef<str>) {
        self.out.push_str(text.as_ref());
        self.out.push('\n');
    }

    /// One network, its name padded to `width` so the blocks line up.
    pub fn network(&mut self, name: &str, width: usize, cidr: &str, internet: bool) {
        let pad = " ".repeat(width.saturating_sub(name.len()));
        let internet = if internet { "" } else { ", internet: false" };
        self.line(format!("  {name}:{pad} {{ cidr: {cidr}{internet} }}"));
    }

    /// The `machines:` key, after a blank line.
    pub fn machines(&mut self) {
        self.out.push_str("\nmachines:\n");
    }

    /// A machine's key, after a blank line from the machine before.
    pub fn machine(&mut self, name: &str) {
        if self.machines > 0 {
            self.out.push('\n');
        }
        self.machines += 1;
        self.line(format!("  {name}:"));
    }

    /// The machine's `networks`, each with its address's last octet.
    pub fn machine_networks<N: Display>(&mut self, joins: impl IntoIterator<Item = (N, u8)>) {
        let joins = joins.into_iter().map(|(n, o)| format!("{n}: {o}")).collect::<Vec<_>>().join(", ");
        self.line(format!("    networks: {{ {joins} }}"));
    }

    /// The machine's `ports`, each named, when it has any.
    pub fn ports(&mut self, ports: impl IntoIterator<Item = PortEntry>) {
        let mut names: Vec<String> = Vec::new();
        let mut entries: Vec<String> = Vec::new();
        for p in ports {
            let name = p.name(&names);
            entries.push(format!("{name}: {}", p.flow()));
            names.push(name);
        }
        if !entries.is_empty() {
            self.line(format!("    ports: {{ {} }}", entries.join(", ")));
        }
    }

    /// The machine's `resources`, when it sets any (memory: 256 MB at least).
    pub fn resources(&mut self, cpus: Option<u32>, memory_mb: Option<u32>) {
        let mut res = Vec::new();
        if let Some(c) = cpus {
            res.push(format!("cpus: {c}"));
        }
        if let Some(mb) = memory_mb {
            res.push(format!("memory_mb: {}", mb.max(256)));
        }
        if !res.is_empty() {
            self.line(format!("    resources: {{ {} }}", res.join(", ")));
        }
    }

    /// The machine's `os`.
    pub fn os(&mut self, os: &str) {
        self.line(format!("    os: {os}"));
    }

    pub fn finish(self) -> String {
        self.out
    }
}
