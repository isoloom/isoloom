//! Writing a draft `isoloom.yml`: the parts every import writes the same way (the header,
//! networks, a machine's networks, services and resources). Each import writes the rest itself.

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

/// One entry of a machine's `services` list.
pub(super) struct ServiceEntry {
    pub port: u16,
    pub udp: bool,
    pub publish: Option<u16>,
}

impl ServiceEntry {
    fn flow(&self) -> String {
        let udp = if self.udp { ", protocol: udp" } else { "" };
        match self.publish {
            Some(h) => format!("{{ port: {}{udp}, publish: {h} }}", self.port),
            None => format!("{{ port: {}{udp} }}", self.port),
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

    /// The machine's `services`, when it has any.
    pub fn services(&mut self, services: impl IntoIterator<Item = ServiceEntry>) {
        let services: Vec<String> = services.into_iter().map(|s| s.flow()).collect();
        if !services.is_empty() {
            self.line(format!("    services: [{}]", services.join(", ")));
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

    /// The machine's `vm:` key and its `os`.
    pub fn vm_os(&mut self, os: &str) {
        self.line(format!("    vm:\n      os: {os}"));
    }

    pub fn finish(self) -> String {
        self.out
    }
}
