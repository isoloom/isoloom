//! `isoloom import compose` against a fixture written the way people write Compose files:
//! what the format expresses lands in a valid draft, and everything else is a note.

use std::path::Path;

use isoloom_core::import::NoteKind;
use isoloom_core::import::compose::{draft, label};
use isoloom_core::{parse, validate};

fn shop() -> isoloom_core::import::Draft {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compose-shop/docker-compose.yml");
    draft(&std::fs::read_to_string(path).unwrap(), "fallback", "docker-compose.yml").unwrap()
}

fn has(d: &isoloom_core::import::Draft, kind: NoteKind, at: &str, text: &str) -> bool {
    d.notes.iter().any(|n| n.kind == kind && n.at == at && n.text.contains(text))
}

#[test]
fn the_draft_is_a_valid_spec() {
    let d = shop();
    let spec = parse(&d.yaml).expect("the draft parses");
    assert_eq!(validate(&spec), vec![], "{}", d.yaml);
    assert_eq!(spec.name, "shop-demo");
    assert!(d.yaml.starts_with(isoloom_core::schema::MODELINE));
    assert!(d.yaml.contains("# Drafted by `isoloom import compose` from docker-compose.yml."));
}

#[test]
fn what_the_format_expresses_is_carried_over() {
    let spec = parse(&shop().yaml).unwrap();
    // Networks: a usable subnet is kept, an unusable one re-addressed; internal = offline.
    assert_eq!(spec.networks["back"].cidr, "10.31.2.0/24");
    assert!(!spec.networks["back"].internet);
    assert_eq!(spec.networks["front"].cidr, "10.88.1.0/24");
    let web = &spec.machines["web"];
    // The fixed address's last octet survives re-addressing.
    assert_eq!(web.networks["front"], 10);
    assert_eq!(web.services.iter().map(|s| s.port).collect::<Vec<_>>(), [80]);
    assert_eq!(web.services[0].publish, Some(8080), "the host port carries over");
    assert_eq!(web.inputs, ["API_TOKEN"]);
    assert_eq!(spec.inputs, ["API_TOKEN"]);
    let r = web.resources.unwrap();
    assert_eq!((r.cpus, r.memory_mb), (Some(1), Some(256)));
    assert_eq!(web.depends_on, ["cache"]);
    assert_eq!(web.docker.as_ref().unwrap().image.as_deref(), Some("nginx:1.27-alpine"));
    assert_eq!(spec.machines["cache"].services[0].port, 6379, "expose becomes a service");
    assert_eq!(spec.machines["cache"].volumes["cache-data"], "/data", "a named volume carries over");
}

#[test]
fn jobs_and_profile_services_are_left_out_with_a_note() {
    let d = shop();
    let spec = parse(&d.yaml).unwrap();
    assert!(!spec.machines.contains_key("seed"), "a one-shot job would restart forever as a machine");
    assert!(!spec.machines.contains_key("tester"), "Compose doesn't start profile-only services");
    assert!(has(&d, NoteKind::Changed, "services.seed", "runs once and exits"));
    assert!(has(&d, NoteKind::Changed, "services.tester", "only started with profile test"));
}

#[test]
fn everything_else_is_a_note_with_its_reason() {
    let d = shop();
    assert!(has(&d, NoteKind::Changed, "networks.front.ipam", "re-addressed to 10.88.1.0/24"));
    assert!(has(&d, NoteKind::InImage, "services.web.environment", "MODE=production"));
    assert!(!d.notes.iter().any(|n| n.at.contains("volumes")), "named volumes carry over: {:?}", d.notes);
    assert!(has(&d, NoteKind::Changed, "services.web.ports", "loopback"));
    assert!(has(&d, NoteKind::Equivalent, "services.cache.healthcheck", "probes every service port"));
}

#[test]
fn renames_point_at_the_settings_that_still_use_the_old_name() {
    let compose = "services:\n  db.main:\n    image: postgres:16\n    expose: [5432]\n  app:\n    image: app\n    environment: { DB_HOST: db.main }\n";
    let d = draft(compose, "x", "compose.yaml").unwrap();
    assert!(d.yaml.contains("  db-main:"));
    assert!(has(&d, NoteKind::Changed, "services.db.main", "the environment of app still says `db.main`"));
    assert_eq!(label("My_App 2"), "my-app-2");
    assert_eq!(label("9lives"), "m-9lives");
}

#[test]
fn not_a_compose_file_is_an_error() {
    assert!(draft("just text", "x", "f").is_err());
    assert!(draft("services: {}\n", "x", "f").is_err());
}

#[test]
fn volumes_shared_between_services_are_noted() {
    let compose = "services:\n  a:\n    image: x\n    volumes: [\"shared:/data\", \"./conf:/etc/x:ro\", \"/cache\"]\n  b:\n    image: x\n    volumes: [{ type: volume, source: shared, target: /in }]\nvolumes: { shared: {} }\n";
    let d = draft(compose, "x", "compose.yaml").unwrap();
    let spec = parse(&d.yaml).unwrap();
    assert_eq!(validate(&spec), vec![]);
    assert_eq!(
        spec.machines["a"].volumes.get("cache").map(String::as_str),
        Some("/cache"),
        "anonymous volume kept"
    );
    assert!(!spec.machines["a"].volumes.contains_key("shared"));
    assert!(has(&d, NoteKind::NotYet, "services.a.volumes", "shared:/data"));
    assert!(has(&d, NoteKind::InImage, "services.a.volumes", "./conf:/etc/x:ro"));
}

#[test]
fn published_ports_read_every_compose_form() {
    let compose =
        "services:\n  a:\n    image: x\n    ports: [\"127.0.0.1:8443:443\", \"${WEB_PORT:-8080}:80\", \"9000\", { target: 5432, published: 15432 }]\n";
    let d = draft(compose, "x", "compose.yaml").unwrap();
    let spec = parse(&d.yaml).unwrap();
    let svc = |p: u16| spec.machines["a"].services.iter().find(|s| s.port == p).unwrap().publish;
    assert_eq!((svc(443), svc(80), svc(9000), svc(5432)), (Some(8443), Some(8080), None, Some(15432)));
    assert!(has(&d, NoteKind::Changed, "services.a.ports", "no fixed host port for 9000"));
}

// Vagrant: from what a Vagrantfile set when it ran (recorded by the CLI's vagrant_record.rb).

fn recorded(name: &str) -> serde_json::Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vagrant-classic").join(name);
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn a_vagrantfile_with_a_loop_becomes_a_valid_spec() {
    let d = isoloom_core::import::vagrant::draft(&recorded("recorded.json"), "classic", "Vagrantfile").unwrap();
    let spec = parse(&d.yaml).expect("the draft parses");
    assert_eq!(validate(&spec), vec![], "{}", d.yaml);
    assert_eq!(spec.networks["lab"].cidr, "192.168.33.0/24");
    let web = &spec.machines["web"];
    assert_eq!(web.networks["lab"], 10);
    assert_eq!((web.services[0].port, web.services[0].publish), (80, Some(8080)));
    assert_eq!(web.resources.unwrap().memory_mb, Some(1024));
    assert_eq!(spec.machines["db"].resources.unwrap().memory_mb, Some(2048));
    let vm = web.vm.as_ref().unwrap();
    assert_eq!(vm.image.as_ref().unwrap().vagrant.as_deref(), Some("ubuntu/jammy64"), "an unknown box is kept");
    assert_eq!(vm.provision, ["site.yml", "provision/web.sh"], "global provisioners run first");
    assert!(has(&d, NoteKind::InImage, "define web.vm.provision", "inline script"));
}

#[test]
fn importing_an_isoloom_vagrantfile_gives_back_the_environment() {
    let d = isoloom_core::import::vagrant::draft(&recorded("segmented-recorded.json"), "segmented", "Vagrantfile").unwrap();
    let spec = parse(&d.yaml).unwrap();
    let original = isoloom_core::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/segmented")).unwrap();
    let cidrs = |s: &isoloom_core::Spec| {
        let mut v: Vec<String> = s.networks.values().map(|n| n.cidr.clone()).collect();
        v.sort();
        v
    };
    assert_eq!(cidrs(&spec), cidrs(&original));
    for name in ["cache", "web", "user"] {
        assert!(spec.machines.contains_key(name), "{name} comes back");
    }
    assert!(!spec.machines.contains_key("isoloom-router"), "Isoloom's own router isn't a machine");
    assert_eq!(spec.machines["web"].vm.as_ref().unwrap().os, "debian-12");
    assert!(spec.machines["web"].vm.as_ref().unwrap().image.is_none(), "a built-in box needs no override");
}
