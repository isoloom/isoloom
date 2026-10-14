//! `checks`: a script, or a declared check with one probe (`http`, `tcp`, `exec`, `script`),
//! run from a machine that can run it, with an `expect` its probe understands.

use super::Problems;
use crate::model::{Check, Declared, Expect, Spec};
use crate::probe::Url;

/// Every declared check.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    for (i, c) in spec.checks.iter().enumerate() {
        match c {
            Check::Script(path) if path.trim().is_empty() => p.add(format!("checks[{i}]"), "give the script's path in the project"),
            Check::Script(_) => {}
            Check::Declared(d) => declared(spec, &format!("checks[{i}]"), d, p),
        }
    }
}

/// A declared check: one probe, a machine it can run from, an `expect` its probe understands.
fn declared(spec: &Spec, at: &str, d: &Declared, p: &mut Problems) {
    let kinds = [d.http.is_some(), d.tcp.is_some(), d.exec.is_some(), d.script.is_some()]
        .iter()
        .filter(|k| **k)
        .count();
    if kinds != 1 {
        p.add(at, "say what to check with exactly one of `http`, `tcp`, `exec` or `script`");
        return;
    }
    if let Some(n) = &d.name
        && n.trim().is_empty()
    {
        p.add(format!("{at}.name"), "give the check a name, or leave `name` out");
    }
    from(spec, at, d, p);
    if let Some(w) = d.wait
        && w > 3600
    {
        p.add(format!("{at}.wait"), "at most 3600 seconds");
    }
    extras(at, d, p);
    if let Some(u) = &d.http {
        http(spec, at, u, d, p);
    }
    if let Some(t) = &d.tcp {
        tcp(at, t, d, p);
    }
    if d.exec.is_some() {
        if d.exec.as_deref().is_some_and(|c| c.trim().is_empty()) {
            p.add(format!("{at}.exec"), "give a command to run");
        }
        if matches!(d.expect, Some(Expect::Status(_))) {
            p.add(format!("{at}.expect"), "for `exec`: the text the output must contain");
        }
    }
    if d.script.is_some() && d.expect.is_some() {
        p.add(format!("{at}.expect"), "a script says whether it passed by exiting 0; leave `expect` out");
    }
}

/// `from`: a machine of the spec that can run checks (`exec` needs one).
fn from(spec: &Spec, at: &str, d: &Declared, p: &mut Problems) {
    match &d.from {
        Some(f) => match spec.machines.get(f) {
            None => p.add(format!("{at}.from"), format!("no machine named `{f}`")),
            Some(m) if m.docker.as_ref().is_some_and(|d| d.appliance.is_some()) => p.add(
                format!("{at}.from"),
                format!("`{f}` is a network appliance: checks run from the machines around it"),
            ),
            Some(m) if !crate::probe::can_run_checks(m) => p.add(format!("{at}.from"), format!("`{f}` runs Windows; checks run from Linux machines for now")),
            Some(_) => {}
        },
        None if d.exec.is_some() => p.add(at, "`exec` runs inside a machine: say which with `from`"),
        None => {}
    }
}

/// Whether the check expects to be blocked (no answer at all).
fn blocked(d: &Declared) -> bool {
    matches!(&d.expect, Some(Expect::Text(t)) if t == "blocked")
}

/// The options that go with one probe only: `method`, `headers`, `body`, `follow`, `contains`, `send`.
fn extras(at: &str, d: &Declared, p: &mut Problems) {
    let http_extras = d.method.is_some() || !d.headers.is_empty() || d.body.is_some();
    if http_extras && d.http.is_none() {
        p.add(at, "`method`, `headers` and `body` go with `http`");
    }
    if d.follow && d.http.is_none() {
        p.add(at, "`follow` (redirects) goes with `http`");
    }
    if d.follow && d.http.is_some() && blocked(d) {
        p.add(at, "a `blocked` check gets no answer, so no redirect to follow: leave out `follow`");
    }
    if d.contains.is_some() && d.http.is_none() && d.tcp.is_none() {
        p.add(at, "`contains` goes with `http` or `tcp`");
    }
    if d.send.is_some() && (d.tcp.is_none() || d.contains.is_none()) {
        p.add(at, "`send` goes with `tcp` and `contains` (the text the answer must contain)");
    }
    if (http_extras || d.contains.is_some() || d.send.is_some()) && blocked(d) {
        p.add(
            at,
            "a `blocked` check sends nothing to look at: leave out `method`, `headers`, `body`, `send` and `contains`",
        );
    }
    if d.contains.as_deref().is_some_and(str::is_empty) {
        p.add(format!("{at}.contains"), "give the text to look for, or leave `contains` out");
    }
    if let Some(s) = &d.send
        && s.contains('\n')
    {
        p.add(format!("{at}.send"), "one line: Isoloom adds the newline");
    }
    if let Some(m) = &d.method
        && (m.is_empty() || !m.chars().all(|c| c.is_ascii_alphabetic()))
    {
        p.add(format!("{at}.method"), format!("`{m}` isn't an HTTP method like GET or POST"));
    }
}

/// An `http` probe: a URL (or a cloud output filled in later), and a status, `any` or `blocked`.
fn http(spec: &Spec, at: &str, u: &str, d: &Declared, p: &mut Problems) {
    // A cloud-services output is only known once the services are deployed.
    let output = spec.cloud.is_some() && u.contains("{{ cloud.outputs.");
    if !output && Url::parse(u).is_none() {
        p.add(format!("{at}.http"), format!("`{u}` isn't a URL like http://web:8080/path"));
    }
    match &d.expect {
        None | Some(Expect::Status(100..=599)) => {}
        Some(Expect::Text(t)) if t == "any" || t == "blocked" => {}
        Some(_) => p.add(format!("{at}.expect"), "for `http`: a status code (100-599), `any` or `blocked`"),
    }
}

/// A `tcp` probe: `host:port`, and `open` or `blocked`.
fn tcp(at: &str, t: &str, d: &Declared, p: &mut Problems) {
    let ok = t
        .rsplit_once(':')
        .and_then(|(h, port)| port.parse::<u16>().ok().filter(|port| *port > 0).map(|_| h))
        .is_some_and(|h| !h.is_empty() && !h.chars().any(|c| c.is_whitespace() || c == '\'' || c == '"'));
    if !ok {
        p.add(format!("{at}.tcp"), format!("`{t}` isn't `host:port`, like cache:6379"));
    }
    match &d.expect {
        None => {}
        Some(Expect::Text(e)) if e == "open" || e == "blocked" => {}
        Some(_) => p.add(format!("{at}.expect"), "for `tcp`: `open` or `blocked`"),
    }
}
