//! The parts of a check's probe a spec is validated against: its URL and host, and which
//! machines can run checks.

use std::fmt;

use crate::images;
use crate::model::Machine;

/// What a probe targets: a machine of the environment on one of its networks (each target
/// writes the address its own way), or a host as written (an address, an outside name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Host {
    /// A machine of the environment, on one of its networks.
    Machine {
        /// The machine's name.
        name: String,
        /// The network whose address the probe uses.
        network: String,
    },
    /// A host as written: an address or an outside name.
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
    /// The scheme is `https` (else `http`).
    pub https: bool,
    /// The host the request goes to.
    pub host: Host,
    /// None: the scheme's default port.
    pub port: Option<u16>,
    /// From the first `/` on (at least `/`).
    pub path: String,
}

impl Url {
    /// Parses `http://host[:port][/path]`; None when it isn't one.
    #[must_use]
    pub fn parse(s: &str) -> Option<Url> {
        let (https, rest) = match s.strip_prefix("https://") {
            Some(r) => (true, r),
            None => (false, s.strip_prefix("http://")?),
        };
        let (hostport, path) = match rest.split_once('/') {
            Some((hostport, path)) => (hostport, format!("/{path}")),
            None => (rest, "/".to_owned()),
        };
        let (host, port) = host_and_port(hostport)?;
        if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c == '\'' || c == '"' || c == '@') {
            return None;
        }
        Some(Url {
            https,
            host: Host::Literal(host.to_owned()),
            port,
            path,
        })
    }

    /// The URL with its host written by `host`, an IPv6 address in brackets
    /// (`http://[fd00:20::10]:80/`).
    #[must_use]
    pub fn render(&self, host: &str) -> String {
        let scheme = if self.https { "https" } else { "http" };
        let bracketed;
        let host = if host.contains(':') {
            bracketed = format!("[{host}]");
            bracketed.as_str()
        } else {
            host
        };
        match self.port {
            Some(p) => format!("{scheme}://{host}:{p}{}", self.path),
            None => format!("{scheme}://{host}{}", self.path),
        }
    }
}

/// `host[:port]`, or `[ipv6]` with an optional `:port` after the bracket.
fn host_and_port(hostport: &str) -> Option<(&str, Option<u16>)> {
    let port = |p: &str| p.parse::<u16>().ok().filter(|p| *p > 0);
    if let Some(rest) = hostport.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        return match after.strip_prefix(':') {
            Some(p) => Some((host, Some(port(p)?))),
            None if after.is_empty() => Some((host, None)),
            None => None,
        };
    }
    match hostport.rsplit_once(':') {
        Some((h, p)) => Some((h, Some(port(p)?))),
        None => Some((hostport, None)),
    }
}

/// Whether a machine can run checks: Linux (or supplied by the runner), not Windows.
#[must_use]
pub fn can_run_checks(m: &Machine) -> bool {
    // An appliance's container isn't where its traffic is (that's the OS inside it).
    !m.vm.as_ref().is_some_and(|v| images::is_windows(&v.os)) && m.docker.as_ref().is_none_or(|d| d.appliance.is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ipv6_literal_goes_in_brackets() {
        let u = Url::parse("http://[fd00:20::10]:8080/x").unwrap();
        assert_eq!((u.port, u.host.to_string()), (Some(8080), "fd00:20::10".to_owned()));
        assert_eq!(u.render("fd00:20::10"), "http://[fd00:20::10]:8080/x");
        assert!(Url::parse("http://[fd00::1]/").is_some_and(|bare| bare.port.is_none()));
        assert!(Url::parse("http://[fd00::1/").is_none());
        assert!(Url::parse("http://[fd00::1]x/").is_none());
    }
}
