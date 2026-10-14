//! The plan built from a spec: addresses, shared volumes, start order, refusals.

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
    assert!(matches!(serve, Step::SharedVolume { role: SharedRole::Serve { clients }, .. } if clients == &[Ipv4Addr::new(10, 30, 0, 10)]));
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
    assert_eq!(env.requirements, [Requirement::EnvironmentProvisioning]);
    let none = Capabilities::default();
    let why = refusal(&env.requirements, &none).expect("refused");
    assert_eq!(why.to_string(), "environment-level provisioning (`provision:`) runs on VM targets for now");
    assert!(refusal(&env.requirements, &Capabilities::of([Feature::EnvironmentProvisioning])).is_none());
}

#[test]
fn nothing_required_nothing_refused() {
    let env = env(Target::Docker);
    assert!(env.requirements.is_empty());
    assert!(refusal(&env.requirements, &Capabilities::default()).is_none());
}
