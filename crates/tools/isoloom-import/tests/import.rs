//! `isoloom import compose` against a fixture written the way people write Compose files:
//! what the format expresses lands in a valid draft, and everything else is a note.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_import::NoteKind;
use isoloom_import::compose::{draft, label};
use isoloom_spec::parse;
use isoloom_spec::syntax::{self, Kind, Publish, Recipe, Spec, Step};

/// The problems `isoloom validate` would show for a draft.
fn validate(spec: &Spec) -> Vec<String> {
    syntax::check(spec).iter().map(syntax::Problem::line).collect()
}

/// A machine's ports: (name, port, published host port, UDP).
fn ports(spec: &Spec, machine: &str) -> Vec<(String, u16, Option<u16>, bool)> {
    spec.machines
        .get(machine)
        .into_iter()
        .flat_map(|m| &m.ports)
        .map(|(name, p)| {
            let publish = p.publish.map(|p| match p {
                Publish::Preferred(h) | Publish::Fixed(h) => h.get(),
                Publish::Free => 0,
            });
            (name.to_string(), p.port.unwrap().get(), publish, !p.protocol.is_tcp())
        })
        .collect()
}

fn image(spec: &Spec, machine: &str) -> Option<String> {
    match &spec.machines.get(machine)?.kind {
        Kind::App(a) => match &a.container {
            Some(Recipe::Image(i)) => Some(i.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn steps(spec: &Spec, machine: &str) -> Vec<String> {
    match spec.machines.get(machine).map(|m| &m.kind) {
        Some(Kind::App(a)) => a
            .steps
            .iter()
            .map(|s| match s {
                Step::Run { script, .. } => script.clone(),
                Step::Playbook(p) => p.clone(),
                other => format!("{other:?}"),
            })
            .collect(),
        _ => vec![],
    }
}

fn shop() -> isoloom_import::Draft {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compose-shop/docker-compose.yml");
    draft(&std::fs::read_to_string(path).unwrap(), "fallback", "docker-compose.yml").unwrap()
}

fn has(d: &isoloom_import::Draft, kind: NoteKind, at: &str, text: &str) -> bool {
    d.notes.iter().any(|n| n.kind == kind && n.at == at && n.text.contains(text))
}

#[test]
fn the_draft_is_a_valid_spec() {
    let d = shop();
    let spec = parse(&d.yaml).expect("the draft parses");
    assert_eq!(validate(&spec), Vec::<String>::new(), "{}", d.yaml);
    assert_eq!(spec.name, "shop-demo");
    assert!(d.yaml.starts_with(isoloom_spec::schema::MODELINE));
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
    assert_eq!(web.networks["front"], syntax::Octets::One(10));
    assert_eq!(ports(&spec, "web"), [("http".to_owned(), 80, Some(8080), false)], "the host port carries over");
    assert!(
        matches!(&web.environment["API_TOKEN"], syntax::EnvValue::Input(i) if i.as_str() == "API_TOKEN"),
        "an interpolated variable becomes an input: {:?}",
        web.environment
    );
    assert!(spec.inputs.0.keys().any(|k| k.as_str() == "API_TOKEN"));
    let r = web.resources.unwrap();
    assert_eq!((r.cpus, r.memory_mb), (Some(1), Some(256)));
    assert_eq!(web.depends_on, ["cache"]);
    assert_eq!(image(&spec, "web").as_deref(), Some("nginx:1.27-alpine"));
    assert_eq!(ports(&spec, "cache"), [("redis".to_owned(), 6379, None, false)], "expose becomes a port");
    assert_eq!(spec.machines["cache"].volumes["cache-data"].path(), "/data", "a named volume carries over");
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
    assert_eq!(validate(&spec), Vec::<String>::new());
    assert_eq!(
        spec.machines["a"].volumes.get("cache").map(isoloom_spec::model::Mount::path),
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
    let svc = |p: u16| ports(&spec, "a").into_iter().find(|s| s.1 == p).unwrap().2;
    assert_eq!((svc(443), svc(80), svc(9000), svc(5432)), (Some(8443), Some(8080), None, Some(15432)));
    assert!(has(&d, NoteKind::Changed, "services.a.ports", "no fixed host port for 9000"));
}

#[test]
fn udp_ports_are_imported_as_udp_services() {
    let compose = "services:\n  dns:\n    image: x\n    ports: [\"5353:53/udp\", \"53:53\", { target: 69, published: 6969, protocol: udp }]\n    expose: [\"161/udp\"]\n  app:\n    image: x\n    depends_on: [dns]\n";
    let d = draft(compose, "x", "compose.yaml").unwrap();
    let spec = parse(&d.yaml).unwrap();
    assert_eq!(validate(&spec), Vec::<String>::new(), "{}", d.yaml);
    let all = ports(&spec, "dns");
    let svc = |p: u16, udp: bool| {
        all.iter()
            .find(|s| s.1 == p && s.3 == udp)
            .unwrap_or_else(|| panic!("{p} udp={udp}: {}", d.yaml))
            .2
    };
    assert_eq!(
        (svc(53, true), svc(53, false), svc(69, true), svc(161, true)),
        (Some(5353), Some(53), Some(6969), None)
    );

    // SCTP has no service protocol: drafted as TCP, and said so.
    let d = draft("services:\n  sig:\n    image: x\n    ports: [\"3868:3868/sctp\"]\n", "x", "compose.yaml").unwrap();
    assert!(
        has(&d, NoteKind::Changed, "services.sig.ports", "3868:3868/sctp drafted as TCP"),
        "{:?}",
        d.notes
    );
}

#[test]
fn a_key_set_twice_keeps_the_last_value_as_compose_does() {
    // secDevLabs camplake-api sets MONGO_PORT twice; Compose takes the last.
    let compose = "services:\n  api:\n    image: x\n    expose: [3000]\n    environment:\n      MONGO_PORT: 27017\n      MONGO_HOST: db\n      MONGO_PORT: \"27018\"\n      MONGO_PORT: 27019\n    expose: [8080]\n";
    let d = draft(compose, "x", "compose.yaml").unwrap();
    let spec = parse(&d.yaml).unwrap();
    assert_eq!(validate(&spec), Vec::<String>::new());
    assert_eq!(ports(&spec, "api").iter().map(|s| s.1).collect::<Vec<_>>(), [8080]);
    assert!(has(&d, NoteKind::InImage, "services.api.environment", "MONGO_PORT=27019, MONGO_HOST=db"));
    assert!(has(
        &d,
        NoteKind::Changed,
        "services.api.environment",
        "`MONGO_PORT` is set more than once; the last value (`27019`)"
    ));
    assert!(has(&d, NoteKind::Changed, "services.api", "`expose` is set more than once"));
    assert_eq!(
        d.notes.iter().filter(|n| n.text.contains("more than once")).count(),
        2,
        "one note per key: {:?}",
        d.notes
    );
}

// Vagrant: from what a Vagrantfile set when it ran (recorded by the CLI's vagrant_record.rb).

fn recorded(name: &str) -> serde_json::Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vagrant-classic").join(name);
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn a_vagrantfile_with_a_loop_becomes_a_valid_spec() {
    let d = isoloom_import::vagrant::draft(&recorded("recorded.json"), "classic", "Vagrantfile").unwrap();
    let spec = parse(&d.yaml).expect("the draft parses");
    assert_eq!(validate(&spec), Vec::<String>::new(), "{}", d.yaml);
    assert_eq!(spec.networks["lab"].cidr, "192.168.33.0/24");
    let web = &spec.machines["web"];
    assert_eq!(web.networks["lab"], syntax::Octets::One(10));
    assert_eq!(ports(&spec, "web").first().map(|p| (p.1, p.2)), Some((80, Some(8080))));
    assert_eq!(web.resources.unwrap().memory_mb, Some(1024));
    assert_eq!(spec.machines["db"].resources.unwrap().memory_mb, Some(2048));
    let boxes = &spec.overrides.vagrant.as_ref().unwrap().machines;
    assert_eq!(boxes["web"].box_name.as_deref(), Some("ubuntu/jammy64"), "an unknown box is kept");
    assert_eq!(steps(&spec, "web"), ["site.yml", "provision/web.sh"], "global provisioners run first");
    assert!(has(&d, NoteKind::InImage, "define web.vm.provision", "inline script"));
}

#[test]
fn a_udp_forwarded_port_is_imported_as_a_udp_service() {
    let mut rec = recorded("recorded.json");
    let calls = rec["children"]["vm"]["calls"][1]["block"]["children"]["vm"]["calls"].as_array_mut().unwrap();
    calls.insert(
        2,
        serde_json::json!({"name": "network", "args": ["forwarded_port", {"guest": 514, "host": 1514, "protocol": "udp"}], "block": null}),
    );
    let d = isoloom_import::vagrant::draft(&rec, "classic", "Vagrantfile").unwrap();
    let spec = parse(&d.yaml).unwrap();
    assert_eq!(validate(&spec), Vec::<String>::new(), "{}", d.yaml);
    assert_eq!(
        ports(&spec, "web").get(1),
        Some(&("syslog-udp".to_owned(), 514, Some(1514), true)),
        "{}",
        d.yaml
    );
}

#[test]
fn importing_an_isoloom_vagrantfile_gives_back_the_environment() {
    let d = isoloom_import::vagrant::draft(&recorded("segmented-recorded.json"), "segmented", "Vagrantfile").unwrap();
    let spec = parse(&d.yaml).unwrap();
    let original = isoloom_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/segmented")).unwrap();
    let cidrs = |s: &Spec| {
        let mut v: Vec<String> = s.networks.values().map(|n| n.cidr.clone()).collect();
        v.sort();
        v
    };
    assert_eq!(cidrs(&spec), cidrs(&original));
    for name in ["cache", "web", "user"] {
        assert!(spec.machines.contains_key(name), "{name} comes back");
    }
    assert!(!spec.machines.contains_key("isoloom-router"), "Isoloom's own router isn't a machine");
    assert_eq!(spec.machines["web"].os.map(|o| o.to_string()).as_deref(), Some("debian@12"));
    assert!(spec.overrides.vagrant.is_none(), "a built-in box needs no override");
}

// Terraform: read statically.

fn tf_files(dir: &str) -> Vec<(String, String)> {
    let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(dir);
    let mut v: Vec<(String, String)> = std::fs::read_dir(d)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "tf"))
        .map(|p| (p.file_name().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap()))
        .collect();
    v.sort();
    v
}

#[test]
fn terraform_for_each_machines_through_their_network_interfaces() {
    let d = isoloom_import::terraform::draft(&tf_files("terraform-aws"), "aws-lab", "main.tf").unwrap();
    let spec = parse(&d.yaml).expect("the draft parses");
    assert_eq!(spec.networks["lab-private"].cidr, "192.168.56.0/24", "the subnet, through a local");
    assert_eq!(
        spec.machines["dc01"].networks["lab-private"],
        syntax::Octets::One(10),
        "the address on its network interface"
    );
    assert_eq!(spec.machines["srv02"].networks["lab-private"], syntax::Octets::One(22));
    assert_eq!(spec.machines["srv02"].resources.unwrap().memory_mb, Some(8192), "t3.large");
    assert_eq!(spec.machines["dc01"].os.map(|o| o.to_string()).as_deref(), Some("windows-server@2019"));
    assert!(has(&d, NoteKind::Changed, "resource aws_s3_bucket", "not read by the import: left out"));
    assert!(has(&d, NoteKind::Changed, "resource aws_vpc", "not read by the import: left out"));
}

#[test]
fn not_terraform_is_an_error() {
    let bad = vec![("main.tf".to_string(), "resource \"x\" {".to_string())];
    assert!(isoloom_import::terraform::draft(&bad, "x", "main.tf").is_err());
    let none = vec![("main.tf".to_string(), "resource \"aws_s3_bucket\" \"b\" {}\n".to_string())];
    assert!(isoloom_import::terraform::draft(&none, "x", "main.tf").is_err(), "no VM: nothing to draft");
}
