//! `isoloom import compose`: a fixture written the way people write Compose files, and the forms
//! each key comes in.

use isoloom_import::{NoteKind, compose::draft};
use isoloom_spec::syntax::{EnvValue, Octets};

use crate::testing::{Outcome, fixture, has, image, ports, spec, validate, validate_in};

/// The shop fixture's draft.
fn shop() -> Outcome<isoloom_import::Draft> {
    let text = std::fs::read_to_string(fixture("compose-shop").join("docker-compose.yml"))?;
    Ok(draft(&text, "fallback", "docker-compose.yml")?)
}

#[test]
fn the_draft_passes_validate() -> Outcome {
    let d = shop()?;
    assert_eq!(validate_in(&d, &fixture("compose-shop"))?, Vec::<String>::new(), "{}", d.yaml);
    assert_eq!(spec(&d)?.name, "shop-demo");
    assert!(d.yaml.starts_with(isoloom_spec::schema::MODELINE));
    assert!(d.yaml.contains("# Drafted by `isoloom import compose` from docker-compose.yml."));
    assert!(d.yaml.contains("Review it with the notes the\n# import printed:"), "{}", d.yaml);
    Ok(())
}

#[test]
fn what_the_format_expresses_is_carried_over() -> Outcome {
    let spec = spec(&shop()?)?;
    // Networks: a usable subnet is kept, an unusable one re-addressed; internal = offline.
    assert_eq!(spec.networks["back"].cidr, "10.31.2.0/24");
    assert!(!spec.networks["back"].internet);
    assert_eq!(spec.networks["front"].cidr, "10.88.1.0/24");
    let web = &spec.machines["web"];
    // The fixed address's last octet survives re-addressing.
    assert_eq!(web.networks["front"], Octets::One(10));
    assert_eq!(ports(&spec, "web"), [("http".to_owned(), 80, Some(8080), false)], "the host port carries over");
    assert!(
        matches!(&web.environment["API_TOKEN"], EnvValue::Input(i) if i.as_str() == "API_TOKEN"),
        "an interpolated variable becomes an input: {:?}",
        web.environment
    );
    assert!(spec.inputs.0.keys().any(|k| k.as_str() == "API_TOKEN"));
    let r = web.resources.ok_or("web has no resources")?;
    assert_eq!((r.cpus, r.memory_mb), (Some(1), Some(256)));
    assert_eq!(web.depends_on, ["cache"]);
    assert_eq!(image(&spec, "web").as_deref(), Some("nginx:1.27-alpine"));
    assert_eq!(ports(&spec, "cache"), [("redis".to_owned(), 6379, None, false)], "expose becomes a port");
    assert_eq!(spec.machines["cache"].volumes["cache-data"].path(), "/data", "a named volume carries over");
    Ok(())
}

#[test]
fn jobs_and_profile_services_are_left_out_with_a_note() -> Outcome {
    let d = shop()?;
    let spec = spec(&d)?;
    assert!(!spec.machines.contains_key("seed"), "a one-shot job would restart forever as a machine");
    assert!(!spec.machines.contains_key("tester"), "Compose doesn't start profile-only services");
    assert!(has(&d, NoteKind::Changed, "services.seed", "runs once and exits"));
    assert!(has(&d, NoteKind::Changed, "services.tester", "only started with profile test"));
    Ok(())
}

#[test]
fn everything_else_is_a_note_with_its_reason() -> Outcome {
    let d = shop()?;
    assert!(has(&d, NoteKind::Changed, "networks.front.ipam", "re-addressed to 10.88.1.0/24"));
    assert!(has(&d, NoteKind::InImage, "services.web.environment", "MODE=production"));
    assert!(!d.notes.iter().any(|n| n.at.contains("volumes")), "named volumes carry over: {:?}", d.notes);
    assert!(has(&d, NoteKind::Changed, "services.web.ports", "loopback"));
    assert!(has(&d, NoteKind::Equivalent, "services.cache.healthcheck", "probes every service port"));
    Ok(())
}

#[test]
fn renames_point_at_the_settings_that_still_use_the_old_name() -> Outcome {
    let compose = "services:\n  db.main:\n    image: postgres:16\n    expose: [5432]\n  app:\n    image: app\n    environment: { DB_HOST: db.main }\n";
    let d = draft(compose, "x", "compose.yaml")?;
    assert!(d.yaml.contains("  db-main:"));
    assert!(has(&d, NoteKind::Changed, "services.db.main", "the environment of app still says `db.main`"));
    Ok(())
}

#[test]
fn services_and_networks_named_alike_get_names_of_their_own() -> Outcome {
    let compose = "services:\n  a_b:\n    image: x\n    networks: [n_1]\n  a-b:\n    image: y\n    networks: [n-1]\n  7:\n    image: z\n";
    let d = draft(compose, "x", "compose.yaml")?;
    let spec = spec(&d)?;
    assert_eq!(spec.machines.keys().map(String::as_str).collect::<Vec<_>>(), ["a-b", "a-b-2", "m-7"]);
    assert_eq!(spec.networks.keys().map(String::as_str).collect::<Vec<_>>(), ["n-1", "n-1-2", "lab"]);
    assert_eq!(validate(&d)?, Vec::<String>::new(), "{}", d.yaml);
    Ok(())
}

#[test]
fn not_a_compose_file_is_an_error() {
    assert!(matches!(draft("just text", "x", "f"), Err(isoloom_import::Error::ComposeNotMapping)));
    assert!(matches!(draft("a: [", "x", "f"), Err(isoloom_import::Error::ComposeYaml(_))));
    assert!(matches!(draft("services: {}\n", "x", "f"), Err(isoloom_import::Error::NoServices)));
}

#[test]
fn volumes_shared_between_services_are_noted() -> Outcome {
    let compose = "services:\n  a:\n    image: x\n    volumes: [\"shared:/data\", \"./conf:/etc/x:ro\", \"/cache\"]\n  b:\n    image: x\n    volumes: [{ type: volume, source: shared, target: /in }]\nvolumes: { shared: {} }\n";
    let d = draft(compose, "x", "compose.yaml")?;
    let spec = spec(&d)?;
    assert_eq!(validate(&d)?, Vec::<String>::new(), "{}", d.yaml);
    assert_eq!(
        spec.machines["a"].volumes.get("cache").map(isoloom_spec::model::Mount::path),
        Some("/cache"),
        "anonymous volume kept"
    );
    assert!(!spec.machines["a"].volumes.contains_key("shared"));
    assert!(has(&d, NoteKind::NotYet, "services.a.volumes", "shared:/data"));
    assert!(has(&d, NoteKind::InImage, "services.a.volumes", "./conf:/etc/x:ro"));
    Ok(())
}

#[test]
fn a_mount_path_the_spec_cant_hold_is_kept_for_validate_to_name() -> Outcome {
    // Compose mounts at a path with a space; a machine's paths can't hold one (the Docker short
    // syntax and the VMs' `mkdir -p` split on it), so the draft says it and validate names it.
    let d = draft("services:\n  a:\n    image: x\n    volumes: [\"logs:/var/my logs\"]\n", "x", "compose.yaml")?;
    assert!(d.yaml.contains("volumes: { logs: \"/var/my logs\" }"), "{}", d.yaml);
    let problems = validate(&d)?;
    assert!(problems.iter().any(|p| p.starts_with("machines.a.volumes.logs:")), "{problems:?}");
    Ok(())
}

#[test]
fn published_ports_read_every_compose_form() -> Outcome {
    let compose =
        "services:\n  a:\n    image: x\n    ports: [\"127.0.0.1:8443:443\", \"${WEB_PORT:-8080}:80\", \"9000\", { target: 5432, published: 15432 }]\n";
    let d = draft(compose, "x", "compose.yaml")?;
    let spec = spec(&d)?;
    let published = |p: u16| ports(&spec, "a").into_iter().find(|s| s.1 == p).and_then(|s| s.2);
    assert_eq!(
        (published(443), published(80), published(9000), published(5432)),
        (Some(8443), Some(8080), None, Some(15432))
    );
    assert!(has(&d, NoteKind::Changed, "services.a.ports", "no fixed host port for 9000"));
    Ok(())
}

#[test]
fn udp_ports_are_imported_as_udp_ports() -> Outcome {
    let compose = "services:\n  dns:\n    image: x\n    ports: [\"5353:53/udp\", \"53:53\", { target: 69, published: 6969, protocol: udp }]\n    expose: [\"161/udp\"]\n  app:\n    image: x\n    depends_on: [dns]\n";
    let d = draft(compose, "x", "compose.yaml")?;
    let spec = spec(&d)?;
    assert_eq!(validate(&d)?, Vec::<String>::new(), "{}", d.yaml);
    let all = ports(&spec, "dns");
    let published = |p: u16, udp: bool| all.iter().find(|s| s.1 == p && s.3 == udp).map(|s| s.2);
    assert_eq!(
        (published(53, true), published(53, false), published(69, true), published(161, true)),
        (Some(Some(5353)), Some(Some(53)), Some(Some(6969)), Some(None))
    );
    Ok(())
}

#[test]
fn a_protocol_other_than_tcp_and_udp_is_drafted_as_tcp_and_said_so() -> Outcome {
    let d = draft("services:\n  sig:\n    image: x\n    ports: [\"3868:3868/sctp\"]\n", "x", "compose.yaml")?;
    assert!(
        has(&d, NoteKind::Changed, "services.sig.ports", "3868:3868/sctp drafted as TCP"),
        "{:?}",
        d.notes
    );
    Ok(())
}

#[test]
fn a_key_set_twice_keeps_the_last_value_as_compose_does() -> Outcome {
    // secDevLabs camplake-api sets MONGO_PORT twice; Compose takes the last.
    let compose = "services:\n  api:\n    image: x\n    expose: [3000]\n    environment:\n      MONGO_PORT: 27017\n      MONGO_HOST: db\n      MONGO_PORT: \"27018\"\n      MONGO_PORT: 27019\n    expose: [8080]\n";
    let d = draft(compose, "x", "compose.yaml")?;
    let spec = spec(&d)?;
    assert_eq!(validate(&d)?, Vec::<String>::new());
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
    Ok(())
}

#[test]
fn a_build_with_text_yaml_would_misread_is_quoted() -> Outcome {
    let compose = "services:\n  web:\n    build:\n      context: \"web app\"\n      dockerfile: \"Dockerfile.prod\"\n      args: { GREETING: \"a: b # c\" }\n";
    let d = draft(compose, "x", "compose.yaml")?;
    let spec = spec(&d)?;
    assert!(d.yaml.contains("context: \"web app\""), "{}", d.yaml);
    assert!(d.yaml.contains("GREETING: \"a: b # c\""), "{}", d.yaml);
    assert!(spec.machines.contains_key("web"));
    Ok(())
}
