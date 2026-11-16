//! `isoloom import compose`: a fixture written the way people write Compose files, and the forms
//! each key comes in.

use isoloom_import::{NoteKind, compose::draft};
use isoloom_spec::syntax::{EnvValue, Octets};

use crate::testing::{Outcome, fixture, has, image, notes, ports, said, spec, validate, validate_in, value_at, values_at};

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
    let header: Vec<&str> = d.yaml.lines().take_while(|l| l.starts_with('#')).collect();
    assert_eq!(
        header,
        [
            isoloom_spec::schema::MODELINE,
            "# Drafted by `isoloom import compose` from docker-compose.yml. Review it with the notes the",
            "# import printed: what Compose said that this file doesn't, and why.",
            "# Next: add `os` and `app.steps` to each machine for the VM targets, and `checks` that prove the behavior.",
        ]
    );
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
    assert!(has(
        &d,
        NoteKind::Changed,
        "services.seed",
        "left out: it runs once and exits (others wait for it to complete); one-shot work is a `setup` script of the machine it prepares"
    ));
    assert!(has(
        &d,
        NoteKind::Changed,
        "services.tester",
        "left out: only started with profile test; if it's a test, make it a check script (`checks:`)"
    ));
    Ok(())
}

#[test]
fn everything_else_is_a_note_with_its_reason() -> Outcome {
    let d = shop()?;
    assert!(has(
        &d,
        NoteKind::Changed,
        "networks.front.ipam",
        "172.30.0.0/16 isn't a private /24 to /29; re-addressed to 10.88.1.0/24"
    ));
    assert!(has(
        &d,
        NoteKind::InImage,
        "services.web.environment",
        "fixed values belong in the image (ENV): MODE=production"
    ));
    assert!(!d.notes.iter().any(|n| n.at.contains("volumes")), "named volumes carry over: {:?}", d.notes);
    assert!(has(
        &d,
        NoteKind::Changed,
        "services.web.ports",
        "published on the host's loopback (127.0.0.1) only; Compose publishes on every interface unless told otherwise"
    ));
    assert!(has(
        &d,
        NoteKind::Equivalent,
        "services.cache.healthcheck",
        "Isoloom probes every service port; a custom check belongs in `checks`"
    ));
    Ok(())
}

#[test]
fn renames_point_at_the_settings_that_still_use_the_old_name() -> Outcome {
    let compose = "services:\n  db.main:\n    image: postgres:16\n    expose: [5432]\n  app:\n    image: app\n    environment: { DB_HOST: db.main }\n";
    let d = draft(compose, "x", "compose.yaml")?;
    assert!(spec(&d)?.machines.contains_key("db-main"), "{}", d.yaml);
    assert!(has(
        &d,
        NoteKind::Changed,
        "services.db.main",
        "renamed `db-main` (machine names are DNS names); the environment of app still says `db.main`: update it"
    ));
    Ok(())
}

#[test]
fn services_and_networks_named_alike_get_names_of_their_own() -> Outcome {
    let compose = "services:\n  a_b:\n    image: x\n    networks: [n_1]\n  a-b:\n    image: y\n    networks: [n-1]\n  7:\n    image: z\n";
    let d = draft(compose, "x", "compose.yaml")?;
    let spec = spec(&d)?;
    assert_eq!(spec.machines.keys().map(String::as_str).collect::<Vec<_>>(), ["a-b", "a-b-2", "m-7"]);
    assert_eq!(spec.networks.keys().map(String::as_str).collect::<Vec<_>>(), ["n-1", "n-1-2", "net"]);
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
fn a_volume_shared_between_services_is_shared_between_machines() -> Outcome {
    let compose = "services:\n  a:\n    image: x\n    volumes: [\"shared:/data\", \"./conf:/etc/x:ro\", \"/cache\"]\n  b:\n    image: x\n    volumes: [{ type: volume, source: shared, target: /in }]\nvolumes: { shared: {} }\n";
    let d = draft(compose, "x", "compose.yaml")?;
    let spec = spec(&d)?;
    assert_eq!(validate(&d)?, Vec::<String>::new(), "{}", d.yaml);
    assert_eq!(
        spec.machines["a"].volumes.get("cache").map(isoloom_spec::model::Mount::path),
        Some("/cache"),
        "anonymous volume kept"
    );
    // One volume for both, held by the first that mounts it, each at its own path.
    assert_eq!(spec.volumes["shared"].owner.as_deref(), Some("a"), "{}", d.yaml);
    let at = |m: &str| spec.machines[m].volumes.get("shared").map(isoloom_spec::model::Mount::path);
    assert_eq!((at("a"), at("b")), (Some("/data"), Some("/in")));
    assert!(!d.notes.iter().any(|n| n.kind == NoteKind::NotInSpec), "{:?}", d.notes);
    assert!(has(
        &d,
        NoteKind::InImage,
        "services.a.volumes",
        "project files mounted in: copy them into the image, or run scripts as `setup` scripts: ./conf:/etc/x:ro"
    ));
    Ok(())
}

#[test]
fn a_services_network_aliases_are_its_machines_aliases() -> Outcome {
    let compose = "services:\n  api:\n    image: x\n    networks:\n      front: { aliases: [api, API.example.com, web, bad_name] }\n      back: {}\n  web:\n    image: y\n    networks: [front]\nnetworks: { front: {}, back: {} }\n";
    let d = draft(compose, "x", "compose.yaml")?;
    assert_eq!(validate(&d)?, Vec::<String>::new(), "{}", d.yaml);
    let spec = spec(&d)?;
    // Its own name is its machine's; another machine's name and a name no machine can take are
    // left out, and said.
    assert_eq!(spec.machines["api"].aliases, ["api.example.com"], "{}", d.yaml);
    let at = "services.api.networks.front.aliases";
    assert!(has(&d, NoteKind::Changed, at, "`web` left out: it names a machine"), "{:?}", d.notes);
    assert!(
        has(&d, NoteKind::Changed, at, "`bad_name` left out: not a name a machine can answer to"),
        "{:?}",
        d.notes
    );
    // On one of its two networks in Compose, on both in the spec: said.
    assert!(
        has(
            &d,
            NoteKind::Changed,
            at,
            "the machine's `aliases` answer on every network it is on, not only on `front`"
        ),
        "{:?}",
        d.notes
    );
    assert!(!d.notes.iter().any(|n| n.kind == NoteKind::NotInSpec), "{:?}", d.notes);
    Ok(())
}

#[test]
fn a_mount_path_the_spec_cant_hold_is_kept_for_validate_to_name() -> Outcome {
    // Compose mounts at a path with a space; a machine's paths can't hold one (the Docker short
    // syntax and the VMs' `mkdir -p` split on it), so the draft says it and validate names it.
    let d = draft("services:\n  a:\n    image: x\n    volumes: [\"logs:/var/my logs\"]\n", "x", "compose.yaml")?;
    assert_eq!(value_at(&d, "machines.a.volumes.logs"), "/var/my logs");
    let problems = validate(&d)?;
    assert_eq!(
        problems,
        ["machines.a.volumes.logs: `/var/my logs` isn't a safe absolute path inside the machine (no spaces or ':'), like /var/lib/postgresql/data"]
    );
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
    assert!(has(
        &d,
        NoteKind::Changed,
        "services.a.ports",
        "no fixed host port for 9000: add `publish:` to the service by hand"
    ));
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
        has(
            &d,
            NoteKind::Changed,
            "services.sig.ports",
            "3868:3868/sctp drafted as TCP: a port is TCP or UDP"
        ),
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
    assert!(has(
        &d,
        NoteKind::InImage,
        "services.api.environment",
        "fixed values belong in the image (ENV): MONGO_PORT=27019, MONGO_HOST=db"
    ));
    assert!(has(
        &d,
        NoteKind::Changed,
        "services.api.environment",
        "`MONGO_PORT` is set more than once; the last value (`27019`) is kept, as Compose does"
    ));
    assert!(has(
        &d,
        NoteKind::Changed,
        "services.api",
        "`expose` is set more than once; the last value (`8080`) is kept, as Compose does"
    ));
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
    assert_eq!(value_at(&d, "machines.web.app.build.context"), "web app", "{}", d.yaml);
    assert_eq!(value_at(&d, "machines.web.app.build.args.GREETING"), "a: b # c", "{}", d.yaml);
    assert!(spec.machines.contains_key("web"));
    Ok(())
}

#[test]
fn a_gpu_reservation_becomes_gpus_and_every_other_resource_key_is_noted() -> Outcome {
    let d = draft(
        "services:
  train:
    image: pytorch/pytorch
    deploy:
      replicas: 2
      resources:
        limits: { cpus: '2', memory: 4G, pids: 100 }
        reservations:
          cpus: '1'
          memory: 2G
          devices:
            - { driver: nvidia, count: 2, capabilities: [gpu] }
            - { capabilities: [tpu] }
",
        "ml",
        "compose.yml",
    )?;
    assert_eq!(validate(&d)?, Vec::<String>::new(), "{}", d.yaml);
    assert_eq!(
        (
            value_at(&d, "machines.train.hardware.gpus.count"),
            value_at(&d, "machines.train.hardware.gpus.driver")
        ),
        ("2".to_owned(), "nvidia".to_owned())
    );
    let at = "services.train.deploy";
    let unaddressed = ("networks.default.ipam", NoteKind::Changed, "no subnet given; addressed as 10.88.1.0/24");
    assert_eq!(
        notes(&d),
        said(&[
            unaddressed,
            (
                &format!("{at}.resources.limits"),
                NoteKind::NotInSpec,
                "a machine's limits are its CPUs and memory (`resources`); not: pids"
            ),
            (
                &format!("{at}.resources.reservations"),
                NoteKind::NotInSpec,
                "a machine has a size (`resources`), not a reservation; not kept: cpus, memory"
            ),
            (
                &format!("{at}.resources.reservations.devices[1]"),
                NoteKind::Changed,
                "not read by the import: left out; carry it over by hand if the environment needs it"
            ),
            (at, NoteKind::NotInSpec, "only resource limits are kept; not: replicas"),
        ])
    );
    let all = draft(
        "services:\n  t:\n    image: x\n    deploy: { resources: { reservations: { devices: [{ capabilities: [gpu] }] } } }\n",
        "ml",
        "compose.yml",
    )?;
    assert_eq!(value_at(&all, "machines.t.hardware.gpus"), "all");
    assert_eq!(notes(&all), said(&[unaddressed]), "a GPU reservation alone is all kept");
    Ok(())
}

#[test]
fn a_second_gpu_device_and_a_count_that_isnt_one_are_noted_not_dropped() -> Outcome {
    let yaml = "services:
  train:
    image: pytorch/pytorch
    deploy:
      resources:
        reservations:
          devices: [{ device_ids: ['0'], capabilities: [gpu] }, { driver: amd, count: 1, capabilities: [gpu] }]
  infer:
    image: pytorch/pytorch
    deploy: { resources: { reservations: { devices: [{ count: some, capabilities: [gpu] }] } } }
";
    let two = draft(yaml, "ml", "compose.yml")?;
    assert_eq!(values_at(&two, "machines.train.hardware.gpus.ids"), ["0"], "the first one kept");
    let devices = "services.train.deploy.resources.reservations.devices";
    let second = format!("{devices}[1]");
    assert!(
        has(
            &two,
            NoteKind::Changed,
            &second,
            "a machine has one GPU request (`hardware.gpus`), the first GPU device's: this one is left out"
        ),
        "{:?}",
        two.notes
    );
    assert_eq!(value_at(&two, "machines.infer.hardware.gpus"), "all");
    let count = "services.infer.deploy.resources.reservations.devices[0].count";
    assert!(
        has(&two, NoteKind::Changed, count, "`some` is neither a number nor `all`: read as `all`, every GPU"),
        "{:?}",
        two.notes
    );
    assert_eq!(validate(&two)?, Vec::<String>::new(), "{}", two.yaml);
    Ok(())
}

#[test]
fn a_host_folder_read_from_the_shell_is_noted_as_a_bind_not_drafted_as_a_volume() -> Outcome {
    let compose = "services:\n  db:\n    image: postgres:16\n    volumes:\n      - ${DATA_DIR:-./data}:/var/lib/postgresql/data\n";
    let d = draft(compose, "x", "compose.yaml")?;
    assert!(spec(&d)?.machines["db"].volumes.is_empty(), "{}", d.yaml);
    assert!(
        has(
            &d,
            NoteKind::InImage,
            "services.db.volumes",
            "project files mounted in: copy them into the image, or run scripts as `setup` scripts: ${DATA_DIR:-./data}:/var/lib/postgresql/data"
        ),
        "{:?}",
        d.notes
    );
    assert_eq!(validate(&d)?, Vec::<String>::new(), "{}", d.yaml);
    Ok(())
}

#[test]
fn a_variable_set_from_another_one_is_noted_not_renamed_silently() -> Outcome {
    let compose = "services:\n  db:\n    image: postgres:16\n    environment:\n      POSTGRES_PASSWORD: ${DB_PASSWORD:-secret}\n      DB_USER: ${DB_USER}\n";
    let d = draft(compose, "x", "compose.yaml")?;
    let db = &spec(&d)?.machines["db"];
    assert!(
        db.environment.contains_key("DB_PASSWORD") && db.environment.contains_key("DB_USER"),
        "{}",
        d.yaml
    );
    assert!(
        has(
            &d,
            NoteKind::Changed,
            "services.db.environment",
            "the machine gets each shell variable these read as an input of that name (an input keeps its name), without a default: set the variable from it in the image or a `setup` script: POSTGRES_PASSWORD=${DB_PASSWORD:-secret}"
        ),
        "{:?}",
        d.notes
    );
    assert!(
        !d.notes.iter().any(|n| n.text.contains("DB_USER=")),
        "a variable read under its own name is the input: {:?}",
        d.notes
    );
    Ok(())
}

#[test]
fn each_service_key_without_a_place_in_the_spec_is_a_note_of_its_kind() -> Outcome {
    let compose = std::fs::read_to_string(fixture("compose-keys").join("compose.yaml"))?;
    let d = draft(&compose, "x", "compose.yaml")?;
    let left_out = "not read by the import: left out; carry it over by hand if the environment needs it";
    let expected = said(&[
        ("secrets", NoteKind::Changed, left_out),
        (
            "services.web.network_mode",
            NoteKind::ByDesign,
            "a machine has its own addresses; attached to the default network instead",
        ),
        ("networks.default.ipam", NoteKind::Changed, "no subnet given; addressed as 10.88.1.0/24"),
        (
            "services.web.image",
            NoteKind::Changed,
            "`nginx:${TAG}` reads shell variables; kept as written, fix it to one image",
        ),
        (
            "services.web.command",
            NoteKind::InImage,
            "set it in the image (CMD / ENTRYPOINT): nginx -g daemon off;",
        ),
        ("services.web.hostname", NoteKind::Equivalent, "the hostname is the machine's name, `web`"),
        (
            "services.web.env_file",
            NoteKind::Equivalent,
            "values given at launch are `inputs`: list the ones the machine needs",
        ),
        ("services.web.x-team", NoteKind::Tooling, "extension field, ignored"),
        ("services.web.stdin_open", NoteKind::Changed, left_out),
        (
            "services.web.expose",
            NoteKind::Changed,
            "couldn't read port `1-100`; add it to the machine's `ports` by hand",
        ),
        ("services.app.build", NoteKind::Changed, "context `../app` is outside the project; copy it in"),
        (
            "services.app.build",
            NoteKind::InImage,
            "only the context is kept, not: network (build settings belong in the Dockerfile)",
        ),
        (
            "services.app.build.args",
            NoteKind::InImage,
            "read from the shell, left out: B (give fixed values)",
        ),
        ("services.app.image", NoteKind::Equivalent, "names the built image; Isoloom names it"),
    ]);
    assert_eq!(notes(&d), expected);
    Ok(())
}

#[test]
fn a_services_size_dependency_and_fixed_build_settings_carry_over() -> Outcome {
    let compose = std::fs::read_to_string(fixture("compose-keys").join("compose.yaml"))?;
    let d = draft(&compose, "x", "compose.yaml")?;
    assert_eq!(
        (value_at(&d, "machines.web.resources.cpus"), value_at(&d, "machines.web.resources.memory_mb")),
        ("2".to_owned(), "512".to_owned())
    );
    assert_eq!(values_at(&d, "machines.web.depends_on"), ["db"]);
    let build = "machines.app.app.build";
    assert_eq!(
        [
            value_at(&d, &format!("{build}.context")),
            value_at(&d, &format!("{build}.dockerfile")),
            value_at(&d, &format!("{build}.target")),
            value_at(&d, &format!("{build}.args.A"))
        ],
        ["../app", "../app/app.Dockerfile", "prod", "1"]
    );
    Ok(())
}

#[test]
fn an_address_the_network_cant_give_is_moved_and_a_dependency_with_nothing_to_wait_for_is_dropped() -> Outcome {
    let compose = "services:\n  a:\n    image: x\n    depends_on: [dns]\n    networks: { n: { ipv4_address: 10.5.0.1 } }\n  dns:\n    image: y\n    expose: [\"53/udp\"]\n    networks: [n]\nnetworks:\n  n: { ipam: { config: [{ subnet: 10.5.0.0/24 }] } }\n";
    let d = draft(compose, "x", "compose.yaml")?;
    assert_eq!(
        notes(&d),
        said(&[
            ("services.a.networks.n.ipv4_address", NoteKind::Changed, "10.5.0.1 isn't usable here; given .10"),
            (
                "machines.a.depends_on",
                NoteKind::Changed,
                "`dns` has no known TCP port (UDP has no handshake), so there's nothing to wait for; add its `ports` and the dependency back"
            ),
        ])
    );
    let spec = spec(&d)?;
    assert_eq!(spec.machines["a"].networks["n"], Octets::One(10));
    assert_eq!(spec.machines["a"].depends_on, Vec::<String>::new());
    Ok(())
}
