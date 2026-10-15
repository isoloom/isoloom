//! How the common mistakes of the spec (spec.md §9.2) read: each one's problems rendered the way
//! an author sees them (`error[<code>] <path>  (isoloom.yml:<line>:<column>)`, the reason, the
//! fix), and as JSON for the launcher and editors. The rendering is the user-facing format, so a
//! change to a code, a path, a place, a reason or a fix shows here.

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
        "machines:\n  web:\n    networks: { lab: 10 }\n    docker: { build: build/web, init: [ctf/flag.sh] }\n"
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
        "inputs: [CTF_FLAG_MAIN]\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    inputs: [CTF_FLAG_MAIN]\n"
    )));
}

#[test]
fn a_sensitive_input_in_the_environment() {
    insta::assert_snapshot!(rendered(&spec(
        "inputs: [FLAG]\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    environment: { FLAG: { input: FLAG } }\n"
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
fn a_catalogue_service_before_the_catalogue() {
    insta::assert_snapshot!(rendered(&spec(
        "machines:\n  db:\n    os: debian@12\n    networks: { lab: 10 }\n    service: postgres@16\n    with: { database: books }\n"
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
        "inputs: [FLAG]\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    files: [{ path: /ctf/flag, source: flag.txt, input: FLAG }]\n"
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
        "inputs: [FLAG]\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    environment: { FLAG: { input: FLAG } }\n    dns: { servers: [wbe] }\n",
    ));
    insta::assert_snapshot!(serde_json::to_string_pretty(&found).unwrap_or_else(|e| panic!("{e}")));
}
