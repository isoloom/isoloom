//! How the common mistakes of a spec (a 1.x block, ports as a list, an unknown OS version, ...)
//! read: each one's problems rendered the way an author sees them
//! (`error[<code>] <path>  (isoloom.yml:<line>:<column>)`, the reason, the fix), and as JSON for
//! apps and editors. The rendering is the user-facing format, so a change to a code, a path, a
//! place, a reason or a fix shows here.

use isoloom_spec::syntax::{self, Problem};

/// A spec with one network (`lab`) and the machines (and anything else) in `rest`.
fn spec(rest: &str) -> String {
    format!("version: 1\nname: mistakes\nnetworks:\n  lab: {{ cidr: 10.1.0.0/24 }}\n{rest}")
}

/// What an author reads about `yaml`: the parser's problems, else the checks'.
fn problems(yaml: &str) -> Vec<Problem> {
    match syntax::parse(yaml) {
        Err(refused) => refused,
        Ok(parsed) => syntax::check(&parsed),
    }
}

/// Every problem of `yaml`, one after the other, as `isoloom validate` prints them.
fn rendered(yaml: &str) -> String {
    let found = problems(yaml);
    assert!(!found.is_empty(), "no problem in:\n{yaml}");
    found.iter().map(|p| p.render("isoloom.yml")).collect::<Vec<_>>().join("\n")
}

#[test]
fn an_old_docker_block() {
    insta::assert_snapshot!(rendered(&spec(
        "machines:\n  web:\n    networks: { lab: 10 }\n    docker: { build: build/web, init: [data/secret.sh] }\n"
    )));
}

#[test]
fn an_old_vm_block() {
    insta::assert_snapshot!(rendered(&spec(
        "machines:\n  dc01:\n    networks: { lab: 10 }\n    vm: { os: windows-server@2019, provision: [dc.ps1], image: { vagrant: StefanScherer/windows_2019 } }\n"
    )));
}

#[test]
fn old_services() {
    insta::assert_snapshot!(rendered(&spec(
        "machines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    services: [{ port: 80, http: true, publish: 8080 }]\n"
    )));
}

#[test]
fn ports_as_a_list() {
    insta::assert_snapshot!(rendered(&spec(
        "machines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    ports: [{ port: 80 }]\n"
    )));
}

#[test]
fn a_machines_inputs() {
    insta::assert_snapshot!(rendered(&spec(
        "inputs: [DEMO_SECRET]\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    inputs: [DEMO_SECRET]\n"
    )));
}

#[test]
fn a_sensitive_input_in_the_environment() {
    insta::assert_snapshot!(rendered(&spec(
        "inputs: [DEMO_SECRET]\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    environment: { DEMO_SECRET: { input: DEMO_SECRET } }\n"
    )));
}

#[test]
fn a_secret_in_the_environment() {
    insta::assert_snapshot!(rendered(&spec(
        "secrets:\n  db-password: { env: DB_PASSWORD }\nmachines:\n  db:\n    os: debian@12\n    networks: { lab: 10 }\n    environment: { PASSWORD: { secret: db-password } }\n"
    )));
}

#[test]
fn a_service_and_an_app_together() {
    insta::assert_snapshot!(rendered(&spec(
        "machines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    service: nginx\n    app: { image: nginx }\n"
    )));
}

#[test]
fn an_unknown_os_version() {
    insta::assert_snapshot!(rendered(&spec("machines:\n  web:\n    os: debian@10\n    networks: { lab: 10 }\n")));
}

#[test]
fn a_catalogue_service_with_its_own_process_and_port_number() {
    insta::assert_snapshot!(rendered(&spec(
        "machines:\n  db:\n    os: debian@12\n    networks: { lab: 10 }\n    service: postgres@16\n    command: postgres\n    ports: { pg: { port: 5433 } }\n"
    )));
}

#[test]
fn steps_without_an_os() {
    insta::assert_snapshot!(rendered(&spec(
        "machines:\n  web:\n    networks: { lab: 10 }\n    app: { steps: [{ run: provision/web.sh }] }\n"
    )));
}

#[test]
fn an_index_in_the_message() {
    insta::assert_snapshot!(rendered(&spec(
        "machines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    ports: { web: { port: 80, protocol: http, publish: 8080 } }\nmessage: \"Open {{ machines.web.services.0.publish }}\"\n"
    )));
}

#[test]
fn a_file_with_two_sources() {
    insta::assert_snapshot!(rendered(&spec(
        "inputs: [DEMO_SECRET]\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    files: [{ path: /srv/demo/secret, source: secret.txt, input: DEMO_SECRET }]\n"
    )));
}

#[test]
fn dns_naming_the_wrong_machine() {
    insta::assert_snapshot!(rendered(&spec(
        "machines:\n  dc01:\n    os: windows-server@2022\n    networks: { lab: 10 }\n  station:\n    os: debian@12\n    networks: { lab: 20 }\n    dns: { servers: [dc1] }\n"
    )));
}

#[test]
fn an_override_nothing_applies() {
    insta::assert_snapshot!(rendered(&spec(
        "machines:\n  web:\n    networks: { lab: 10 }\n    app: { build: build/web }\noverrides:\n  vagrant:\n    machines:\n      web: { box: generic/debian12 }\n"
    )));
}

#[test]
fn a_container_only_machine_on_a_vm_target() {
    let yaml = spec("machines:\n  web:\n    networks: { lab: 10 }\n    app: { build: build/web }\n");
    let parsed = syntax::parse(&yaml).unwrap_or_else(|p| panic!("{p:?}"));
    let web = &parsed.machines["web"];
    let refusal = parsed.spans.place(syntax::vm_form(web).unwrap_err().refusal("web"));
    insta::assert_snapshot!(format!("vagrant: {}", refusal.render("isoloom.yml")));
}

#[test]
fn problems_read_as_json_too() {
    let found = problems(&spec(
        "inputs: [DEMO_SECRET]\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    environment: { DEMO_SECRET: { input: DEMO_SECRET } }\n    dns: { servers: [wbe] }\n",
    ));
    insta::assert_snapshot!(serde_json::to_string_pretty(&found).unwrap_or_else(|e| panic!("{e}")));
}

#[test]
fn clones_past_the_last_address() {
    let shown = rendered(&spec("machines:\n  m: { app: { image: nginx }, networks: { lab: 250 }, count: 10 }\n"));
    assert!(shown.starts_with("error[invalid-value] machines.m.networks.lab  (isoloom.yml:6:"), "{shown}");
    assert!(
        shown.ends_with("\n  10 machines from .250 run past .255 (the last would be .259): start lower or count fewer"),
        "one problem, saying why: {shown}"
    );
}

#[test]
fn a_value_of_the_wrong_kind_says_what_the_field_takes() {
    let reason = |machine: &str| {
        let found = problems(&spec(&format!("machines:\n  m: {{ app: {{ image: nginx }}, {machine} }}\n")));
        let [p] = &found[..] else { panic!("one problem: {found:?}") };
        (p.path.to_string(), p.reason.clone())
    };
    assert_eq!(
        reason("networks: { lab: 300 }"),
        (
            "machines.m.networks.lab".to_owned(),
            "`300` isn't the last number of an address: 0 to 255".to_owned()
        )
    );
    assert_eq!(reason("networks: { lab: [10, 256] }").1, "`256` isn't the last number of an address: 0 to 255");
    let typed = reason("networks: { lab: \"10\" }").1;
    assert!(
        typed.ends_with("expected the last number of its address (0 to 255, like 10), or a list of them ([10, 11])"),
        "{typed}"
    );
    let limit = reason("networks: { lab: 10 }, ulimits: { nofile: abc }").1;
    assert!(limit.ends_with("expected a number for both limits (65536), or `{ soft, hard }`"), "{limit}");
    let test = reason("networks: { lab: 10 }, healthcheck: { test: 5 }").1;
    assert!(
        test.ends_with("expected a text the shell runs, or a list of arguments run as they are"),
        "{test}"
    );
}

#[test]
fn a_secrets_source_and_a_device_of_the_wrong_kind_say_what_they_take() {
    let reason = |rest: &str| {
        let found = problems(&spec(rest));
        let [p] = &found[..] else { panic!("one problem: {found:?}") };
        (p.path.to_string(), p.reason.clone())
    };
    let machine = "machines:\n  m: { os: debian@12, networks: { lab: 10 } }\n";
    let store = "one store: `{ env: NAME }`, `{ file: <path> }`, `{ vault: <path>#<field> }` or `{ sops: <file>#<key> }`";
    for source in ["5", "[DB]", "{}", "{ env: A, file: b }"] {
        let (path, why) = reason(&format!("secrets: {{ db: {source} }}\n{machine}"));
        assert_eq!(path, "secrets.db", "{source}");
        assert!(why.contains(store) && !why.contains("untagged"), "{source}: {why}");
    }
    let (_, typo) = reason(&format!("secrets: {{ db: {{ evn: DB }} }}\n{machine}"));
    assert_eq!(typo, "`evn` isn't a field here");
    let device = |value: &str| {
        reason(&format!(
            "machines:\n  m: {{ app: {{ image: nginx }}, networks: {{ lab: 10 }}, hardware: {{ devices: [{value}] }} }}\n"
        ))
    };
    let (path, why) = device("5");
    assert_eq!(path, "machines.m.hardware.devices[0]");
    assert!(
        why.ends_with("expected a device path (`/dev/fuse`, or `host:guest[:permissions]`), or `{ host, guest, permissions }`"),
        "{why}"
    );
    let (_, misspelt) = device("{ hots: /dev/fuse }");
    assert_eq!(misspelt, "`hots` isn't a field here");
}

#[test]
fn a_free_host_port_is_asked_for_by_any_number_of_machines_and_a_number_once() {
    let web = |rest: &str| problems(&spec(&format!("machines:\n  web: {{ os: debian@12, networks: {{ lab: 10 }}, {rest} }}\n")));
    assert_eq!(web("count: 3, ports: { web: { port: 80, publish: true } }"), []);
    let both = problems(&spec(
        "machines:\n  a: { os: debian@12, networks: { lab: 10 }, ports: { web: { port: 80, publish: true } } }\n  b: { os: debian@12, networks: { lab: 11 }, ports: { web: { port: 80, publish: true } } }\n",
    ));
    assert_eq!(both, []);
    for publish in ["8080", "{ host: 8080 }", "{ host: 8080, fixed: true }"] {
        let found = web(&format!("count: 3, ports: {{ web: {{ port: 80, publish: {publish} }} }}"));
        let [p] = &found[..] else { panic!("{publish}: one problem, on the machine as written: {found:?}") };
        assert_eq!(p.path.to_string(), "machines.web.ports.web.publish", "{publish}");
        assert!(
            p.reason.contains("its 3 clones (`count`) can't all have host port 8080: `publish: true`"),
            "{publish}: {}",
            p.reason
        );
    }
}
