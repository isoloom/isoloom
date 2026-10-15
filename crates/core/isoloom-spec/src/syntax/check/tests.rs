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
fn a_windows_machine_has_no_playbook_step() {
    let found =
        problems("machines:\n  dc:\n    os: windows-server@2022\n    networks: { lab: 10 }\n    app: { steps: [{ run: dc.ps1 }, { playbook: site.yml }] }\n");
    assert_eq!(found, [(Code::InvalidValue, "machines.dc.app.steps[1].playbook".to_owned())]);
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
    let declared = problems(
        "secrets: { db: { env: DB_PASSWORD } }\nmachines:\n  web: { os: debian@12, networks: { lab: 10 }, files: [{ path: /run/secrets/db, secret: db }] }\n",
    );
    assert_eq!(declared, []);
    let windows = problems(
        "inputs: [FLAG]\nmachines:\n  dc: { os: windows-server@2019, networks: { lab: 10 }, files: [{ path: 'C:\\flag.txt', input: FLAG, mode: \"0444\" }] }\n",
    );
    assert!(
        has(&windows, Code::InvalidValue, "machines.dc.files[0].mode") && windows.len() == 1,
        "{windows:?}"
    );
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
fn each_unresolved_placeholder_says_why() {
    let why = |spec: &crate::syntax::Spec| check(spec).into_iter().map(|p| p.reason).collect::<Vec<_>>();
    assert_eq!(why(&with_message("site", "{{ machines.web.addresses.lab }}")), Vec::<String>::new());
    assert_eq!(
        why(&with_message("site", "{{ machines.web.addresses.wan }}")),
        ["`{{ machines.web.addresses.wan }}`: `web` isn't on a network named `wan`"]
    );
    assert_eq!(
        why(&with_message("site", "{{ cloud.outputs.site }}")),
        ["`{{ cloud.outputs.site }}`: the spec has no `cloud:`"]
    );
    let cloud = |name: &str| {
        let mut spec = parse("version: 1\nname: t\ncloud: { provider: aws, terraform: infra, outputs: { site: site_url } }\n").unwrap();
        spec.message = Some(format!("{{{{ cloud.outputs.{name} }}}}"));
        why(&spec)
    };
    assert_eq!(cloud("site"), Vec::<String>::new());
    assert_eq!(cloud("db"), ["`{{ cloud.outputs.db }}`: `cloud.outputs` has no `db`"]);
}

#[test]
fn an_override_names_a_declared_shared_volume() {
    let found = problems(
        "volumes: { data: {} }\nmachines:\n  web: { os: debian@12, networks: { lab: 10 } }\noverrides:\n  docker: { volumes: { data: { external: lab-data }, ghost: { external: x } } }\n",
    );
    assert_eq!(found, [(Code::UnknownReference, "overrides.docker.volumes.ghost".to_owned())]);
}

#[test]
fn a_group_override_reaches_globs_clones_and_nested_groups() {
    let found = check(
        &parse(
            "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.1.0.0/24 }\ngroups:\n  servers: { members: [web*, cl, inner] }\n  inner: { members: [api] }\nmachines:\n  web1: { networks: { lab: 10 }, app: { build: b } }\n  cl: { count: 2, networks: { lab: 20 }, app: { build: b } }\n  api: { networks: { lab: 30 }, app: { build: b } }\n  other: { networks: { lab: 40 }, app: { build: b } }\noverrides:\n  vagrant: { machines: { servers: { box: x } } }\n",
        )
        .unwrap(),
    );
    let named: Vec<String> = found
        .iter()
        .filter(|p| p.path.as_str() == "overrides.vagrant.machines.servers")
        .filter_map(|p| p.reason.split('`').nth(1).map(str::to_owned))
        .collect();
    assert_eq!(named, ["web1", "cl-01", "cl-02", "api"]);
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
    fn any_message_is_checked_and_only_the_message_is_faulted(message in "\\PC{0,80}") {
        for p in check(&with_message("web", &message)) {
            proptest::prop_assert_eq!(p.path.as_str(), "message");
        }
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

#[test]
fn steps_install_onto_a_known_os_with_files_of_its_kind() {
    let bare = problems("machines:\n  web:\n    networks: { lab: 10 }\n    app: { steps: [{ run: a.sh }] }\n");
    assert!(has(&bare, Code::MissingField, "machines.web.os"), "{bare:?}");
    let ok = problems(
        "machines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    app: { steps: [{ run: a.sh }, { playbook: p.yaml }, { copy: { from: a, to: /srv/a } }, { packages: [nginx] }, reboot] }\n",
    );
    assert_eq!(ok, []);
    let wrong = problems("machines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    app: { steps: [{ playbook: p.txt }, { run: a.ps1 }] }\n");
    for path in ["machines.web.app.steps[0].playbook", "machines.web.app.steps[1].run"] {
        assert!(has(&wrong, Code::InvalidValue, path), "{path}: {wrong:?}");
    }
    assert_eq!(wrong.len(), 2, "{wrong:?}");
    // A relative destination and no packages are refused when the spec is read.
    for step in ["{ copy: { from: a, to: srv/a } }", "{ packages: [] }"] {
        let yaml = format!("version: 1\nname: t\nmachines:\n  web: {{ os: debian@12, app: {{ steps: [{step}] }} }}\n");
        assert!(parse(&yaml).is_err(), "{step}");
    }
}

#[test]
fn a_windows_machine_steps_with_powershell_and_restarts_itself() {
    let found = problems(
        "machines:\n  dc:\n    os: windows-server@2022\n    networks: { lab: 10 }\n    app: { steps: [{ run: a.ps1 }, { copy: { from: a, to: 'C:\\app\\a' } }, { packages: [x] }, reboot] }\n",
    );
    // Packages on Windows are its features.
    assert!(has(&found, Code::InvalidValue, "machines.dc.app.steps[3]"), "{found:?}");
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn an_appliance_has_no_steps() {
    let found =
        problems("machines:\n  r1:\n    networks: { lab: 10 }\n    app: { appliance: cisco-iol, image: vrnetlab/cisco_iol:17, steps: [{ run: a.sh }] }\n");
    assert!(has(&found, Code::Conflict, "machines.r1.app.steps"), "{found:?}");
    let alone = problems("machines:\n  r1:\n    networks: { lab: 10 }\n    app: { appliance: cisco-iol, image: vrnetlab/cisco_iol:17 }\n");
    assert_eq!(alone, []);
}

#[test]
fn a_firewall_step_opens_the_machines_own_ports() {
    let found = problems(
        "machines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    ports: { http: 80 }\n    app: { steps: [{ firewall: { allow: [http, https, 8443, 53/udp] } }] }\n",
    );
    assert_eq!(found, [(Code::UnknownReference, "machines.web.app.steps[0].firewall.allow[1]".to_owned())]);
}

#[test]
fn a_suite_and_components_are_apts_alone() {
    let key = format!("key: {{ url: \"https://a.test/k\", sha256: {} }}", "0".repeat(64));
    let repo = |os: &str, extra: &str| {
        problems(&format!(
            "machines:\n  web:\n    os: {os}\n    networks: {{ lab: 10 }}\n    app: {{ steps: [{{ repository: {{ name: r, url: \"https://a.test/r\", {key}{extra} }} }}] }}\n"
        ))
    };
    assert_eq!(
        repo("rocky@9", ", suite: el9"),
        [(Code::InvalidValue, "machines.web.app.steps[0].repository.suite".to_owned())]
    );
    assert_eq!(
        repo("alpine", ", components: [main]"),
        [(Code::InvalidValue, "machines.web.app.steps[0].repository.components".to_owned())]
    );
    assert_eq!(repo("ubuntu@24.04", ", suite: noble, components: [main]"), []);
}

#[test]
fn a_linux_account_is_named_as_useradd_takes_it() {
    let found = problems(
        "machines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    app: { steps: [{ user: { name: Alice } }, { group: { name: ops } }] }\n  dc:\n    os: windows-server@2022\n    networks: { lab: 11 }\n    app: { steps: [{ user: { name: Alice } }, { group: { name: Remote Desktop Users } }] }\n",
    );
    assert_eq!(found, [(Code::InvalidValue, "machines.web.app.steps[0].user.name".to_owned())]);
}

#[test]
fn packages_on_windows_are_its_features() {
    let found = problems("machines:\n  dc:\n    os: windows-server@2022\n    networks: { lab: 10 }\n    app: { steps: [{ packages: [Web-Server] }] }\n");
    assert_eq!(found, []);
}
