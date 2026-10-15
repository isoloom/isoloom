use std::path::Path;

use isoloom_spec::model::{MachineSecret, SecretSource, Target};
use isoloom_spec::syntax::Code;

use super::*;

/// A design example of `plan/phase2/spec-examples`, parsed.
fn example(name: &str) -> syntax::Spec {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../plan/phase2/spec-examples")
        .join(name)
        .join("isoloom.yml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    syntax::parse(&text).unwrap_or_else(|p| panic!("{name}: {p:?}"))
}

fn on(name: &str, target: Target) -> Lowered {
    lower(&example(name), Scope::Target(target))
}

#[test]
fn a_flag_becomes_a_secret_file_read_from_its_input() {
    let l = on("help-desk", Target::Docker);
    assert_eq!(l.spec.secrets["input-flag"], SecretSource::Input { input: "FLAG".into() });
    let web = &l.spec.machines["web"];
    let MachineSecret::Mount(m) = &web.secrets[0] else { panic!() };
    assert_eq!(
        (m.source.as_str(), m.target.as_deref(), m.uid, m.mode),
        ("input-flag", Some("/srv/archive/flag.txt"), Some(1000), Some(0o400))
    );
    assert!(web.inputs.is_empty() && web.environment.is_empty());
    let d = web.docker.as_ref().unwrap();
    assert_eq!(d.build.as_deref(), Some("build/web"));
    let p = &web.ports[0];
    assert_eq!(
        (p.name.as_deref(), p.port, p.http, p.publish, p.fixed),
        (Some("site"), 8000, true, Some(8080), false)
    );
    assert_eq!(web.healthcheck.as_ref().and_then(|h| h.http.as_ref()).map(|h| h.port), Some(8000));
    assert_eq!(l.refusals, []);
}

#[test]
fn a_container_only_machine_is_refused_on_a_vm_target_at_its_recipe() {
    let l = on("help-desk", Target::Vagrant);
    assert_eq!(l.refusals.len(), 1);
    assert_eq!((l.refusals[0].code, l.refusals[0].path.as_str()), (Code::Refused, "machines.web.app.build"));
    assert!(l.spec.machines["web"].vm.is_none());
}

#[test]
fn each_target_gets_its_own_overrides() {
    let docker = on("legacy-intranet", Target::Docker);
    assert_eq!(docker.spec.networks["office"].docker.as_ref().map(|d| d.cidr.as_str()), Some("10.70.0.0/24"));
    let portal = &docker.spec.machines["portal"];
    let generated = portal.docker.as_ref().and_then(|d| d.generated.as_ref()).unwrap();
    assert!(generated.base.starts_with("debian:12@sha256:") && generated.steps.len() == 3);
    assert_eq!(portal.vm.as_ref().unwrap().provision, ["provision/portal.sh"]);
    assert_eq!(
        portal.vm.as_ref().and_then(|v| v.image.as_ref()).and_then(|i| i.vagrant.as_deref()),
        Some("example/debian-12-lts")
    );
    assert_eq!(
        portal.vm.as_ref().and_then(|v| v.image.as_ref()).and_then(|i| i.aws.as_deref()),
        Some("ami-0123456789abcdef0")
    );
    let clerk = docker.spec.machines["clerk"].docker.as_ref().unwrap();
    assert!(clerk.idle && clerk.image.as_deref().is_some_and(|i| i.starts_with("debian:12@sha256:")));
    assert!(docker.spec.volumes["uploads"].virtiofs.is_some());
    // On Proxmox only, the status board runs as an LXC container from its image.
    let proxmox = on("legacy-intranet", Target::Proxmox);
    let status = &proxmox.spec.machines["status"];
    assert!(status.docker.as_ref().is_some_and(|d| d.lxc) && status.vm.is_none());
    assert!(docker.spec.machines["status"].vm.is_some() && !docker.spec.machines["status"].docker.as_ref().unwrap().lxc);
    assert_eq!(docker.refusals, []);
    // The portal's and the scanner's `copy` and `packages` steps wait for VM configuration
    // steps (Phase 4); the status board's VM recipe is a script.
    assert!(
        proxmox
            .refusals
            .iter()
            .all(|r| r.path.as_str().starts_with("machines.portal.") || r.path.as_str().starts_with("machines.scanner."))
    );
}

#[test]
fn a_windows_domain_lowers_onto_vms() {
    let l = on("forest-mini", Target::Vagrant);
    // The station's `packages` step waits for VM configuration steps (Phase 4).
    let refused: Vec<&str> = l.refusals.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(refused, ["machines.station.app.steps[0]"]);
    let dc = &l.spec.machines["dc01"];
    let MachineSecret::Mount(m) = &dc.secrets[0] else { panic!() };
    assert_eq!((m.source.as_str(), m.mode), ("input-flag-admin", None));
    for server in ["dc01", "srv01"] {
        let image = l.spec.machines[server].vm.as_ref().and_then(|v| v.image.clone()).unwrap();
        assert_eq!(
            (image.vagrant.as_deref(), image.winrm),
            (Some("example/windows-server-2019-eval"), Some(isoloom_spec::model::Winrm::Ssl))
        );
    }
    assert!(l.spec.machines["ws01"].vm.as_ref().and_then(|v| v.image.as_ref()).is_none());
    assert_eq!(l.spec.machines["station"].dns.as_ref().unwrap().servers, ["10.56.10.10"]);
    assert_eq!(l.spec.machines["srv01"].vm.as_ref().unwrap().provision, ["provision/iis.ps1"]);
    // Windows has no container form: the container targets refuse it at `os`.
    let docker = on("forest-mini", Target::Docker);
    assert!(docker.refusals.iter().any(|r| r.path.as_str() == "machines.dc01.os"), "{:?}", docker.refusals);
}

#[test]
fn the_lab_scope_applies_no_targets_own_facts() {
    let l = lower(&example("legacy-intranet"), Scope::Lab);
    assert!(!l.spec.machines["status"].docker.as_ref().unwrap().lxc && l.refusals.is_empty());
    assert_eq!(l.trail.path("machines.portal.ports[0].publish").as_str(), "machines.portal.ports.web.publish");
}

#[test]
fn configuration_steps_on_a_vm_wait_for_phase_3() {
    let l = on("branch-office", Target::Vagrant);
    let paths: Vec<&str> = l.refusals.iter().map(|r| r.path.as_str()).collect();
    assert!(
        paths.contains(&"machines.edge.app.steps[0]") && paths.contains(&"machines.web.app.steps[2]"),
        "{paths:?}"
    );
    assert!(on("branch-office", Target::Docker).refusals.iter().all(|r| !r.path.as_str().contains("steps")));
}

#[test]
fn the_design_examples_validate_but_for_what_waits_on_later_phases() {
    let problems = |name: &str| -> Vec<(Code, String)> {
        crate::Lab::new(example(name))
            .problems()
            .into_iter()
            .map(|p| (p.code, p.path.to_string()))
            .collect()
    };
    assert_eq!(problems("help-desk"), []);
    assert_eq!(problems("legacy-intranet"), []);
    // `windows` is a group name Isoloom fills itself (the inventory's).
    assert_eq!(problems("forest-mini"), [(Code::InvalidValue, "groups.windows".to_owned())]);
    // The catalogue arrives in Phase 4.
    assert_eq!(problems("branch-office"), [(Code::UnknownReference, "machines.db.service".to_owned())]);
    let bookshelf = problems("bookshelf");
    assert!(bookshelf.contains(&(Code::UnknownReference, "machines.db.service".to_owned())), "{bookshelf:?}");
    assert!(bookshelf.iter().all(|(c, _)| *c == Code::UnknownReference), "{bookshelf:?}");
}

#[test]
fn a_lab_lists_the_targets_its_machines_have_forms_for() {
    let lab = crate::Lab::new(example("help-desk"));
    assert_eq!(
        lab.possible(),
        [Target::Docker, Target::Hosted, Target::DockerVm, Target::CloudDocker, Target::Kubernetes]
    );
    let refused = lab.refusal(Target::Vagrant).unwrap();
    assert_eq!((refused.path.as_str(), refused.span.map(|s| s.line)), ("machines.web.app.build", Some(21)));
    let forest = crate::Lab::new(example("forest-mini"));
    assert_eq!(forest.possible(), [Target::Vagrant, Target::Proxmox, Target::Esxi, Target::CloudVm]);
}

#[test]
fn a_catalogue_service_is_refused_at_its_service_until_its_entry_exists() {
    for target in [Target::Docker, Target::Vagrant, Target::Hybrid] {
        let l = on("branch-office", target);
        let db: Vec<&str> = l
            .refusals
            .iter()
            .filter(|r| r.path.as_str().starts_with("machines.db"))
            .map(|r| r.path.as_str())
            .collect();
        assert_eq!(db, ["machines.db.service"], "{target:?}");
        let machine = &l.spec.machines["db"];
        assert!(machine.docker.is_none() && machine.vm.is_none(), "{target:?}");
    }
    assert_eq!(crate::Lab::new(example("branch-office")).possible(), []);
}

#[test]
fn what_lowering_cant_lower_is_a_problem_at_the_authored_field() {
    let lab = crate::testing::lab(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web:\n    networks: { lan: 10 }\n    app: { image: nginx }\n    ports: { site: { protocol: http }, admin: 8080 }\n    healthcheck: { http: { port: status, path: /health } }\n",
    );
    let l = lower(lab.source(), Scope::Lab);
    let found: Vec<(Code, &str)> = l.problems.iter().map(|p| (p.code, p.path.as_str())).collect();
    assert_eq!(
        found,
        [
            (Code::MissingField, "machines.web.ports.site.port"),
            (Code::UnknownReference, "machines.web.healthcheck.http.port")
        ]
    );
    let web = &l.spec.machines["web"];
    let ports: Vec<u16> = web.ports.iter().map(|p| p.port).collect();
    assert_eq!(ports, [8080]);
    assert!(web.healthcheck.is_none());
    // The spec's own checks say the same, once each.
    let problems: Vec<String> = lab.problems().iter().map(|p| p.path.to_string()).collect();
    assert_eq!(problems, ["machines.web.ports.site.port", "machines.web.healthcheck.http.port"]);
}

#[test]
fn a_healthcheck_page_on_a_named_port_takes_its_number() {
    let lab = crate::testing::lab(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web:\n    networks: { lan: 10 }\n    app: { image: nginx }\n    ports: { site: 8000 }\n    healthcheck: { http: { port: site, path: /health } }\n",
    );
    let l = lower(lab.source(), Scope::Lab);
    assert_eq!(l.problems, []);
    let page = l.spec.machines["web"].healthcheck.as_ref().and_then(|h| h.http.clone()).expect("a page");
    assert_eq!((page.port, page.path.as_str()), (8000, "/health"));
}

#[test]
fn a_dns_server_named_by_machine_is_its_address_on_a_network_both_share() {
    let lab = crate::testing::lab(
        "version: 1\nname: t\nnetworks: { a: { cidr: 10.1.0.0/24 }, b: { cidr: 10.2.0.0/24 } }\nmachines:\n  dns: { networks: { a: 53, b: 53 }, app: { image: coredns } }\n  pc: { networks: { b: 10 }, dns: { servers: [dns, 9.9.9.9] }, app: { image: x } }\n  far: { networks: { a: 11 }, dns: { servers: [pc] }, app: { image: x } }\n",
    );
    let l = lower(lab.source(), Scope::Lab);
    let servers = |m: &str| l.spec.machines[m].dns.as_ref().map(|d| d.servers.clone()).unwrap_or_default();
    assert_eq!(servers("pc"), ["10.2.0.53", "9.9.9.9"]);
    assert_eq!(servers("far"), ["pc"], "they share no network: the name stays");
    let docker = lower(lab.source(), Scope::Target(Target::Docker));
    assert!(docker.spec.machines["pc"].dns.as_ref().is_some_and(|d| d.servers[0] != "dns"));
}

#[test]
fn the_controller_takes_its_vagrant_box_from_the_overrides() {
    let controller = |overrides: &str| {
        let yaml = format!("version: 1\nname: t\nmachines:\n  m: {{ os: debian@12 }}\n{overrides}");
        lower(crate::testing::lab(&yaml).source(), Scope::Lab).spec.controller
    };
    assert_eq!(controller(""), None);
    let boxed = controller("overrides: { vagrant: { controller: { box: corp/ansible, box_version: '2.1' } } }\n").and_then(|c| c.image);
    assert_eq!(
        boxed.map(|i| (i.vagrant, i.vagrant_version)),
        Some((Some("corp/ansible".to_owned()), Some("2.1".to_owned())))
    );
    let pinned = controller("overrides: { vagrant: { controller: { box_version: '2.1' } } }\n").and_then(|c| c.image);
    assert_eq!(pinned.map(|i| (i.vagrant, i.vagrant_version)), Some((None, Some("2.1".to_owned()))));
    let sized = controller("controller: { resources: { memory_mb: 768 } }\n").expect("the spec's controller");
    assert_eq!((sized.image, sized.resources.and_then(|r| r.memory_mb)), (None, Some(768)));
}
