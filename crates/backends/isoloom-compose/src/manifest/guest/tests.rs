use isoloom_manifest::{Condition, Delivery, PublishMode, SlotKind, StepSelection};

use super::*;
use crate::render::compose_file;
use crate::testing::environment;

/// A web machine with a flag file, a published port and a set-up script nothing waits for, and a
/// cache.
const FLAGGED: &str = "version: 1
name: shop
inputs: { FLAG: { description: the flag } }
networks: { app: { cidr: 10.60.0.0/24, internet: false } }
machines:
  cache:
    networks: { app: 20 }
    ports: { redis: 6379 }
    app: { image: redis:7-alpine }
  web:
    networks: { app: 10 }
    ports: { web: { port: 80, protocol: http, publish: 8080 } }
    depends_on: [cache]
    files: [{ path: /run/lab/flag, input: FLAG }]
    app: { image: nginx }
    setup: [seed.sh]
checks:
  - { name: page, http: http://web/ }
  - { name: inside, from: cache, exec: redis-cli ping }
";

fn guest_of(yaml: &str) -> GuestCompose {
    let env = environment(yaml);
    let file = compose_file(isoloom_spec::model::Target::Docker, &env).unwrap().file;
    guest(&env, &file).unwrap()
}

#[test]
fn compose_runs_from_the_vms_copy_under_the_files_own_project() {
    let g = guest_of(FLAGGED);
    let start = g.start[0].command.to_string();
    assert!(start.starts_with("cd /opt/isoloom && sudo -E env ISOLOOM_PUBLISH_ADDRESS=0.0.0.0 ISOLOOM_PUBLISH_FIXED=1 docker compose --project-name shop --file .isoloom/docker/compose.yml up --detach --build --pull missing --wait --wait-timeout 600"), "{start}");
    assert_eq!(g.start[0].id.as_str(), "start");
    // Web's set-up job runs attached once the rest is up.
    let jobs: Vec<&str> = g.start.iter().skip(1).map(|c| c.id.as_str()).collect();
    assert_eq!(jobs, ["job-web-init-1"]);
    assert!(g.start[1].command.to_string().ends_with("up --no-deps --exit-code-from web-init-1 web-init-1"));
    let ready: Vec<String> = g
        .ready
        .iter()
        .filter_map(|c| match c {
            Condition::StepOk { step, .. } => Some(step.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(ready, ["start", "job-web-init-1"]);
    assert_eq!(
        g.status.command.to_string(),
        "if command -v docker >/dev/null; then sudo docker compose --project-name shop ps --all --format json; fi"
    );
}

#[test]
fn the_flag_reaches_only_the_start_steps_and_is_forwarded_into_the_vm() {
    let g = guest_of(FLAGGED);
    let flag = g.slots.iter().find(|s| s.name.as_str() == "FLAG").unwrap();
    assert_eq!(flag.kind, SlotKind::Input);
    let Some(Delivery::Env {
        var,
        steps: StepSelection::Only(steps),
    }) = flag.deliver.iter().next()
    else {
        panic!("{:?}", flag.deliver)
    };
    assert_eq!(var.as_str(), "FLAG");
    let steps: Vec<&str> = steps.iter().map(StepId::as_str).collect();
    assert_eq!(steps, ["start", "job-web-init-1"]);
    assert_eq!(g.forwarded(), ["FLAG"]);
}

#[test]
fn every_published_port_is_fixed_and_its_address_left_to_the_vm() {
    let g = guest_of(FLAGGED);
    let web = g.machines.iter().find(|m| m.name.as_str() == "web").unwrap();
    let publish = web.ports[0].publish.as_ref().unwrap();
    assert_eq!((publish.mode, publish.address.as_ref()), (PublishMode::Fixed, None));
    let at = g.machines_at(&Template::text("127.0.0.1"));
    let placed = at.iter().find(|m| m.name.as_str() == "web").unwrap();
    assert_eq!(
        placed.ports[0].publish.as_ref().unwrap().address.as_ref().map(ToString::to_string).as_deref(),
        Some("127.0.0.1")
    );
    // Volumes and networks are named as Compose names them in the VM.
    let cache = g.machines.iter().find(|m| m.name.as_str() == "cache").unwrap();
    assert!(
        cache
            .volumes
            .iter()
            .all(|v| v.native.as_ref().is_some_and(|n| n.to_string().starts_with("shop_")))
    );
    assert_eq!(g.networks[0].native.as_ref().map(ToString::to_string).as_deref(), Some("shop_app"));
}

#[test]
fn the_runners_pass_the_derived_switch_and_pipe_exec_checks_from_the_copy() {
    let g = guest_of(FLAGGED);
    let ids: Vec<&str> = g.runners.iter().map(|r| r.command.id.as_str()).collect();
    assert!(ids.contains(&"check-cache") && ids.contains(&"check-exec-cache"), "{ids:?}");
    let exec = g.runners.iter().find(|r| r.command.id.as_str() == "check-exec-cache").unwrap();
    assert!(
        exec.command
            .command
            .to_string()
            .ends_with("exec -T cache sh -s < .isoloom/docker/checks/exec-cache.sh")
    );
    let compose_run = g.runners.iter().find(|r| r.command.id.as_str() == "check-cache").unwrap();
    assert!(
        compose_run
            .command
            .command
            .to_string()
            .contains("--env ISOLOOM_DERIVED=${params.ISOLOOM_DERIVED} "),
        "{}",
        compose_run.command.command
    );
    assert!(
        compose_run.on_timeout[0]
            .command
            .to_string()
            .contains("com.docker.compose.oneoff=True | xargs -r sudo docker rm -f")
    );
    let derived = &g.params[0];
    let Some(Delivery::Env {
        steps: StepSelection::Only(steps),
        ..
    }) = derived.deliver.iter().next()
    else {
        panic!()
    };
    assert!(steps.iter().all(|s| !s.as_str().starts_with("check-exec-")));
}
