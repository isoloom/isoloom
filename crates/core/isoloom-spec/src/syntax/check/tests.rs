use super::*;
use crate::syntax::parse;

/// The problems of a spec whose machines are `machines` (YAML), as `(code, path)`.
fn problems(rest: &str) -> Vec<(Code, String)> {
    let yaml = format!("version: 1\nname: t\nnetworks:\n  lab: {{ cidr: 10.1.0.0/24 }}\n  lab2: {{ cidr: 10.2.0.0/24 }}\n{rest}");
    let spec = parse(&yaml).unwrap_or_else(|p| panic!("{p:?}"));
    check(&spec).into_iter().map(|p| (p.code, p.path.to_string())).collect()
}

fn has(found: &[(Code, String)], code: Code, path: &str) -> bool {
    found.iter().any(|(c, p)| *c == code && p == path)
}

#[test]
fn a_well_formed_spec_has_no_problem() {
    let found = problems(
        "inputs: { FLAG: {}, API_URL: { sensitive: false } }\nmachines:\n  web:\n    networks: { lab: 10 }\n    app: { build: build/web }\n    ports: { site: { port: 8000, protocol: http, publish: 8080 } }\n    files: [{ path: /ctf/flag, input: FLAG, uid: 1000 }]\n    environment: { API_URL: { input: API_URL }, MODE: prod }\n    healthcheck: { http: { port: site } }\nmessage: \"{{ machines.web.ports.site.url }} {{ machines.web.addresses.lab }}\"\n",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_machine_says_what_it_is() {
    let found = problems(
        "machines:\n  a: { networks: { lab: 10 } }\n  b: { networks: { lab: 11 }, app: { steps: [{ run: a.sh }] } }\n  c: { networks: { lab: 12 }, access: true }\n",
    );
    assert!(
        has(&found, Code::MissingField, "machines.a.os") && has(&found, Code::MissingField, "machines.b.os"),
        "{found:?}"
    );
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn a_catalogue_service_waits_for_phase_4() {
    let found = problems("machines:\n  db: { os: debian@12, networks: { lab: 10 }, service: postgres@16 }\n");
    assert!(has(&found, Code::UnknownReference, "machines.db.service"), "{found:?}");
}

#[test]
fn sensitive_values_stay_out_of_the_environment() {
    let found = problems(
        "inputs: [FLAG]\nsecrets: { db: { env: DB } }\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    environment: { FLAG: { input: FLAG }, DB: { secret: db }, X: { input: NOPE } }\n",
    );
    assert!(has(&found, Code::SensitiveValue, "machines.web.environment.FLAG"), "{found:?}");
    assert!(has(&found, Code::SensitiveValue, "machines.web.environment.DB"), "{found:?}");
    assert!(has(&found, Code::UnknownReference, "machines.web.environment.X"), "{found:?}");
}

#[test]
fn an_input_keeps_its_name_in_the_environment() {
    let found = problems(
        "inputs: { API_URL: { sensitive: false } }\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    environment: { URL: { input: API_URL } }\n",
    );
    assert_eq!(found, [(Code::Refused, "machines.web.environment.URL".to_owned())]);
}

#[test]
fn files_name_declared_values_once_each() {
    let found = problems(
        "machines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    files: [{ path: /a, input: FLAG }, { path: /a, secret: db }, { path: rel, content: x }]\n",
    );
    for (code, path) in [
        (Code::UnknownReference, "machines.web.files[0].input"),
        (Code::UnknownReference, "machines.web.files[1].secret"),
        (Code::Conflict, "machines.web.files[1].path"),
        (Code::InvalidValue, "machines.web.files[2].path"),
    ] {
        assert!(has(&found, code, path), "{path}: {found:?}");
    }
    let found = problems(
        "inputs: [FLAG]\nmachines:\n  dc: { os: windows-server@2019, networks: { lab: 10 }, files: [{ path: 'C:\\flag.txt', input: FLAG, mode: \"0444\" }] }\n",
    );
    assert!(has(&found, Code::InvalidValue, "machines.dc.files[0].mode") && found.len() == 1, "{found:?}");
}

#[test]
fn scripts_have_the_kind_their_os_runs() {
    let found = problems(
        "machines:\n  web: { os: debian@12, networks: { lab: 10 }, setup: [a.ps1], app: { steps: [{ run: a.yml }, { playbook: p.sh }] } }\n  dc: { os: windows-server@2019, networks: { lab: 11 }, setup: [a.ps1], app: { steps: [{ run: a.sh }, reboot] } }\n",
    );
    for path in [
        "machines.web.setup[0]",
        "machines.web.app.steps[0].run",
        "machines.web.app.steps[1].playbook",
        "machines.dc.app.steps[0].run",
        "machines.dc.app.steps[1]",
    ] {
        assert!(has(&found, Code::InvalidValue, path), "{path}: {found:?}");
    }
    assert_eq!(found.len(), 5, "{found:?}");
}

#[test]
fn ports_and_healthchecks_are_named() {
    let found =
        problems("machines:\n  web: { os: debian@12, networks: { lab: 10 }, ports: { site: { protocol: http } }, healthcheck: { http: { port: api } } }\n");
    assert!(has(&found, Code::MissingField, "machines.web.ports.site.port"), "{found:?}");
    assert!(has(&found, Code::UnknownReference, "machines.web.healthcheck.http.port"), "{found:?}");
}

#[test]
fn the_message_uses_named_paths() {
    let found = problems(
        "machines:\n  web: { os: debian@12, networks: { lab: 10 }, ports: { site: { port: 80, protocol: http }, ssh: 22 } }\nmessage: \"{{ machines.web.ports.0.publish }} {{ machines.web.ports.ssh.url }} {{ machines.web.addresses.lab }} {{ cloud.outputs.x }}\"\n",
    );
    assert_eq!(
        found.iter().filter(|(c, p)| *c == Code::UnknownReference && p == "message").count(),
        3,
        "{found:?}"
    );
}

#[test]
fn overrides_name_what_exists_in_the_form_they_need() {
    let found = problems(
        "groups:\n  servers: { members: [web, app] }\nmachines:\n  web: { networks: { lab: 10 }, app: { build: build/web } }\n  app: { os: debian@12, networks: { lab: 11 } }\noverrides:\n  vagrant: { machines: { servers: { box: x }, nope: { box: y } } }\n  docker: { networks: { wan: { cidr: 10.2.0.0/24 } } }\n  proxmox: { machines: { web: { lxc: true } } }\n",
    );
    assert!(has(&found, Code::UnknownReference, "overrides.vagrant.machines.servers"), "{found:?}");
    assert!(has(&found, Code::UnknownReference, "overrides.vagrant.machines.nope"), "{found:?}");
    assert!(has(&found, Code::UnknownReference, "overrides.docker.networks.wan"), "{found:?}");
    assert_eq!(found.len(), 3, "{found:?}");
}

#[test]
fn dns_servers_are_addresses_or_machines_on_a_shared_network() {
    let found = problems(
        "machines:\n  dc: { os: debian@12, networks: { lab: 10 } }\n  far: { os: debian@12, networks: { lab2: 10 } }\n  ws: { os: debian@12, networks: { lab: 11 }, dns: { servers: [dc, 10.1.0.53, far, dc2] } }\n",
    );
    assert!(
        has(&found, Code::UnknownReference, "machines.ws.dns.servers[2]") && has(&found, Code::UnknownReference, "machines.ws.dns.servers[3]"),
        "{found:?}"
    );
    assert_eq!(found.len(), 2, "{found:?}");
}

/// A spec with machine `web` on `lab`, its port named `port` (HTTP), and `message`.
fn with_message(port: &str, message: &str) -> crate::syntax::Spec {
    let yaml = format!(
        "version: 1\nname: t\nnetworks:\n  lab: {{ cidr: 10.1.0.0/24 }}\nmachines:\n  web: {{ os: debian@12, networks: {{ lab: 10 }}, ports: {{ {port}: {{ port: 80, protocol: http, publish: 8080 }} }} }}\n"
    );
    let mut spec = parse(&yaml).unwrap();
    spec.message = Some(message.to_owned());
    spec
}

proptest::proptest! {
    #[test]
    fn any_message_is_checked_without_panicking(message in "\\PC{0,80}") {
        let _ = check(&with_message("web", &message));
    }

    #[test]
    fn a_named_ports_paths_resolve(port in "[a-z][a-z0-9-]{0,30}", field in proptest::sample::select(vec!["port", "published", "url"])) {
        let spec = with_message(&port, &format!("at {{{{ machines.web.ports.{port}.{field} }}}} and {{{{ machines.web.addresses.lab }}}}"));
        proptest::prop_assert_eq!(check(&spec), []);
    }

    #[test]
    fn a_numbered_port_path_never_resolves(i in 0..10_u8, field in proptest::sample::select(vec!["port", "publish", "published"])) {
        let spec = with_message("web", &format!("{{{{ machines.web.ports.{i}.{field} }}}}"));
        proptest::prop_assert!(check(&spec).iter().any(|p| p.code == Code::UnknownReference && p.path.as_str() == "message"));
    }
}
