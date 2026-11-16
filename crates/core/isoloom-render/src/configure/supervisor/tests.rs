//! The supervisor: one daemon `exec`'d, several supervised, nothing for a container without services.

use isoloom_plan::{Build, Image};
use isoloom_spec::model::Target;

use super::*;
use crate::testing;

/// The supervisor of a machine `m` (YAML) on Docker, when it has one.
fn supervisor(m: &str) -> Option<String> {
    let env = testing::machine(m, Target::Docker, &[]);
    let c = Configured::of(&env, "m")?;
    let Some(Image::Container {
        build: Some(Build::Generated { process, .. }),
        ..
    }) = &env.machines["m"].image
    else {
        return None;
    };
    script(&c, process)
}

/// What supervisor `r` runs after its helper functions (`isoloom_as`, and the supervising ones when
/// it supervises several).
fn run_part(r: &str) -> &str {
    let after_as = r.split_once(AS).map_or("", |(_, rest)| rest);
    after_as.split_once(SUPERVISE).map_or(after_as, |(_, rest)| rest)
}

#[test]
fn one_daemon_is_execd_with_its_logs_on_the_containers_output() {
    let r = supervisor("os: debian@12, ports: { web: 80 }, app: { steps: [{ packages: [nginx] }, { service: nginx }] }").expect("a supervisor");
    assert!(
        r.starts_with(&format!(
            "#!/bin/sh\n{}# The process of machine `m`'s container: its services, run by Isoloom's supervisor.\nset -eu\n{AS}",
            crate::header("#")
        )),
        "{r}"
    );
    assert_eq!(
        run_part(&r),
        "mkdir -p \"$(dirname '/var/log/nginx/access.log')\" && rm -f '/var/log/nginx/access.log' && ln -s /dev/stdout '/var/log/nginx/access.log'\n\
         mkdir -p \"$(dirname '/var/log/nginx/error.log')\" && rm -f '/var/log/nginx/error.log' && ln -s /dev/stderr '/var/log/nginx/error.log'\n\
         isoloom_as '' '' 'nginx' '-g' 'daemon off;'\n",
        "its logs linked, then the daemon exec'd: nothing supervises it"
    );
}

#[test]
fn several_daemons_are_started_in_order_and_watched() {
    let r = supervisor(
        "os: debian@12, app: { steps: [{ service: { name: api, run: [/srv/api, --port, '8000'], cwd: /srv } }, { service: { name: worker, run: [/srv/worker], user: api } }] }",
    )
    .expect("a supervisor");
    assert!(r.contains(SUPERVISE), "{r}");
    assert_eq!(
        run_part(&r),
        "isoloom_start 'api' '' '/srv' '/srv/api' '--port' '8000'\nisoloom_start 'worker' 'api' '' '/srv/worker'\nisoloom_watch\n"
    );
}

#[test]
fn a_daemon_and_the_machines_command_run_side_by_side() {
    let r = supervisor("os: debian@12, command: [tail, -f, /dev/null], app: { steps: [{ service: { name: api, run: [/srv/api] } }] }").expect("a supervisor");
    assert_eq!(
        run_part(&r),
        "isoloom_start 'api' '' '' '/srv/api'\nisoloom_start command '' '' \"$@\"\nisoloom_watch\n"
    );
}

#[test]
fn a_container_without_services_has_no_supervisor() {
    assert_eq!(supervisor("os: debian@12, app: { steps: [{ packages: [curl] }] }"), None);
    assert_eq!(
        supervisor("os: debian@12, command: [sleep, infinity], app: { steps: [{ packages: [curl] }] }"),
        None
    );
}

/// A machine whose recipe's steps are all `start` (an entry's, over its official image), and
/// the process of an image whose start Isoloom's supervisor runs.
fn started() -> (isoloom_plan::Environment, ContainerProcess) {
    let mut env = testing::machine(
        "os: debian@12, app: { steps: [{ command: [touch, /run/started], creates: /run/started }, { run: prepare.sh }] }",
        Target::Docker,
        &[],
    );
    if let Some(r) = env.machines.get_mut("m").and_then(|m| m.recipe.as_mut()) {
        r.start = std::mem::take(&mut r.install);
    }
    let process = ContainerProcess::Started {
        entrypoint: vec!["/docker-entrypoint.sh".into()],
        command: vec!["nginx".into(), "-g".into(), "daemon off;".into()],
    };
    (env, process)
}

#[test]
fn an_images_start_steps_run_then_its_own_process() {
    let (env, process) = started();
    let c = Configured::of(&env, "m").unwrap();
    let r = script(&c, &process).expect("a supervisor");
    let (_, body) = r.split_once("\nset -eu\n").expect("the supervisor's body");
    assert_eq!(
        body,
        ". /usr/local/lib/isoloom/configure.sh\ncd /opt/isoloom\nisoloom_command 'creates' '1' '/run/started' 'touch' '/run/started'\nsh '/opt/isoloom/prepare.sh'\nexec '/docker-entrypoint.sh' \"$@\"\n"
    );
    let d = super::super::dockerfile::render(&c, "nginx@sha256:0", &process);
    let instructions: Vec<&str> = d.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    assert_eq!(
        instructions,
        [
            "FROM nginx@sha256:0",
            "WORKDIR /opt/isoloom",
            "COPY [\".isoloom/configure/lib/configure.sh\", \"/usr/local/lib/isoloom/configure.sh\"]",
            "COPY [\"prepare.sh\", \"prepare.sh\"]",
            "COPY --chmod=0755 [\".isoloom/configure/m/run\", \"/usr/local/bin/isoloom-run\"]",
            "ENTRYPOINT [\"/usr/local/bin/isoloom-run\"]",
            "CMD [\"nginx\",\"-g\",\"daemon off;\"]"
        ],
        "start runs at each start, not at build: no RUN"
    );
}

/// Every kind of supervisor reads as POSIX sh: one daemon, several, an image's start.
#[cfg(unix)]
#[test]
fn the_supervisor_parses_as_sh() {
    let (env, process) = started();
    let mut scripts: Vec<String> = [
        "os: debian@12, ports: { web: 80 }, app: { steps: [{ packages: [nginx] }, { service: nginx }] }",
        "os: debian@12, app: { steps: [{ service: { name: a, run: [/a] } }, { service: { name: b, run: [/b, \"it's\"] } }] }",
    ]
    .into_iter()
    .map(|m| supervisor(m).expect("a supervisor"))
    .collect();
    scripts.push(script(&Configured::of(&env, "m").expect("a recipe"), &process).expect("a supervisor"));
    for r in scripts {
        let status = std::process::Command::new("sh").args(["-n", "-c", &r]).status().expect("sh runs");
        assert!(status.success(), "{r}");
    }
}
