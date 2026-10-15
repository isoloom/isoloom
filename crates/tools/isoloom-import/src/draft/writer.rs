//! Writing a draft `isoloom.yml`, top to bottom: the parts every import writes the same way (the
//! header, networks, a machine's networks, ports and resources). Each import writes the rest
//! itself, through [`Writer::line`] and the quoting of [`super::quoting`].

use std::fmt::Display;

use isoloom_spec::validate::Cidr;

/// A draft being written, top to bottom.
#[derive(Debug)]
pub(crate) struct Writer {
    out: String,
    machines: usize,
}

/// What the header says: which import drafted it, from what, and what to do next.
#[derive(Debug)]
pub(crate) struct Header<'a> {
    /// The `isoloom import` subcommand, e.g. `compose`.
    pub(crate) tool: &'a str,
    /// How the source is named, e.g. `docker-compose.yml`.
    pub(crate) source: &'a str,
    /// The sentence after "Review it", up to the end of the comment.
    pub(crate) review: &'a str,
    /// What to add next.
    pub(crate) next: &'a str,
    /// The environment's name.
    pub(crate) name: &'a str,
}

/// One entry of a machine's `ports` map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Port {
    /// The port's number, the one the machine listens on.
    pub(crate) number: u16,
    /// UDP rather than TCP.
    pub(crate) udp: bool,
    /// The host port it is published on, when it is.
    pub(crate) publish: Option<u16>,
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

impl Port {
    /// The port's name: a well-known one, `-udp` for UDP, never one `taken` already.
    fn name(self, taken: &[String]) -> String {
        let known = KNOWN
            .iter()
            .find(|(p, _)| *p == self.number)
            .map_or_else(|| format!("p{}", self.number), |(_, n)| (*n).to_owned());
        let base = if self.udp { format!("{known}-udp") } else { known };
        let mut name = base.clone();
        let mut n = 2;
        while taken.contains(&name) {
            name = format!("{base}-{n}");
            n += 1;
        }
        name
    }

    /// The port as the value of its name in the `ports` map.
    fn flow(self) -> String {
        let udp = if self.udp { ", protocol: udp" } else { "" };
        match self.publish {
            Some(h) => format!("{{ port: {}{udp}, publish: {h} }}", self.number),
            None if self.udp => format!("{{ port: {}{udp} }}", self.number),
            None => self.number.to_string(),
        }
    }
}

impl Writer {
    /// The header comment, `version`, `name`, and the `networks:` key.
    pub(crate) fn new(h: &Header) -> Self {
        let mut w = Writer {
            out: String::new(),
            machines: 0,
        };
        w.line(isoloom_spec::schema::MODELINE);
        w.line(super::quoting::comment(&format!(
            "Drafted by `isoloom import {}` from {}. {}",
            h.tool, h.source, h.review
        )));
        w.line(super::quoting::comment(&format!("Next: {}", h.next)));
        w.line(format!("version: 1\nname: {}\n\nnetworks:", h.name));
        w
    }

    /// Text as is, then a line break.
    pub(crate) fn line(&mut self, text: impl AsRef<str>) {
        self.out.push_str(text.as_ref());
        self.out.push('\n');
    }

    /// One network, its name padded to `width` so the blocks line up.
    pub(crate) fn network(&mut self, name: &str, width: usize, block: Cidr, internet: bool) {
        let pad = " ".repeat(width.saturating_sub(name.len()));
        let internet = if internet { "" } else { ", internet: false" };
        self.line(format!("  {name}:{pad} {{ cidr: {block}{internet} }}"));
    }

    /// The `machines:` key, after a blank line.
    pub(crate) fn machines(&mut self) {
        self.out.push_str("\nmachines:\n");
    }

    /// A machine's key, after a blank line from the machine before.
    pub(crate) fn machine(&mut self, name: &str) {
        if self.machines > 0 {
            self.out.push('\n');
        }
        self.machines += 1;
        self.line(format!("  {name}:"));
    }

    /// The machine's `networks`, each with its address's last octet.
    pub(crate) fn machine_networks<N: Display>(&mut self, joins: impl IntoIterator<Item = (N, u8)>) {
        let joins = joins.into_iter().map(|(n, o)| format!("{n}: {o}")).collect::<Vec<_>>().join(", ");
        self.line(format!("    networks: {{ {joins} }}"));
    }

    /// The machine's `ports`, each named, when it has any.
    pub(crate) fn ports(&mut self, ports: impl IntoIterator<Item = Port>) {
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
    pub(crate) fn resources(&mut self, cpus: Option<u32>, memory_mb: Option<u32>) {
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
    pub(crate) fn os(&mut self, os: &str) {
        self.line(format!("    os: {os}"));
    }

    /// The draft as written.
    pub(crate) fn finish(self) -> String {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_are_named_by_their_well_known_name_and_numbered_when_taken() {
        let mut w = Writer {
            out: String::new(),
            machines: 0,
        };
        let tcp = |number| Port {
            number,
            udp: false,
            publish: None,
        };
        w.ports([
            tcp(80),
            Port {
                number: 53,
                udp: true,
                publish: Some(5353),
            },
            tcp(80),
            tcp(9999),
        ]);
        assert_eq!(
            w.finish(),
            "    ports: { http: 80, dns-udp: { port: 53, protocol: udp, publish: 5353 }, http-2: 80, p9999: 9999 }\n"
        );
    }

    #[test]
    fn a_header_from_a_file_name_with_a_line_break_stays_a_comment() {
        let w = Writer::new(&Header {
            tool: "compose",
            source: "a\nname: injected",
            review: "Review it.",
            next: "checks.",
            name: "lab",
        });
        let text = w.finish();
        assert!(!text.lines().any(|l| l.starts_with("name: injected")), "{text}");
    }
}
