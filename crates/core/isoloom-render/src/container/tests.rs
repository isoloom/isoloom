use isoloom_plan::{Health, HealthTest};
use isoloom_spec::model::{CommandLine, Target};

use super::{
    copied_from_project, docker_net, exec_runner, host_environment, init_names, leaf_jobs, probe, project_variables, ready_test, start_commands, variable,
};
use crate::testing;

/// `web` serves two ports (one dormant) and has two setup jobs; `api` waits for `db`, whose job
/// a running service therefore waits for; `web`'s own project file has a mode.
const SPEC: &str = r"
version: 1
name: c
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  web:
    networks: { lan: 10 }
    ports: { site: 80, admin: { port: 8080, dormant: true } }
    setup: [a.sh, b.sh]
    files:
      - { path: /etc/web.conf, source: conf/web.conf, mode: '0600' }
      - { path: /etc/plain.conf, source: conf/plain.conf }
      - { path: /etc/inline.conf, content: x, mode: '0600' }
    app: { image: nginx }
  db: { networks: { lan: 20 }, ports: { pg: 5432 }, setup: [seed.sh], app: { image: postgres } }
  api: { networks: { lan: 30 }, depends_on: [db], app: { image: api } }
";

#[test]
fn the_probe_tries_each_port_but_the_dormant_ones() {
    let spec = testing::spec(SPEC);
    assert_eq!(probe(&spec.machines["web"]), "/.isoloom-probe/busybox nc -z -w 2 127.0.0.1 80");
    assert_eq!(probe(&spec.machines["api"]), "");
}

#[test]
fn readiness_is_the_ports_then_the_machine_s_own_healthcheck() {
    let spec = testing::spec(SPEC);
    let health = Health {
        test: HealthTest::Command(CommandLine::Shell("pg_isready || exit 1".into())),
        interval: 5,
        timeout: 3,
        retries: 3,
        start_period: 0,
    };
    assert_eq!(
        ready_test(&spec.machines["db"], Some(&health)),
        "/.isoloom-probe/busybox nc -z -w 2 127.0.0.1 5432 && ( pg_isready || exit 1 )"
    );
    assert_eq!(ready_test(&spec.machines["api"], Some(&health)), "( pg_isready || exit 1 )");
    assert_eq!(ready_test(&spec.machines["api"], None), "");
    assert_eq!(exec_runner("web"), "exec-web.sh");
}

#[test]
fn the_jobs_nothing_waits_for_run_attached_after_the_rest() {
    let spec = testing::spec(SPEC);
    assert_eq!(init_names("web", &spec.machines["web"]), ["web-init-1", "web-init-2"]);
    assert_eq!(leaf_jobs(&spec), ["web-init-1", "web-init-2"], "api waits for db: its job is not a leaf");
    assert_eq!(start_commands("dc", &[], Some(60)), "dc up -d --build --wait --wait-timeout 60");
    assert_eq!(
        start_commands("dc", &leaf_jobs(&spec), None),
        "dc up -d --build --wait $(dc config --services | grep -vx -e web-init-1 -e web-init-2) && dc up --no-deps --exit-code-from web-init-1 web-init-1 && dc up --no-deps --exit-code-from web-init-2 web-init-2"
    );
}

#[test]
fn only_a_project_file_with_an_owner_or_a_mode_goes_through_compose_s_environment() {
    let env = testing::env(SPEC, Target::Docker);
    let files = &env.machines["web"].files;
    assert_eq!(files.iter().map(copied_from_project).collect::<Vec<_>>(), [true, false, false]);
    assert_eq!(project_variables(&env), [(variable("web", 0), "conf/web.conf".to_owned())]);
    assert_eq!(variable("my-web", 2), "ISOLOOM_FILE_MY_WEB_2");
    assert_eq!(
        host_environment(&env),
        "ISOLOOM_FILE_WEB_0=\"$(cat 'conf/web.conf'; printf x)\" && ISOLOOM_FILE_WEB_0=\"${ISOLOOM_FILE_WEB_0%x}\" && export ISOLOOM_FILE_WEB_0 && "
    );
    // A VM places its files itself.
    assert_eq!(
        project_variables(&testing::env(&SPEC.replace("app: { image: nginx }", "os: debian@12"), Target::Vagrant)),
        []
    );
    assert_eq!(docker_net(&env, "lan"), "isoloom-c-lan");
}
