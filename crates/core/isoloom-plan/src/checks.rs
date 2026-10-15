//! Checks: what proves an environment behaves as its spec says, on every target.
//!
//! A spec's `checks:` hold scripts and declared probes (`http`, `tcp`, `exec`, `script`), each
//! run from a position: a machine of the environment (its networks, its routes, its view), or
//! the environment's networks at once when the spec has no access machine. Isoloom adds
//! **derived checks** from the spec itself: every service must answer from every machine that
//! `reach` lets through, and must not answer from the machines it doesn't; a machine whose
//! networks are offline must not reach the internet.
//!
//! The planner here is target-independent: [`plan`] resolves every check to a position and a
//! probe with symbolic hosts, and [`script`] turns one position's checks into a POSIX sh
//! runner. Each generator decides how a position is realized (a container in a machine's
//! network namespace, a provisioner on a VM, a Job with a machine's labels) and how a symbolic
//! host is written (an address, or a name on Kubernetes). The runners print one line per
//! check, `isoloom-check: PASS <name>` or `isoloom-check: FAIL <name>: <why>`, which
//! `isoloom test` reads back.

pub use isoloom_spec::probe::{Host, Url, can_run_checks};

mod derived;
mod runner;

pub use derived::{allowed, derived, from_networks};
pub use runner::{Line, Render, parse_line, script};

use isoloom_spec::model::{Check, Declared, Expect, Spec};

/// Where a check runs from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Position {
    /// A machine of the environment (by name).
    Machine(String),
    /// The environment's networks at once: a spec without an access machine.
    Networks,
}

impl Position {
    /// A file-safe id: the machine's name, or `networks`.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Position::Machine(m) => m,
            Position::Networks => "networks",
        }
    }

    /// How results name the position.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Position::Machine(m) => format!("from {m}"),
            Position::Networks => "from the environment's networks".to_owned(),
        }
    }
}

/// What an HTTP probe expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpExpect {
    /// This exact status.
    Status(u16),
    /// An answer of any status.
    Any,
    /// Nothing answers (the connection fails).
    Blocked,
}

/// What a TCP probe expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpExpect {
    /// The connection is accepted.
    Open,
    /// Nothing accepts the connection.
    Blocked,
}

/// What an `http` check sends and looks for beyond a GET and its status (all optional).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HttpRequest {
    /// The method, when not a GET.
    pub method: Option<String>,
    /// Headers sent as written, in order.
    pub headers: Vec<(String, String)>,
    /// The request body, sent as it is.
    pub body: Option<String>,
    /// Text the response body must contain.
    pub contains: Option<String>,
    /// Follow redirects (up to [`HTTP_MAX_REDIRECTS`]) and compare the final status.
    pub follow: bool,
}

impl HttpRequest {
    /// Nothing beyond a GET and its status (following redirects or not).
    fn is_plain(&self) -> bool {
        self.method.is_none() && self.headers.is_empty() && self.body.is_none() && self.contains.is_none()
    }
}

/// How many redirects an `http` check with `follow` goes through before giving up (the status
/// compared is then that of the last answer, a redirect).
pub const HTTP_MAX_REDIRECTS: u32 = 5;

/// What a `tcp` check writes and reads beyond connecting (both optional; `send` needs `contains`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TcpExchange {
    /// A line written once connected (a newline is added).
    pub send: Option<String>,
    /// Text the bytes read in the first 5 seconds of the connection must contain.
    pub contains: Option<String>,
}

/// One resolved probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// An HTTP request to a URL.
    Http {
        /// Where the request goes; its host is resolved per position.
        url: Url,
        /// The answer that passes.
        expect: HttpExpect,
        /// A method, headers, a body, text to find: empty for a plain GET.
        request: HttpRequest,
    },
    /// A TCP connection to a host's port.
    Tcp {
        /// The host connected to, resolved per position.
        host: Host,
        /// The port connected to.
        port: u16,
        /// Whether the connection should open.
        expect: TcpExpect,
        /// A line to write and text to read back: empty for a bare connection.
        exchange: TcpExchange,
    },
    /// A command inside the position's machine.
    Exec {
        /// The command, run with sh.
        command: String,
        /// Text the output must contain (`None`: exiting 0 passes).
        expect: Option<String>,
    },
    /// A script in the project, run with sh from the position.
    Script {
        /// The script's path in the project.
        path: String,
    },
    /// An Ansible playbook in the project, run from the controller.
    Playbook {
        /// The playbook's path in the project.
        path: String,
    },
}

/// A check, resolved: where it runs, what it probes, how long it may take to pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The name results report it by.
    pub name: String,
    /// Where it runs from.
    pub position: Position,
    /// What it probes and expects.
    pub probe: Probe,
    /// Seconds to keep retrying (0: one attempt).
    pub wait: u32,
    /// Derived from the spec by Isoloom (not written by the author).
    pub derived: bool,
}

/// The position a check without `from` runs from: the access machine, else the networks.
#[must_use]
pub fn default_position(spec: &Spec) -> Position {
    match spec.machines.iter().find(|(_, m)| m.access) {
        Some((name, _)) => Position::Machine(name.clone()),
        None => Position::Networks,
    }
}

/// Every check of the spec, resolved: the author's in order, then the derived ones.
/// Validated specs only (a declared check that validation rejects isn't resolved).
#[must_use]
pub fn plan(spec: &Spec) -> Vec<Resolved> {
    let default = default_position(spec);
    let mut out: Vec<Resolved> = spec
        .checks
        .iter()
        .filter_map(|c| match c {
            Check::Script(p) => Some(Resolved {
                name: p.clone(),
                position: default.clone(),
                probe: if c.is_playbook() {
                    Probe::Playbook { path: p.clone() }
                } else {
                    Probe::Script { path: p.clone() }
                },
                wait: 0,
                derived: false,
            }),
            Check::Declared(d) => declared(d, &default),
        })
        .collect();
    out.extend(derived(spec));
    out
}

fn declared(d: &Declared, default: &Position) -> Option<Resolved> {
    let position = d.from.as_ref().map_or_else(|| default.clone(), |f| Position::Machine(f.clone()));
    let (probe, default_wait, what) = if let Some(u) = &d.http {
        declared_http(d, u)?
    } else if let Some(t) = &d.tcp {
        declared_tcp(d, t)?
    } else if let Some(c) = &d.exec {
        let expect = match &d.expect {
            None => None,
            Some(Expect::Text(t)) => Some(t.clone()),
            Some(Expect::Status(_)) => return None,
        };
        (Probe::Exec { command: c.clone(), expect }, 0, c.clone())
    } else {
        let p = d.script.as_ref()?;
        (Probe::Script { path: p.clone() }, 0, p.clone())
    };
    Some(Resolved {
        name: d.name.clone().unwrap_or_else(|| match &position {
            Position::Machine(m) => format!("{what} from {m}"),
            Position::Networks => what,
        }),
        position,
        probe,
        wait: d.wait.unwrap_or(default_wait),
        derived: false,
    })
}

/// A declared probe, the seconds it waits by default, and what its name says it checks.
type DeclaredProbe = (Probe, u32, String);

/// A declared `http` check (to URL `u`).
fn declared_http(d: &Declared, u: &str) -> Option<DeclaredProbe> {
    let url = Url::parse(u)?;
    let expect = match &d.expect {
        None => HttpExpect::Status(200),
        Some(Expect::Status(s)) => HttpExpect::Status(*s),
        Some(Expect::Text(t)) if t == "any" => HttpExpect::Any,
        Some(Expect::Text(t)) if t == "blocked" => HttpExpect::Blocked,
        Some(Expect::Text(_)) => return None,
    };
    let wait = if expect == HttpExpect::Blocked { 0 } else { 30 };
    let request = HttpRequest {
        method: d.method.clone(),
        headers: d.headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        body: d.body.clone(),
        contains: d.contains.clone(),
        follow: d.follow,
    };
    let what = match (&expect, &request.method) {
        (HttpExpect::Blocked, _) => format!("{u} blocked"),
        (_, Some(m)) => format!("{} {u}", m.to_uppercase()),
        _ => u.to_owned(),
    };
    Some((Probe::Http { url, expect, request }, wait, what))
}

/// A declared `tcp` check (to `host:port` `target`).
fn declared_tcp(d: &Declared, target: &str) -> Option<DeclaredProbe> {
    let (host, port) = target.rsplit_once(':')?;
    let port: u16 = port.parse().ok().filter(|p| *p > 0)?;
    let expect = match d.expect.as_ref() {
        None => TcpExpect::Open,
        Some(Expect::Text(t)) if t == "open" => TcpExpect::Open,
        Some(Expect::Text(t)) if t == "blocked" => TcpExpect::Blocked,
        _ => return None,
    };
    let wait = if expect == TcpExpect::Blocked { 0 } else { 30 };
    let what = match expect {
        TcpExpect::Blocked => format!("{target} blocked"),
        TcpExpect::Open => target.to_owned(),
    };
    let exchange = TcpExchange {
        send: d.send.clone(),
        contains: d.contains.clone(),
    };
    let probe = Probe::Tcp {
        host: Host::Literal(host.to_owned()),
        port,
        expect,
        exchange,
    };
    Some((probe, wait, what))
}

/// The checks grouped by position: the default position first, then machines in spec order.
#[must_use]
pub fn by_position<'a>(spec: &Spec, checks: &'a [Resolved]) -> Vec<(Position, Vec<&'a Resolved>)> {
    let mut order: Vec<Position> = vec![default_position(spec)];
    for m in spec.machines.keys() {
        let p = Position::Machine(m.clone());
        if !order.contains(&p) {
            order.push(p);
        }
    }
    if !order.contains(&Position::Networks) {
        order.push(Position::Networks);
    }
    order
        .into_iter()
        .filter_map(|p| {
            let group: Vec<&Resolved> = checks.iter().filter(|c| c.position == p).collect();
            (!group.is_empty()).then_some((p, group))
        })
        .collect()
}

#[cfg(test)]
mod tests;
