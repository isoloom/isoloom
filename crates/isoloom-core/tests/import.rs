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
    assert!(d.yaml.starts_with("# Drafted by `isoloom import compose` from docker-compose.yml."));
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
    assert_eq!(web.inputs, ["API_TOKEN"]);
    assert_eq!(spec.inputs, ["API_TOKEN"]);
    let r = web.resources.unwrap();
    assert_eq!((r.cpus, r.memory_mb), (Some(1), Some(256)));
    assert_eq!(web.depends_on, ["cache"]);
    assert_eq!(web.docker.as_ref().unwrap().image.as_deref(), Some("nginx:1.27-alpine"));
    assert_eq!(spec.machines["cache"].services[0].port, 6379, "expose becomes a service");
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
    assert!(has(&d, NoteKind::NotYet, "services.cache.volumes", "cache-data:/data"));
    assert!(has(&d, NoteKind::NotYet, "volumes", "persistent or shared data"));
    assert!(has(&d, NoteKind::NotYet, "services.web.ports", "isn't in the format yet"));
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
