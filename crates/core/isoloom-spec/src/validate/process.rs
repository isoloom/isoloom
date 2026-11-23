//! A machine's own process: `command`, `working_dir` and `healthcheck`. What each target needs
//! to run them the same way: a command to run, an absolute directory, and a healthcheck with one
//! test and a pace it can keep.

use super::Problems;
use crate::model::{Healthcheck, Machine, Spec, seconds};

/// Every machine's process fields.
pub(super) fn check(spec: &Spec, problems: &mut Problems) {
    for (name, m) in &spec.machines {
        let at = format!("machines.{name}");
        command(&at, m, problems);
        if let Some(dir) = &m.working_dir
            && !dir.starts_with('/')
        {
            problems.add(format!("{at}.working_dir"), format!("`{dir}` must be an absolute path, like /srv/app"));
        }
        if let Some(h) = &m.healthcheck {
            healthcheck(&format!("{at}.healthcheck"), h, problems);
        }
    }
}

/// A command to run, on a machine that runs one of its own (an appliance or an idle machine
/// has Isoloom's).
fn command(at: &str, m: &Machine, problems: &mut Problems) {
    if m.command.as_ref().is_some_and(crate::model::CommandLine::is_empty) {
        problems.add(format!("{at}.command"), "give the program to run (a text, or a list starting with the program)");
    }
    let set = m.command.is_some() || m.working_dir.is_some();
    let Some(d) = m.docker.as_ref().filter(|_| set) else { return };
    if d.appliance.is_some() {
        problems.add(at, "a network appliance runs its own OS: no `command` or `working_dir`");
    } else if d.idle {
        problems.add(
            at,
            "an `idle` machine runs nothing of its own: leave `idle` out to give it a `command` or `working_dir`",
        );
    }
}

/// One test, a pace in seconds, and at least one try.
fn healthcheck(at: &str, h: &Healthcheck, problems: &mut Problems) {
    match (&h.test, &h.http) {
        (Some(_), Some(_)) => problems.add(at, "use `test` or `http`, not both"),
        (None, None) => problems.add(at, "say what to check: `test` (a command) or `http` (a page, with its `port`)"),
        (Some(t), None) if t.is_empty() => problems.add(format!("{at}.test"), "give the command to run"),
        _ => {}
    }
    if let Some(http) = &h.http {
        if http.port == 0 {
            problems.add(format!("{at}.http.port"), "1 to 65535");
        }
        if !http.path.starts_with('/') {
            problems.add(format!("{at}.http.path"), format!("`{}` must start with /, like /healthz", http.path));
        }
    }
    let paces = [("interval", &h.interval, 1), ("timeout", &h.timeout, 1), ("start_period", &h.start_period, 0)];
    for (field, value, least) in paces {
        let Some(v) = value else { continue };
        if seconds(v).is_none_or(|s| s < least) {
            problems.add(
                format!("{at}.{field}"),
                format!("`{v}` isn't a duration of at least {least}s, like 5s, 1m or 1m30s"),
            );
        }
    }
    if h.retries == Some(0) {
        problems.add(format!("{at}.retries"), "at least 1 try");
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::model as parse;
    use crate::validate;

    fn problems(machine: &str) -> Vec<String> {
        let yaml = format!("version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n  a: {{ networks: {{ lan: 10 }}, {machine} }}\n");
        let spec = parse(&yaml).expect("the spec parses");
        validate(&spec).into_iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn a_machine_with_its_own_process_is_valid() {
        let ok = problems(
            "docker: { image: postgres }, command: [docker-entrypoint.sh, postgres], working_dir: /srv, healthcheck: { test: pg_isready, interval: 2s, retries: 30, start_period: 1m }",
        );
        assert_eq!(ok, Vec::<String>::new());
        let vm = problems("vm: { os: debian@12 }, command: 'python3 -m http.server 80', healthcheck: { http: { port: 80 } }");
        assert_eq!(vm, Vec::<String>::new());
    }

    #[test]
    fn the_working_dir_is_absolute() {
        assert_eq!(
            problems("docker: { image: postgres }, working_dir: srv"),
            ["machines.a.working_dir: `srv` must be an absolute path, like /srv/app"]
        );
    }

    #[test]
    fn a_healthcheck_says_what_to_check_and_how_often() {
        assert_eq!(
            problems("docker: { image: nginx }, healthcheck: { interval: '5' }"),
            [
                "machines.a.healthcheck: say what to check: `test` (a command) or `http` (a page, with its `port`)",
                "machines.a.healthcheck.interval: `5` isn't a duration of at least 1s, like 5s, 1m or 1m30s",
            ]
        );
    }

    #[test]
    fn a_healthcheck_is_a_test_or_a_page_from_its_root() {
        assert_eq!(
            problems("docker: { image: nginx }, healthcheck: { test: x, http: { port: 80, path: health } }"),
            [
                "machines.a.healthcheck: use `test` or `http`, not both",
                "machines.a.healthcheck.http.path: `health` must start with /, like /healthz",
            ]
        );
    }

    #[test]
    fn an_idle_machine_runs_no_command() {
        assert_eq!(
            problems("docker: { image: alpine, idle: true }, command: [sleep, '1']"),
            ["machines.a: an `idle` machine runs nothing of its own: leave `idle` out to give it a `command` or `working_dir`"]
        );
    }

    #[test]
    fn an_appliance_runs_no_process_of_its_own() {
        assert_eq!(
            problems("docker: { image: x, appliance: cisco-iol }, working_dir: /srv"),
            ["machines.a: a network appliance runs its own OS: no `command` or `working_dir`"]
        );
    }

    #[test]
    fn a_healthcheck_test_is_a_command() {
        assert_eq!(
            problems("docker: { image: nginx }, healthcheck: { test: '' }"),
            ["machines.a.healthcheck.test: give the command to run"]
        );
    }

    #[test]
    fn a_healthchecks_pace_has_a_floor() {
        let floor = "docker: { image: nginx }, healthcheck: { test: x, interval: 1s, timeout: 1s, start_period: 0s }";
        assert_eq!(problems(floor), Vec::<String>::new());
        assert_eq!(
            problems("docker: { image: nginx }, healthcheck: { test: x, interval: 0s }"),
            ["machines.a.healthcheck.interval: `0s` isn't a duration of at least 1s, like 5s, 1m or 1m30s"]
        );
    }

    #[test]
    fn a_command_a_healthchecks_port_and_its_retries_are_given() {
        assert_eq!(
            problems("docker: { image: x }, command: '', healthcheck: { http: { port: 0 }, retries: 0 }"),
            [
                "machines.a.command: give the program to run (a text, or a list starting with the program)",
                "machines.a.healthcheck.http.port: 1 to 65535",
                "machines.a.healthcheck.retries: at least 1 try",
            ]
        );
    }
}
