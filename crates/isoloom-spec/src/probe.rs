//! The parts of a check's probe a spec is validated against: its URL and host, and which
//! machines can run checks.

use std::fmt;

use crate::images;
use crate::model::Machine;

/// What a probe targets: a machine of the environment on one of its networks (each target
/// writes the address its own way), or a host as written (an address, an outside name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Host {
    Machine { name: String, network: String },
    Literal(String),
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Host::Machine { name, .. } => f.write_str(name),
            Host::Literal(h) => f.write_str(h),
        }
    }
}

/// An HTTP request's parts (the URL as parsed from the spec).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub https: bool,
    pub host: Host,
    /// None: the scheme's default port.
    pub port: Option<u16>,
    /// From the first `/` on (at least `/`).
    pub path: String,
}

impl Url {
    /// Parses `http://host[:port][/path]`; None when it isn't one.
    pub fn parse(s: &str) -> Option<Url> {
        let (https, rest) = match s.strip_prefix("https://") {
            Some(r) => (true, r),
            None => (false, s.strip_prefix("http://")?),
        };
        let (hostport, path) = match rest.find('/') {
            Some(i) => (&rest[..i], rest[i..].to_string()),
            None => (rest, "/".to_string()),
        };
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) => (h, Some(p.parse::<u16>().ok().filter(|p| *p > 0)?)),
            None => (hostport, None),
        };
        if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c == '\'' || c == '"' || c == '@') {
            return None;
        }
        Some(Url {
            https,
            host: Host::Literal(host.to_string()),
            port,
            path,
        })
    }

    /// The URL with its host written by `host`.
    pub fn render(&self, host: &str) -> String {
        let scheme = if self.https { "https" } else { "http" };
        match self.port {
            Some(p) => format!("{scheme}://{host}:{p}{}", self.path),
            None => format!("{scheme}://{host}{}", self.path),
        }
    }
}

/// Whether a machine can run checks: Linux (or supplied by the runner), not Windows.
pub fn can_run_checks(m: &Machine) -> bool {
    // An appliance's container isn't where its traffic is (that's the OS inside it).
    !m.vm.as_ref().is_some_and(|v| images::is_windows(&v.os)) && m.docker.as_ref().is_none_or(|d| d.appliance.is_none())
}
