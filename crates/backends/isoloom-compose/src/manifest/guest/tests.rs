use isoloom_manifest::{Condition, Delivery, PublishMode, SlotKind, StepSelection};

use super::*;
use crate::render::compose_file;
use crate::testing::environment;

/// A web machine with a secret file, a published port and a set-up script nothing waits for, and a
/// cache.
const WITH_SECRET: &str = "version: 1
name: shop
inputs: { DEMO_SECRET: { description: the secret } }
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
    files: [{ path: /run/lab/secret, input: DEMO_SECRET }]
    app: { image: nginx }
    setup: [seed.sh]
checks:
  - { name: page, http: http://web/ }
  - { name: inside, from: cache, exec: redis-cli ping }
";

fn guest_of(yaml: &str) -> GuestCompose {
    guest_as(yaml, AsRoot::Sudo)
}

fn guest_as(yaml: &str, as_root: AsRoot) -> GuestCompose {
    let env = environment(yaml);
    let file = compose_file(crate::backend::ComposeTarget::Docker, &env).unwrap().file;
    guest(&env, &file, as_root).unwrap()
}

#[test]
fn a_root_way_in_runs_every_command_without_sudo() {
    let g = guest_as(WITH_SECRET, AsRoot::Root);
    let commands = g
        .start
        .iter()
        .chain([&g.status])
        .chain(g.check_setup.iter())
        .chain(g.check_teardown.iter())
        .chain(g.runners.iter().flat_map(|r| std::iter::once(&r.command).chain(&r.on_timeout)))
        .map(|c| c.command.to_string());
    let mut seen = 0;
    for c in commands {
        assert!(!c.contains("sudo"), "{c}");
        seen += 1;
    }
    assert!(seen > 3, "{seen}");
    assert!(
        g.start[0]
            .command
            .to_string()
            .starts_with("cd /opt/isoloom && env ISOLOOM_PUBLISH_ADDRESS=0.0.0.0")
    );
}

#[test]
fn compose_runs_from_the_vms_copy_under_the_files_own_project() {
    let g = guest_of(WITH_SECRET);
    let start = g.start[1].command.to_string();
    assert!(start.starts_with("cd /opt/isoloom && sudo -E env ISOLOOM_PUBLISH_ADDRESS=0.0.0.0 ISOLOOM_PUBLISH_FIXED=1 docker compose --project-name shop --file .isoloom/docker/compose.yml up --detach --build --pull missing --remove-orphans --wait --wait-timeout 600"), "{start}");
    // Each `up` after the pull of its images; web's set-up job runs attached once the rest is up.
    let ids: Vec<&str> = g.start.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["pull-start", "start", "pull-job-isoloom-init-web-1", "job-isoloom-init-web-1"]);
    assert!(
        g.start[3]
            .command
            .to_string()
            .ends_with("up --no-deps --exit-code-from isoloom-init-web-1 isoloom-init-web-1")
    );
    let ready: Vec<String> = g
        .ready
        .iter()
        .filter_map(|c| match c {
            Condition::StepOk { step, .. } => Some(step.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(ready, ["start", "job-isoloom-init-web-1"]);
    assert_eq!(
        g.status.command.to_string(),
        "if command -v docker >/dev/null; then sudo docker compose --project-name shop ps --all --format json; fi"
    );
}

#[test]
fn a_published_build_pulls_first_in_the_vm_as_on_the_host_its_failure_tolerated() {
    let g = guest_of("version: 1\nname: lab\nmachines:\n  web:\n    app:\n      image: ghcr.io/example/lab-web:1\n      build: { context: web }\n");
    let ids: Vec<&str> = g.start.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["pull", "pull-start", "start"]);
    assert!(g.start[0].command.to_string().ends_with("pull --ignore-pull-failures"));
    assert_eq!(g.start[0].failure, Failure::Tolerate);
    assert_eq!(g.start[0].note.as_deref(), Some("Pulling the environment's published images in the VM…"));
    assert!(!g.start[0].download, "a best effort, not the start's download");
    assert!(!g.start[2].command.to_string().contains("--build"));
    assert_eq!(g.start[2].failure, Failure::Fail);
}

#[test]
fn each_up_in_the_vm_pulls_its_images_first_in_a_download_of_its_own() {
    let g = guest_of(WITH_SECRET);
    let (pull, up) = (&g.start[0], &g.start[1]);
    // The same project, file, folder and variables as the `up`; its policy and services, the
    // images a service builds left to the `up`.
    let prefix = "cd /opt/isoloom && sudo -E env ISOLOOM_PUBLISH_ADDRESS=0.0.0.0 ISOLOOM_PUBLISH_FIXED=1 docker compose --project-name shop --file .isoloom/docker/compose.yml ";
    assert_eq!(
        pull.command.to_string(),
        format!("{prefix}pull --policy missing --ignore-buildable --include-deps cache isoloom-routes-cache web isoloom-routes-web isoloom-probe")
    );
    assert!(up.command.to_string().starts_with(&format!("{prefix}up ")));
    assert!(pull.download && !up.download);
    assert_eq!((pull.timeout_s, pull.failure), (900, Failure::Fail));
    assert_eq!(pull.note.as_deref(), Some("Pulling the environment's images in the VM…"));
    // A set-up job: its own service, no dependency.
    assert!(
        g.start[2]
            .command
            .to_string()
            .ends_with("pull --policy missing --ignore-buildable isoloom-init-web-1")
    );
    assert!(g.start[2].download);
}

#[test]
fn only_an_up_that_pulls_is_pulled_for() {
    let pulled = |line: &str| pull_args(&line.split(' ').map(str::to_owned).collect::<Vec<_>>()).map(|a| a.join(" "));
    assert_eq!(
        pulled("up --detach --pull always --wait --wait-timeout 600 db web").as_deref(),
        Some("pull --policy always --ignore-buildable --include-deps db web")
    );
    for not in [
        "up --detach --pull never web",
        "up --pull build web",
        "pull --ignore-pull-failures",
        "up --pull",
    ] {
        assert_eq!(pulled(not), None, "{not}");
    }
}

#[test]
fn the_secret_reaches_only_the_start_steps_and_is_forwarded_into_the_vm() {
    let g = guest_of(WITH_SECRET);
    let secret = g.slots.iter().find(|s| s.name.as_str() == "DEMO_SECRET").unwrap();
    assert_eq!(secret.kind, SlotKind::Input);
    let Some(Delivery::Env {
        var,
        steps: StepSelection::Only(steps),
    }) = secret.deliver.iter().next()
    else {
        panic!("{:?}", secret.deliver)
    };
    assert_eq!(var.as_str(), "DEMO_SECRET");
    let steps: Vec<&str> = steps.iter().map(StepId::as_str).collect();
    assert_eq!(steps, ["start", "job-isoloom-init-web-1"]);
    assert_eq!(g.forwarded(), ["DEMO_SECRET"]);
}

#[test]
fn every_published_port_is_fixed_and_its_address_left_to_the_vm() {
    let g = guest_of(WITH_SECRET);
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
    let g = guest_of(WITH_SECRET);
    let ids: Vec<&str> = g.runners.iter().map(|r| r.command.id.as_str()).collect();
    assert!(ids.contains(&"check-cache") && ids.contains(&"check-isoloom-exec-cache"), "{ids:?}");
    let exec = g.runners.iter().find(|r| r.command.id.as_str() == "check-isoloom-exec-cache").unwrap();
    assert!(
        exec.command
            .command
            .to_string()
            .ends_with("exec -T cache sh -s < .isoloom/docker/checks/isoloom-exec-cache.sh")
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
