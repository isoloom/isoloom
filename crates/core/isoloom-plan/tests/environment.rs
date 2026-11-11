//! The plan built from a spec: addresses, shared volumes, start order, refusals.

use std::net::Ipv4Addr;

use isoloom_plan::lifecycle::SharedRole;
use isoloom_plan::{Capabilities, Environment, Feature, MachineShape, Requirement, Step, VolumeKind, refusal};
use isoloom_spec::model::Target;
use testing::{Outcome, plan, plan_anyway};

pub mod testing;

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
    os: debian@12
    networks: { front: 10 }
    aliases: [www]
    depends_on: [db]
    ports: { web: { port: 80, publish: 8080 } }
    volumes: { shared: /srv/data }
    app: { image: nginx, steps: [{ run: web.sh }] }
  db:
    os: debian@12
    networks: { back: 20, front: 20 }
    ports: { postgres: 5432 }
    volumes: { shared: /var/data, own: /var/lib/db }
    app: { image: postgres, steps: [{ run: setup.sh }] }
";

fn env(target: Target) -> Outcome<Environment> {
    plan(SPEC, target)
}

#[test]
fn machines_have_their_addresses_names_and_ports() -> Outcome {
    let env = env(Target::Vagrant)?;
    let web = &env.machines["web"];
    assert_eq!(web.addresses["front"], Ipv4Addr::new(10, 30, 0, 10));
    assert_eq!(web.names, ["web", "www"]);
    assert_eq!(web.ports.len(), 1);
    assert_eq!((web.ports[0].port, web.ports[0].publish), (80, 8080));
    assert_eq!(web.shape, MachineShape::Vm);
    // web isn't on back: it routes there through the router, at front's last address.
    assert_eq!(web.routes.len(), 1);
    assert_eq!(web.routes[0].cidr.to_string(), "10.31.0.0/24");
    assert_eq!(env.router.as_ref().map(|r| r["front"]), Some(web.routes[0].via));
    assert_eq!(env.address_for("web", "db"), Ipv4Addr::new(10, 30, 0, 20));
    Ok(())
}

#[test]
fn docker_targets_lay_networks_on_their_docker_blocks() -> Outcome {
    let env = env(Target::Docker)?;
    assert_eq!(env.machines["web"].shape, MachineShape::Container);
    for n in env.networks.values() {
        assert_eq!(n.cidr.prefix(), n.declared_cidr.prefix());
    }
    assert!(env.machines["web"].lifecycle.is_empty(), "containers have no VM steps");
    Ok(())
}

#[test]
fn shared_volumes_name_their_owner_and_users() -> Outcome {
    let env = env(Target::Vagrant)?;
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
    Ok(())
}

#[test]
fn machines_start_after_what_they_wait_for() -> Outcome {
    let env = env(Target::Vagrant)?;
    assert_eq!(env.start_order, ["db", "web"]);
    let steps = &env.machines["web"].lifecycle;
    assert_eq!(steps.first(), Some(&Step::Prelude));
    assert_eq!(steps.last(), Some(&Step::Ready));
    assert!(steps.contains(&Step::WaitFor {
        machine: "db".into(),
        ports: vec![5432]
    }));
    Ok(())
}

#[test]
fn a_backend_refuses_what_it_lacks_the_capability_for() -> Outcome {
    let env = plan(&format!("{SPEC}provision:\n  - ansible: site.yml\n"), Target::Docker)?;
    assert_eq!(env.requirements, [Requirement::EnvironmentProvisioning, Requirement::MachineAliases]);
    let none = Capabilities::default();
    let why = refusal(&env.requirements, &none).expect("refused");
    assert_eq!(why.to_string(), "environment-level provisioning (`provision:`) runs on the VM targets only");
    let provisioning = Capabilities::of([Feature::EnvironmentProvisioning]);
    let next = refusal(&env.requirements, &provisioning).expect("refused");
    assert_eq!(next.to_string(), "machine `aliases` (dotted DNS names) aren't Kubernetes Service names");
    let both = Capabilities::of([Feature::EnvironmentProvisioning, Feature::MachineAliases]);
    assert!(refusal(&env.requirements, &both).is_none());
    Ok(())
}

#[test]
fn what_is_required_and_nothing_more_is_refused() -> Outcome {
    let env = env(Target::Docker)?;
    assert_eq!(env.requirements, [Requirement::MachineAliases]);
    assert!(refusal(&env.requirements, &Capabilities::of([Feature::MachineAliases])).is_none());
    Ok(())
}

#[test]
fn gateways_ulimits_and_tc_are_required_in_that_order() -> Outcome {
    let env = plan_anyway(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24, gateway: gw, tc: { delay: 50ms } }\nmachines:\n  gw:\n    networks: { lan: 1 }\n    app: { image: alpine }\n  web:\n    networks: { lan: 10 }\n    ulimits: { nofile: 1024 }\n    app: { image: nginx }\n",
        Target::Kubernetes,
    )?;
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
    Ok(())
}

#[test]
fn a_machine_process_is_required_and_planned_as_steps() -> Outcome {
    let env = plan(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nmachines:\n  web:\n    os: debian@12\n    networks: { lan: 10 }\n    app: { steps: [{ run: web.sh }] }\n    command: [serve]\n    setup: [seed.sh]\n    healthcheck: { test: 'true', interval: 1m }\n",
        Target::Vagrant,
    )?;
    assert_eq!(env.requirements, [Requirement::MachineProcess { machine: "web".into() }]);
    let why = refusal(&env.requirements, &Capabilities::default()).expect("refused");
    assert!(why.to_string().contains("an existing host runs what it already runs"), "{why}");
    let steps = &env.machines["web"].lifecycle;
    assert!(
        steps.ends_with(&[Step::Command, Step::Setup { paths: vec!["seed.sh".into()] }, Step::Ready]),
        "{steps:?}"
    );
    assert_eq!(env.machines["web"].process.health.as_ref().map(|h| h.interval), Some(60));
    Ok(())
}

#[test]
fn secrets_come_before_what_reads_them_on_linux_and_windows() -> Outcome {
    let env = plan(
        "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.9.0.0/24 }\nsecrets:\n  db: { env: DB }\nmachines:\n  web:\n    os: debian@12\n    networks: { lan: 10 }\n    app: { steps: [{ run: web.sh }] }\n    files: [{ path: /etc/a, content: x }, { path: /run/secrets/db, secret: db }]\n    command: [serve]\n  win:\n    os: windows-server@2022\n    networks: { lan: 11 }\n    files: [{ path: 'C:\\\\secrets\\\\db', secret: db }]\n",
        Target::Vagrant,
    )?;
    let steps = &env.machines["web"].lifecycle;
    let at = |want: fn(&Step) -> bool| steps.iter().position(want).expect("planned");
    let secrets = at(|s| *s == Step::Secrets);
    assert!(at(|s| matches!(s, Step::Files { .. })) < secrets, "{steps:?}");
    assert!(secrets < at(|s| matches!(s, Step::Run { .. })), "{steps:?}");
    assert!(secrets < at(|s| *s == Step::Command), "{steps:?}");
    // A Windows machine gets its secrets like any VM: nothing beyond `Secrets` is asked.
    assert!(env.machines["win"].lifecycle.contains(&Step::Secrets));
    assert!(refusal(&env.requirements, &Capabilities::of([Feature::Secrets, Feature::MachineProcess])).is_none());
    Ok(())
}

/// Every example of the repository, as its folder and its lab.
fn example_labs() -> Result<Vec<(std::path::PathBuf, isoloom_plan::Lab)>, Box<dyn std::error::Error>> {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(examples)? {
        let dir = entry?.path();
        if dir.join("isoloom.yml").is_file() {
            let lab = isoloom_plan::Lab::new(isoloom_spec::load(&dir)?);
            out.push((dir, lab));
        }
    }
    Ok(out)
}

/// A VM's `Routes` and `Ipv6` steps carry the routes its plan holds, worked out once: they were
/// worked out a second time for the steps.
#[test]
fn a_vms_route_steps_are_its_plans_routes() -> Result<(), Box<dyn std::error::Error>> {
    let mut seen = 0;
    for (dir, lab) in example_labs()? {
        for target in [Target::Vagrant, Target::Proxmox, Target::CloudVm] {
            let env = Environment::build(&lab.for_target(target), target, None).unwrap();
            for (name, mp) in &env.machines {
                for step in &mp.lifecycle {
                    match step {
                        Step::Routes { routes, default } => {
                            assert_eq!((routes, *default), (&mp.routes, mp.default_gateway), "{}: {name}", dir.display());
                            seen += 1;
                        }
                        Step::Ipv6 { routes, .. } => assert_eq!(routes, &mp.routes6, "{}: {name}", dir.display()),
                        _ => {}
                    }
                }
            }
        }
    }
    assert!(seen > 0);
    Ok(())
}

/// Every machine the plan runs as a VM is a `Vm`, with its OS, on every example and target: the
/// plan makes a machine a VM only from its `vm:` form, whose OS it keeps beside the shape, so
/// `Vm::of` never leaves a VM out.
#[test]
fn every_vm_of_the_plan_has_its_os() -> Result<(), Box<dyn std::error::Error>> {
    let mut vms = 0;
    for (dir, lab) in example_labs()? {
        for target in Target::ALL {
            let Ok(env) = Environment::build(&lab.for_target(target), target, None) else {
                continue;
            };
            for (name, _) in env.machines.iter().filter(|(_, mp)| mp.shape == MachineShape::Vm) {
                assert!(env.vm(name).is_some_and(|vm| !vm.os.is_empty()), "{}: {name} on {}", dir.display(), target.id());
                vms += 1;
            }
        }
    }
    assert!(vms > 0);
    Ok(())
}
