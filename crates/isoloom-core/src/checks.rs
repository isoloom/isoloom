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

use std::fmt;

use crate::images;
use crate::model::{Check, Declared, Expect, Machine, Spec};

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
    pub fn id(&self) -> &str {
        match self {
            Position::Machine(m) => m,
            Position::Networks => "networks",
        }
    }

    /// How results name the position.
    pub fn label(&self) -> String {
        match self {
            Position::Machine(m) => format!("from {m}"),
            Position::Networks => "from the environment's networks".to_string(),
        }
    }
}

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

/// What an HTTP probe expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpExpect {
    Status(u16),
    /// An answer of any status.
    Any,
    /// Nothing answers (the connection fails).
    Blocked,
}

/// What a TCP probe expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpExpect {
    Open,
    Blocked,
}

/// What an `http` check sends and looks for beyond a GET and its status (all optional).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HttpRequest {
    pub method: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    /// Text the response body must contain.
    pub contains: Option<String>,
}

impl HttpRequest {
    fn is_plain(&self) -> bool {
        *self == HttpRequest::default()
    }
}

/// One resolved probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    Http {
        url: Url,
        expect: HttpExpect,
        /// A method, headers, a body, text to find: empty for a plain GET.
        request: HttpRequest,
    },
    Tcp {
        host: Host,
        port: u16,
        expect: TcpExpect,
    },
    /// A command inside the position's machine; `expect` is text the output must contain.
    Exec {
        command: String,
        expect: Option<String>,
    },
    /// A script in the project, run with sh from the position.
    Script {
        path: String,
    },
    /// An Ansible playbook in the project, run from the controller.
    Playbook {
        path: String,
    },
}

/// A check, resolved: where it runs, what it probes, how long it may take to pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub name: String,
    pub position: Position,
    pub probe: Probe,
    /// Seconds to keep retrying (0: one attempt).
    pub wait: u32,
    /// Derived from the spec by Isoloom (not written by the author).
    pub derived: bool,
}

/// The position a check without `from` runs from: the access machine, else the networks.
pub fn default_position(spec: &Spec) -> Position {
    match spec.machines.iter().find(|(_, m)| m.access) {
        Some((name, _)) => Position::Machine(name.clone()),
        None => Position::Networks,
    }
}

/// Every check of the spec, resolved: the author's in order, then the derived ones.
/// Validated specs only (a declared check that validation rejects isn't resolved).
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
    let position = d.from.as_ref().map(|f| Position::Machine(f.clone())).unwrap_or_else(|| default.clone());
    let (probe, default_wait, what) = if let Some(u) = &d.http {
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
        };
        let what = match (&expect, &request.method) {
            (HttpExpect::Blocked, _) => format!("{u} blocked"),
            (_, Some(m)) => format!("{} {u}", m.to_uppercase()),
            _ => u.to_string(),
        };
        (Probe::Http { url, expect, request }, wait, what)
    } else if let Some(t) = &d.tcp {
        let (host, port) = t.rsplit_once(':')?;
        let port: u16 = port.parse().ok().filter(|p| *p > 0)?;
        let expect = match d.expect.as_ref() {
            None => TcpExpect::Open,
            Some(Expect::Text(t)) if t == "open" => TcpExpect::Open,
            Some(Expect::Text(t)) if t == "blocked" => TcpExpect::Blocked,
            _ => return None,
        };
        let wait = if expect == TcpExpect::Blocked { 0 } else { 30 };
        let what = match expect {
            TcpExpect::Blocked => format!("{t} blocked"),
            TcpExpect::Open => t.to_string(),
        };
        (
            Probe::Tcp {
                host: Host::Literal(host.to_string()),
                port,
                expect,
            },
            wait,
            what,
        )
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

/// Whether a machine can run checks: Linux (or supplied by the runner), not Windows.
pub fn can_run_checks(m: &Machine) -> bool {
    // An appliance's container isn't where its traffic is (that's the OS inside it).
    !m.vm.as_ref().is_some_and(|v| images::is_windows(&v.os)) && m.docker.as_ref().is_none_or(|d| d.appliance.is_none())
}

/// Whether `from` may open connections to `to`'s address on `network`, port `port`: they share
/// the network, or a `reach` rule from one of `from`'s networks opens it.
pub fn allowed(spec: &Spec, from: &Machine, network: &str, port: u16) -> bool {
    from.networks.contains_key(network)
        || spec
            .reach
            .iter()
            .any(|r| from.networks.contains_key(&r.from) && r.to == network && (r.ports.is_empty() || r.ports.contains(&port)))
}

/// The checks the spec implies, from every machine that can run them:
/// - each service of another machine answers at the addresses `reach` (or a shared network)
///   lets this machine through to, and
/// - is blocked at every address when nothing lets it through at all. When one path is open and
///   another isn't, the closed one isn't asserted: a machine with several interfaces answers for
///   any of its addresses on an interface the traffic may use, so the outcome isn't the spec's
///   to promise;
/// - the internet doesn't answer from a machine whose networks are all offline.
pub fn derived(spec: &Spec) -> Vec<Resolved> {
    let mut out = Vec::new();
    for (a, ma) in &spec.machines {
        if !can_run_checks(ma) {
            continue;
        }
        let position = Position::Machine(a.clone());
        for (b, mb) in &spec.machines {
            if a == b {
                continue;
            }
            let multi = mb.networks.len() > 1;
            for svc in &mb.services {
                let paths: Vec<(&String, bool)> = mb.networks.keys().map(|n| (n, allowed(spec, ma, n, svc.port))).collect();
                let on = |n: &str| if multi { format!(" on {n}") } else { String::new() };
                if paths.iter().all(|(_, ok)| !ok) {
                    for (n, _) in &paths {
                        out.push(Resolved {
                            name: format!("{b}:{}{} blocked from {a}", svc.port, on(n)),
                            position: position.clone(),
                            probe: Probe::Tcp {
                                host: Host::Machine {
                                    name: b.clone(),
                                    network: (*n).clone(),
                                },
                                port: svc.port,
                                expect: TcpExpect::Blocked,
                            },
                            wait: 0,
                            derived: true,
                        });
                    }
                    continue;
                }
                for (n, _) in paths.iter().filter(|(_, ok)| *ok) {
                    let host = Host::Machine {
                        name: b.clone(),
                        network: (*n).clone(),
                    };
                    let probe = if svc.http {
                        Probe::Http {
                            url: Url {
                                https: false,
                                host,
                                port: Some(svc.port),
                                path: "/".into(),
                            },
                            expect: HttpExpect::Any,
                            request: HttpRequest::default(),
                        }
                    } else {
                        Probe::Tcp {
                            host,
                            port: svc.port,
                            expect: TcpExpect::Open,
                        }
                    };
                    out.push(Resolved {
                        name: format!("{b}:{}{} from {a}", svc.port, on(n)),
                        position: position.clone(),
                        probe,
                        wait: 30,
                        derived: true,
                    });
                }
            }
        }
        if !ma.networks.keys().any(|n| spec.networks[n].internet) {
            out.push(Resolved {
                name: format!("no internet from {a}"),
                position,
                probe: Probe::Http {
                    url: Url::parse("http://1.1.1.1/").expect("a URL"),
                    expect: HttpExpect::Blocked,
                    request: HttpRequest::default(),
                },
                wait: 0,
                derived: true,
            });
        }
    }
    out
}

/// The checks grouped by position: the default position first, then machines in spec order.
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

/// How a target writes the runner for one position.
pub struct Render<'a> {
    /// A symbolic host as this target reaches it from the position (an address, or a name).
    pub host: &'a dyn Fn(&Host, &Position) -> String,
    /// The shell command that runs a project script (`cd /opt/isoloom && sh checks/x.sh`).
    pub script: &'a dyn Fn(&str) -> String,
    /// The shell command that runs a playbook, when this target can.
    pub playbook: Option<&'a dyn Fn(&str) -> String>,
}

/// Single-quoted for sh.
pub fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Escaped for the inside of a double-quoted sh string.
fn dq(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('$', "\\$").replace('`', "\\`")
}

/// The sh runner for one position's checks. It needs `sh` and either `curl`, `wget` or `bash`
/// (plus `nc` or `bash` for TCP), prints a PASS or FAIL line per check, and exits 1 when any
/// failed. Derived checks are skipped when `ISOLOOM_DERIVED=0`.
pub fn script(position: &Position, checks: &[&Resolved], r: &Render) -> String {
    let mut s = String::new();
    s.push_str("#!/bin/sh\n");
    s.push_str(&format!(
        "# Generated by isoloom from isoloom.yml. Don't edit: change isoloom.yml and run\n# `isoloom generate`. `isoloom check` fails when this file is out of date.\n# The environment's checks, {}. `isoloom test` runs it and reads the PASS/FAIL lines.\n",
        position.label()
    ));
    s.push_str(RUNTIME);
    for c in checks {
        s.push('\n');
        if c.derived {
            s.push_str("if [ \"${ISOLOOM_DERIVED:-1}\" != 0 ]; then\n");
        }
        let n = sq(&c.name);
        let body = match &c.probe {
            Probe::Http { url, expect, request } if !request.is_plain() && *expect != HttpExpect::Blocked => {
                let text = url.render(&(r.host)(&url.host, position));
                let mut args = String::new();
                if let Some(m) = &request.method {
                    args.push_str(&format!(" -X {}", sq(&m.to_uppercase())));
                }
                for (k, v) in &request.headers {
                    args.push_str(&format!(" -H {}", sq(&format!("{k}: {v}"))));
                }
                if let Some(b) = &request.body {
                    args.push_str(&format!(" --data-binary {}", sq(b)));
                }
                let want = match expect {
                    HttpExpect::Status(code) => code.to_string(),
                    _ => "any".to_string(),
                };
                format!(
                    "if _retry {w} _req {want} {t}{args} {u}; then pass {n}; else fail {n} \"$out\"; fi\n",
                    w = c.wait,
                    t = sq(request.contains.as_deref().unwrap_or("")),
                    u = sq(&text),
                )
            }
            Probe::Http { url, expect, .. } => {
                let text = url.render(&(r.host)(&url.host, position));
                let (u, ut) = (sq(&text), dq(&text));
                match expect {
                    HttpExpect::Status(code) => format!(
                        "if _retry {w} _http_is {u} {code}; then pass {n}; else fail {n} \"expected HTTP {code} from {ut}, got $(_http {u})\"; fi\n",
                        w = c.wait
                    ),
                    HttpExpect::Any => format!(
                        "if _retry {w} _http_any {u}; then pass {n}; else fail {n} \"nothing answers at {ut}\"; fi\n",
                        w = c.wait
                    ),
                    HttpExpect::Blocked => {
                        format!("c=$(_http {u}); if [ \"$c\" = 000 ]; then pass {n}; else fail {n} \"{ut} answered (HTTP $c); it should be blocked\"; fi\n")
                    }
                }
            }
            Probe::Tcp { host, port, expect } => {
                let text = (r.host)(host, position);
                let (h, ht) = (sq(&text), dq(&text));
                match expect {
                    TcpExpect::Open => format!(
                        "if _retry {w} _tcp {h} {port}; then pass {n}; else fail {n} \"nothing listens at {ht}:{port}\"; fi\n",
                        w = c.wait
                    ),
                    TcpExpect::Blocked => {
                        format!("if _tcp {h} {port}; then fail {n} \"{ht}:{port} answered; it should be blocked\"; else pass {n}; fi\n")
                    }
                }
            }
            Probe::Exec { command, expect } => format!(
                "if _retry {w} _exec {cmd} {want}; then pass {n}; else fail {n} \"$(printf '%s' \"$out\" | tail -n 1)\"; fi\n",
                w = c.wait,
                cmd = sq(command),
                want = sq(expect.as_deref().unwrap_or("")),
            ),
            Probe::Script { path } => format!(
                "echo {hdr}\nif _retry {w} sh -c {cmd}; then pass {n}; else fail {n} \"{p} exited non-zero\"; fi\n",
                hdr = sq(&format!("== {path}")),
                w = c.wait,
                cmd = sq(&(r.script)(path)),
                p = dq(path)
            ),
            Probe::Playbook { path } => match r.playbook {
                Some(pb) => format!(
                    "echo {hdr}\nif _retry {w} sh -c {cmd}; then pass {n}; else fail {n} \"{p} failed\"; fi\n",
                    hdr = sq(&format!("== {path}")),
                    w = c.wait,
                    cmd = sq(&pb(path)),
                    p = dq(path)
                ),
                None => format!("fail {n} \"Ansible checks don't run from this position\"\n"),
            },
        };
        s.push_str(&body);
        if c.derived {
            s.push_str("fi\n");
        }
    }
    s.push_str("\necho \"isoloom-check: END $passed passed, $failed failed\"\n[ \"$failed\" -eq 0 ]\n");
    s
}

/// The helpers every runner starts with.
const RUNTIME: &str = r#"passed=0; failed=0
pass() { passed=$((passed+1)); echo "isoloom-check: PASS $1"; }
fail() { failed=$((failed+1)); echo "isoloom-check: FAIL $1: $2"; }
# _http URL -> the status code, 000 when nothing answers (curl, else wget, else bash).
_http() {
  if command -v curl >/dev/null 2>&1; then
    c=$(curl -sk -o /dev/null -m 5 -w '%{http_code}' "$1" 2>/dev/null)
  elif command -v wget >/dev/null 2>&1; then
    c=$(wget -q -S -O /dev/null -T 5 --no-check-certificate "$1" 2>&1 | sed -n 's/^ *HTTP\/[0-9.]* \([0-9][0-9][0-9]\).*/\1/p' | tail -n 1)
  else
    u=${1#*://}; hp=${u%%/*}; p=/${u#*/}; [ "$u" = "$hp" ] && p=/
    h=${hp%%:*}; port=${hp#*:}; [ "$hp" = "$h" ] && port=80
    c=$(H=$h P=$port U=$p timeout 5 bash -c 'exec 3<>/dev/tcp/$H/$P && printf "GET %s HTTP/1.0\r\nHost: %s\r\n\r\n" "$U" "$H" >&3 && read -r l <&3 && echo "$l"' 2>/dev/null | sed -n 's/^HTTP\/[0-9.]* \([0-9][0-9][0-9]\).*/\1/p')
  fi
  echo "${c:-000}"
}
_http_is() { [ "$(_http "$1")" = "$2" ]; }
# _req STATUS|any TEXT CURL-ARGS... -> 0 when the request gets that status (any answer for `any`)
# and its body contains TEXT (if any); $out says why not. Needs curl.
_req() {
  want=$1; text=$2; shift 2
  command -v curl >/dev/null 2>&1 || { out="curl is needed for this check"; return 1; }
  b=$(mktemp); c=$(curl -sk -m 10 -o "$b" -w '%{http_code}' "$@" 2>/dev/null); c=${c:-000}; r=0
  if [ "$want" = any ]; then [ "$c" != 000 ] || r=1; else [ "$c" = "$want" ] || r=1; fi
  if [ $r = 1 ]; then out="expected HTTP $want, got $c"
  elif [ -n "$text" ] && ! grep -qF -- "$text" "$b"; then r=1; out="HTTP $c, but the response doesn't contain the expected text"
  fi
  rm -f "$b"; return $r
}
_http_any() { [ "$(_http "$1")" != 000 ]; }
# _tcp HOST PORT -> 0 when it connects (nc, else bash).
_tcp() {
  if command -v nc >/dev/null 2>&1; then nc -z -w 5 "$1" "$2" >/dev/null 2>&1
  else H=$1 P=$2 timeout 5 bash -c 'exec 3<>/dev/tcp/$H/$P' >/dev/null 2>&1
  fi
}
# _exec COMMAND TEXT -> 0 when the command succeeds and its output contains TEXT (if any).
_exec() { out=$(sh -c "$1" 2>&1) && { [ -z "$2" ] || printf '%s\n' "$out" | grep -qF -- "$2"; }; }
# _retry SECONDS COMMAND... -> keeps trying every 2s until it passes or the time is up.
_retry() { _end=$(( $(date +%s) + $1 )); shift; while ! "$@"; do [ "$(date +%s)" -lt "$_end" ] || return 1; sleep 2; done; }
"#;

/// One line of a runner's output, as `isoloom test` reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Pass(String),
    Fail(String, String),
    /// The runner finished: how many passed and failed.
    End(u32, u32),
}

/// Reads a runner's output line (anywhere in the line: tools prefix their output).
pub fn parse_line(line: &str) -> Option<Line> {
    let (_, rest) = line.split_once("isoloom-check: ")?;
    if let Some(n) = rest.strip_prefix("PASS ") {
        return Some(Line::Pass(n.trim_end().to_string()));
    }
    if let Some(r) = rest.strip_prefix("FAIL ") {
        let (name, why) = r.split_once(": ").unwrap_or((r, ""));
        return Some(Line::Fail(name.to_string(), why.trim_end().to_string()));
    }
    if let Some(r) = rest.strip_prefix("END ") {
        let mut nums = r.split_whitespace().filter_map(|w| w.parse::<u32>().ok());
        return Some(Line::End(nums.next()?, nums.next()?));
    }
    None
}
