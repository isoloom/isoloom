//! A machine's own process: `command`, `working_dir`, `before` and `healthcheck`. What each
//! target needs to run them the same way: a command to run, an absolute directory, scripts the
//! machine's OS runs, and a healthcheck with one test and a pace it can keep.

use super::Problems;
use crate::model::{Healthcheck, Machine, Spec, seconds};

/// Every machine's process fields.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    for (name, m) in &spec.machines {
        let at = format!("machines.{name}");
        command(&at, m, p);
        if let Some(dir) = &m.working_dir
            && !dir.starts_with('/')
        {
            p.add(format!("{at}.working_dir"), format!("`{dir}` must be an absolute path, like /srv/app"));
        }
        before(&at, m, p);
        if let Some(h) = &m.healthcheck {
            healthcheck(&format!("{at}.healthcheck"), h, p);
        }
    }
}

/// Whether the machine's VM runs Windows.
fn windows(m: &Machine) -> bool {
    m.vm.as_ref().is_some_and(|v| crate::images::is_windows(&v.os))
}

/// A command to run, on a machine that runs one of its own (an appliance or an idle machine
/// has Isoloom's).
fn command(at: &str, m: &Machine, p: &mut Problems) {
    if m.command.as_ref().is_some_and(crate::model::CommandLine::is_empty) {
        p.add(format!("{at}.command"), "give the program to run (a text, or a list starting with the program)");
    }
    let set = m.command.is_some() || !m.before.is_empty() || m.working_dir.is_some();
    let Some(d) = m.docker.as_ref().filter(|_| set) else { return };
    if d.appliance.is_some() {
        p.add(at, "a network appliance runs its own OS: no `command`, `working_dir` or `before`");
    } else if d.idle {
        p.add(
            at,
            "an `idle` machine runs nothing of its own: leave `idle` out to give it a `command`, `working_dir` or `before`",
        );
    }
}

/// Scripts the machine's OS runs, and on a container the command they hand over to.
fn before(at: &str, m: &Machine, p: &mut Problems) {
    let windows = windows(m);
    for (i, step) in m.before.iter().enumerate() {
        let is = |ext: &str| std::path::Path::new(step).extension().is_some_and(|e| e.eq_ignore_ascii_case(ext));
        if windows && !is("ps1") {
            p.add(
                format!("{at}.before[{i}]"),
                "a Windows machine runs PowerShell scripts (.ps1) before its command",
            );
        } else if !windows && !is("sh") {
            p.add(format!("{at}.before[{i}]"), "a shell script (.sh), run with the machine's `sh`");
        }
    }
    if !m.before.is_empty() && m.docker.is_some() && m.command.is_none() {
        p.add(
            format!("{at}.before"),
            "on a container the `before` steps replace the image's entrypoint, so they need the process to start after them: restate it in `command` (the image's ENTRYPOINT and CMD, e.g. [docker-entrypoint.sh, postgres])",
        );
    }
}

/// One test, a pace in seconds, and at least one try.
fn healthcheck(at: &str, h: &Healthcheck, p: &mut Problems) {
    match (&h.test, &h.http) {
        (Some(_), Some(_)) => p.add(at, "use `test` or `http`, not both"),
        (None, None) => p.add(at, "say what to check: `test` (a command) or `http` (a page, with its `port`)"),
        (Some(t), None) if t.is_empty() => p.add(format!("{at}.test"), "give the command to run"),
        _ => {}
    }
    if let Some(http) = &h.http {
        if http.port == 0 {
            p.add(format!("{at}.http.port"), "1 to 65535");
        }
        if !http.path.starts_with('/') {
            p.add(format!("{at}.http.path"), format!("`{}` must start with /, like /healthz", http.path));
        }
    }
    let paces = [("interval", &h.interval, 1), ("timeout", &h.timeout, 1), ("start_period", &h.start_period, 0)];
    for (field, value, least) in paces {
        let Some(v) = value else { continue };
        if seconds(v).is_none_or(|s| s < least) {
            p.add(
                format!("{at}.{field}"),
                format!("`{v}` isn't a duration of at least {least}s, like 5s, 1m or 1m30s"),
            );
        }
    }
    if h.retries == Some(0) {
        p.add(format!("{at}.retries"), "at least 1 try");
    }
}

#[cfg(test)]
mod tests {
    use crate::{parse, validate};

    fn problems(machine: &str) -> Vec<String> {
        let yaml = format!("version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n  a: {{ networks: {{ lan: 10 }}, {machine} }}\n");
        let spec = parse(&yaml).expect("the spec parses");
        validate(&spec).into_iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn a_machine_with_its_own_process_is_valid() {
        let ok = problems(
            "docker: { image: postgres }, command: [docker-entrypoint.sh, postgres], working_dir: /srv, before: [flag.sh], healthcheck: { test: pg_isready, interval: 2s, retries: 30, start_period: 1m }",
        );
        assert!(ok.is_empty(), "{ok:?}");
        let vm = problems("vm: { os: debian-12 }, command: 'python3 -m http.server 80', healthcheck: { http: { port: 80 } }");
        assert!(vm.is_empty(), "{vm:?}");
    }

    #[test]
    fn mistakes_say_how_to_fix_them() {
        let p = problems("docker: { image: postgres }, before: [flag.py], working_dir: srv");
        assert!(p.iter().any(|m| m.contains("before[0]: a shell script")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("restate it in `command`")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("working_dir: `srv` must be an absolute path")), "{p:?}");
        let p = problems("docker: { image: nginx }, healthcheck: { interval: '5' }");
        assert!(p.iter().any(|m| m.contains("say what to check")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("interval: `5` isn't a duration")), "{p:?}");
        let p = problems("docker: { image: nginx }, healthcheck: { test: x, http: { port: 80, path: health } }");
        assert!(
            p.iter().any(|m| m.contains("not both")) && p.iter().any(|m| m.contains("must start with /")),
            "{p:?}"
        );
        let p = problems("docker: { image: alpine, idle: true }, command: [sleep, '1']");
        assert!(p.iter().any(|m| m.contains("an `idle` machine")), "{p:?}");
        let p = problems("vm: { os: windows-server-2022 }, command: x, before: [set.sh]");
        assert!(p.iter().any(|m| m.contains("PowerShell scripts (.ps1)")), "{p:?}");
    }
}
