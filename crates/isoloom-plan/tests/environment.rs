//! The plan built from a spec: addresses, shared volumes, start order, refusals.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::net::Ipv4Addr;

use isoloom_plan::lifecycle::SharedRole;
use isoloom_plan::{Capabilities, Environment, Feature, MachineShape, Requirement, Step, VolumeKind, refusal};
use isoloom_spec::model::Target;

const SPEC: &str = r"
version: 1
name: plan
networks:
  front: { cidr: 10.30.0.0/24 }
  back: { cidr: 10.31.0.0/24, internet: false }
reach:
  - { from: front, to: back, ports: [5432] }
volumes:
  shared: { owner: db }
machines:
  web:
    networks: { front: 10 }
    aliases: [www]
    depends_on: [db]
    services: [{ port: 80, publish: 8080 }]
    volumes: { shared: /srv/data }
    docker: { image: nginx }
    vm: { os: debian-12, provision: [web.sh] }
  db:
    networks: { back: 20, front: 20 }
    services: [{ port: 5432 }]
    volumes: { shared: /var/data, own: /var/lib/db }
    docker: { image: postgres }
    vm: { os: debian-12, provision: [setup.sh] }
";

fn env(target: Target) -> Environment {
    let spec = isoloom_spec::parse(SPEC).expect("parses");
    assert!(isoloom_spec::validate(&spec).is_empty(), "{:?}", isoloom_spec::validate(&spec));
    Environment::build(&spec, target, None)
}

#[test]
fn machines_have_their_addresses_names_and_ports() {
    let env = env(Target::Vagrant);
    let web = &env.machines["web"];
    assert_eq!(web.addresses["front"], Ipv4Addr::new(10, 30, 0, 10));
    assert_eq!(web.names, ["web", "www"]);
    assert_eq!(web.ports.len(), 1);
    assert_eq!((web.ports[0].port, web.ports[0].publish), (80, 8080));
    assert_eq!(web.shape, MachineShape::Vm);
    // web isn't on back: it routes there through the router, at front's last address.
    assert_eq!(web.routes.len(), 1);
    assert_eq!(web.routes[0].cidr, "10.31.0.0/24");
    assert_eq!(env.router.as_ref().map(|r| r["front"]), Some(web.routes[0].via));
    assert_eq!(env.address_for("web", "db"), Ipv4Addr::new(10, 30, 0, 20));
}

#[test]
fn docker_targets_lay_networks_on_their_docker_blocks() {
    let env = env(Target::Docker);
    assert_eq!(env.machines["web"].shape, MachineShape::Container);
    for n in env.networks.values() {
        assert_eq!(n.cidr.rsplit('/').next(), n.declared_cidr.rsplit('/').next());
    }
    assert!(env.machines["web"].lifecycle.is_empty(), "containers have no VM steps");
}

#[test]
fn shared_volumes_name_their_owner_and_users() {
    let env = env(Target::Vagrant);
    let db = &env.machines["db"];
    let shared = db.volumes.iter().find(|v| v.name == "shared").expect("shared");
    assert_eq!(
        shared.kind,
        VolumeKind::Shared {
            owner: "db".into(),
            users: vec!["web".into(), "db".into()]
        }
    );
    let own = db.volumes.iter().find(|v| v.name == "own").expect("own");
    assert_eq!(own.kind, VolumeKind::Own);
    // The owner serves, the other mounts the owner's path at the address it reaches it at.
    let serve = db
        .lifecycle
        .iter()
        .find(|s| matches!(s, Step::SharedVolume { .. }))
        .expect("a shared volume step");
    assert!(matches!(serve, Step::SharedVolume { role: SharedRole::Serve { clients, .. }, .. } if clients == &[Ipv4Addr::new(10, 30, 0, 10)]));
    let mount = env.machines["web"]
        .lifecycle
        .iter()
        .find(|s| matches!(s, Step::SharedVolume { .. }))
        .expect("a mount step");
    assert!(matches!(mount, Step::SharedVolume { role: SharedRole::Mount { source, .. }, .. } if source == "10.30.0.20:/var/data"));
}

#[test]
fn machines_start_after_what_they_wait_for() {
    let env = env(Target::Vagrant);
    assert_eq!(env.start_order, ["db", "web"]);
    let steps = &env.machines["web"].lifecycle;
    assert_eq!(steps.first(), Some(&Step::Prelude));
    assert_eq!(steps.last(), Some(&Step::Ready));
    assert!(steps.contains(&Step::WaitFor {
        machine: "db".into(),
        ports: vec![5432]
    }));
}

#[test]
fn a_backend_refuses_what_it_lacks_the_capability_for() {
    let spec = isoloom_spec::parse(&format!("{SPEC}provision:\n  - ansible: site.yml\n")).expect("parses");
    let env = Environment::build(&spec, Target::Docker, None);
    assert_eq!(env.requirements, [Requirement::EnvironmentProvisioning, Requirement::MachineAliases]);
    let none = Capabilities::default();
    let why = refusal(&env.requirements, &none).expect("refused");
    assert_eq!(why.to_string(), "environment-level provisioning (`provision:`) runs on VM targets for now");
    let provisioning = Capabilities::of([Feature::EnvironmentProvisioning]);
    let why = refusal(&env.requirements, &provisioning).expect("refused");
    assert_eq!(why.to_string(), "machine `aliases` (dotted DNS names) aren't Kubernetes Service names");
    let both = Capabilities::of([Feature::EnvironmentProvisioning, Feature::MachineAliases]);
    assert!(refusal(&env.requirements, &both).is_none());
}

#[test]
fn what_is_required_and_nothing_more_is_refused() {
    let env = env(Target::Docker);
    assert_eq!(env.requirements, [Requirement::MachineAliases]);
    assert!(refusal(&env.requirements, &Capabilities::of([Feature::MachineAliases])).is_none());
}

#[test]
fn gateways_ulimits_and_tc_are_required_in_that_order() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24, gateway: gw, tc: { delay: 50ms } }\nmachines:\n  gw:\n    networks: { lan: 1 }\n    docker: { image: alpine }\n  web:\n    networks: { lan: 10 }\n    ulimits: { nofile: 1024 }\n    docker: { image: nginx }\n",
    )
    .expect("parses");
    let env = Environment::build(&spec, Target::Kubernetes, None);
    assert_eq!(
        env.requirements,
        [
            Requirement::Gateway { network: "lan".into() },
            Requirement::Ulimits { machine: "web".into() },
            Requirement::TrafficControl
        ]
    );
    let why = refusal(&env.requirements, &Capabilities::of([Feature::Gateways])).expect("refused");
    assert_eq!(why.to_string(), "machine `web` sets `ulimits`: Kubernetes has no per-container process limits");
}

#[test]
fn a_machine_process_is_required_and_planned_as_steps() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  web:\n    networks: { lan: 10 }\n    vm: { os: debian-12, provision: [web.sh] }\n    command: [serve]\n    before: [flag.sh]\n    healthcheck: { test: 'true', interval: 1m }\n",
    )
    .expect("parses");
    let env = Environment::build(&spec, Target::Vagrant, None);
    assert_eq!(env.requirements, [Requirement::MachineProcess { machine: "web".into() }]);
    let why = refusal(&env.requirements, &Capabilities::default()).expect("refused");
    assert!(why.to_string().contains("an existing host runs what it already runs"), "{why}");
    let steps = &env.machines["web"].lifecycle;
    assert!(
        steps.ends_with(&[Step::Before { paths: vec!["flag.sh".into()] }, Step::Command, Step::Ready]),
        "{steps:?}"
    );
    assert_eq!(env.machines["web"].process.health.as_ref().map(|h| h.interval), Some(60));
}

#[test]
fn secrets_come_before_what_reads_them_on_linux_and_windows() {
    let spec = isoloom_spec::parse(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nsecrets:\n  db: { env: DB }\nmachines:\n  web:\n    networks: { lan: 10 }\n    vm: { os: debian-12, provision: [web.sh] }\n    files: [{ path: /etc/a, content: x }]\n    secrets: [db]\n    command: [serve]\n  win:\n    networks: { lan: 11 }\n    vm: { os: windows-server-2022 }\n    secrets: [db]\n",
    )
    .expect("parses");
    let env = Environment::build(&spec, Target::Vagrant, None);
    let steps = &env.machines["web"].lifecycle;
    let at = |want: fn(&Step) -> bool| steps.iter().position(want).expect("planned");
    let secrets = at(|s| *s == Step::Secrets);
    assert!(at(|s| matches!(s, Step::Files { .. })) < secrets, "{steps:?}");
    assert!(secrets < at(|s| matches!(s, Step::Run { .. })), "{steps:?}");
    assert!(secrets < at(|s| *s == Step::Command), "{steps:?}");
    // A Windows machine gets its secrets like any VM: nothing beyond `Secrets` is asked.
    assert!(env.machines["win"].lifecycle.contains(&Step::Secrets));
    assert!(refusal(&env.requirements, &Capabilities::of([Feature::Secrets, Feature::MachineProcess])).is_none());
}
