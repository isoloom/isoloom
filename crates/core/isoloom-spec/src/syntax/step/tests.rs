use super::*;
use crate::model::Protocol;
use crate::syntax::{Code, Problem, parse};

fn step(yaml: &str) -> Step {
    serde_yaml_ng::from_str(yaml).unwrap_or_else(|e| panic!("{yaml}: {e}"))
}

/// The problems of a machine whose only step is `yaml`, as `(code, path, fix)`.
fn mistakes(yaml: &str) -> Vec<(Code, String, Option<String>)> {
    let spec = format!("version: 1\nname: t\nmachines:\n  web:\n    os: debian@12\n    app:\n      steps:\n        - {{ run: a.sh }}\n        - {yaml}\n");
    parse(&spec)
        .expect_err("a mistake")
        .into_iter()
        .map(|p: Problem| (p.code, p.path.to_string(), p.fix))
        .collect()
}

#[test]
fn the_approved_five_keep_their_syntax() {
    assert_eq!(
        step("{ run: provision/web.sh, creates: /srv/.done }"),
        Step::Run {
            script: "provision/web.sh".parse().unwrap(),
            creates: Some("/srv/.done".parse().unwrap())
        }
    );
    let Step::Copy(c) = step("{ copy: { from: site/, to: /var/www/html/, owner: www-data, mode: \"0755\" } }") else {
        panic!()
    };
    assert!(c.is_folder() && c.owner.is_some_and(|o| o.as_str() == "www-data"));
    assert_eq!(step("reboot"), Step::Reboot);
    assert!(matches!(step("{ playbook: site.yml }"), Step::Playbook(p) if p.as_str() == "site.yml"));
}

#[test]
fn a_package_is_a_name_or_a_name_and_its_version() {
    let Step::Packages(p) = step("{ packages: [curl, { name: nginx, version: 1.30.6-1~bookworm }] }") else {
        panic!()
    };
    assert_eq!((p[0].name.as_str(), p[0].version.as_ref()), ("curl", None));
    assert_eq!(p[1].version.as_ref().map(PackageVersion::as_str), Some("1.30.6-1~bookworm"));
    assert_eq!(p[1].to_string(), "nginx 1.30.6-1~bookworm");
}

#[test]
fn a_repository_pins_its_key() {
    let Step::Repository(r) = step(&format!(
        "{{ repository: {{ name: nginx, url: \"https://nginx.org/packages/debian\", components: [nginx], key: {{ url: \"https://nginx.org/keys/nginx_signing.key\", sha256: {} }}, priority: 900 }} }}",
        "4c".repeat(32)
    )) else {
        panic!()
    };
    assert_eq!((r.name.as_str(), r.priority, r.suite), ("nginx", Some(900), None));
    assert_eq!(r.key.sha256.as_str(), "4c".repeat(32));
}

#[test]
fn a_file_step_makes_one_state_of_its_path() {
    let Step::File(f) = step("{ file: { path: /var/lib/parcels, state: directory, owner: parcels, mode: \"0750\" } }") else {
        panic!()
    };
    assert_eq!(f.state, FileState::Directory);
    let Step::File(gone) = step("{ file: { path: /usr/share/nginx/html, state: absent } }") else {
        panic!()
    };
    assert_eq!(gone.state, FileState::Absent);
    let Step::File(link) = step("{ file: { path: /usr/bin/py, link: /usr/bin/python3 } }") else {
        panic!()
    };
    assert!(matches!(link.state, FileState::Link(t) if t.as_str() == "/usr/bin/python3"));
    let Step::File(download) = step(&format!(
        "{{ file: {{ path: /opt/tool.tgz, url: \"https://example.org/tool.tgz\", sha256: {} }} }}",
        "9f".repeat(32)
    )) else {
        panic!()
    };
    assert!(matches!(download.state, FileState::Download { .. }));
}

#[test]
fn accounts_and_templates_parse_into_their_fields() {
    let Step::User(u) = step("{ user: { name: alice, uid: 1500, groups: [sudo], password: winter, system: true, shell: /usr/sbin/nologin } }") else {
        panic!()
    };
    assert_eq!((u.uid, u.system, u.password.as_ref().map(Password::as_str)), (Some(1500), true, Some("winter")));
    let Step::Group(g) = step("{ group: { name: ops, gid: 1600 } }") else {
        panic!()
    };
    assert_eq!((g.name.as_str(), g.gid, g.system), ("ops", Some(1600), false));
    let Step::Template(t) = step("{ template: { from: conf/app.toml, to: /etc/app.toml, mode: \"0640\" } }") else {
        panic!()
    };
    assert_eq!((t.from.as_str(), t.to.as_str()), ("conf/app.toml", "/etc/app.toml"));
}

#[test]
fn a_service_is_a_packaged_unit_or_one_isoloom_defines() {
    assert_eq!(
        step("{ service: nginx }"),
        Step::Service(Service::Unit {
            name: "nginx".parse().unwrap(),
            enabled: true
        })
    );
    let Step::Service(off) = step("{ service: { name: nginx, enabled: false } }") else {
        panic!()
    };
    assert!(!off.enabled() && matches!(off, Service::Unit { .. }));
    let Step::Service(api) = step("{ service: { name: api, run: [/srv/api/bin/api, --port, \"8000\"], user: api, cwd: /srv/api } }") else {
        panic!()
    };
    let Service::Defined { run, user, .. } = &api else { panic!() };
    assert_eq!(
        (run.program(), run.args().len(), user.as_ref().map(UserName::as_str)),
        ("/srv/api/bin/api", 3, Some("api"))
    );
}

#[test]
fn a_command_runs_behind_exactly_one_guard() {
    let Step::Command(c) = step("{ command: a2enmod rewrite, creates: /etc/apache2/mods-enabled/rewrite.load }") else {
        panic!()
    };
    assert!(matches!(c.guard, Guard::Creates(_)) && matches!(c.run, Invocation::Shell(_)));
    let Step::Command(checked) = step("{ command: [systemctl, mask, tmp.mount], unless: \"systemctl is-enabled tmp.mount | grep -q masked\" }") else {
        panic!()
    };
    assert!(matches!(checked.guard, Guard::Unless(Invocation::Shell(_))) && matches!(checked.run, Invocation::Exec(_)));
}

#[test]
fn a_firewall_opens_named_and_numbered_ports() {
    let Step::Firewall(f) = step("{ firewall: { allow: [http, 8443, 53/udp, 22/tcp] } }") else {
        panic!()
    };
    assert!(matches!(&f.allow[0], FirewallPort::Named(n) if n.as_str() == "http"));
    assert!(matches!(f.allow[1], FirewallPort::Number { port, protocol: Protocol::Tcp } if port.get() == 8443));
    assert!(matches!(f.allow[2], FirewallPort::Number { protocol: Protocol::Udp, .. }));
    assert_eq!(step("{ firewall: { allow: [http] } }").kind(), "firewall");
}

#[test]
fn a_command_without_its_guard_says_how_to_tell_it_ran() {
    let found = mistakes("{ command: a2enmod rewrite }");
    assert_eq!(found[0].0, Code::MissingField);
    assert_eq!(found[0].1, "machines.web.app.steps[1]");
    assert!(
        found[0].2.as_deref().is_some_and(|f| f.contains("`creates: <path>` or `unless: <check>`")),
        "{found:?}"
    );
    let both = mistakes("{ command: x, creates: /a, unless: y }");
    assert_eq!((both[0].0, both[0].1.as_str()), (Code::Conflict, "machines.web.app.steps[1]"));
}

#[test]
fn a_download_without_its_checksum_points_at_its_url() {
    let found = mistakes("{ file: { path: /opt/tool.tgz, url: \"https://example.org/tool.tgz\" } }");
    assert_eq!(
        found,
        [(
            Code::MissingField,
            "machines.web.app.steps[1].file.url".to_owned(),
            Some("`sha256: <hex>` (`sha256sum <file>` prints it)".to_owned())
        )]
    );
}

#[test]
fn a_secret_password_in_a_step_is_refused_as_sensitive() {
    let found = mistakes("{ user: { name: alice, password: { secret: alice } } }");
    assert_eq!(
        (found[0].0, found[0].1.as_str()),
        (Code::SensitiveValue, "machines.web.app.steps[1].user.password")
    );
    let input = mistakes("{ user: { name: alice, password: { input: ALICE } } }");
    assert_eq!(input[0].0, Code::SensitiveValue);
}

#[test]
fn each_malformed_step_is_coded_at_its_field() {
    for (yaml, code, path) in [
        ("{ packages: [\"nginx=1.2\"] }", Code::InvalidValue, "machines.web.app.steps[1].packages[0]"),
        ("{ packages: [] }", Code::InvalidValue, "machines.web.app.steps[1]"),
        (
            "{ copy: { from: /etc/passwd, to: /x } }",
            Code::InvalidValue,
            "machines.web.app.steps[1].copy.from",
        ),
        ("{ copy: { from: a, to: relative } }", Code::InvalidValue, "machines.web.app.steps[1].copy.to"),
        (
            "{ file: { path: /a, state: absent, mode: \"0600\" } }",
            Code::Conflict,
            "machines.web.app.steps[1].file",
        ),
        (
            "{ file: { path: /a, state: directory, link: /b } }",
            Code::Conflict,
            "machines.web.app.steps[1].file",
        ),
        ("{ file: { path: /a } }", Code::MissingField, "machines.web.app.steps[1].file"),
        (
            "{ template: { from: conf/, to: /etc/x } }",
            Code::InvalidValue,
            "machines.web.app.steps[1].template.from",
        ),
        (
            "{ service: { name: nginx, user: www-data } }",
            Code::Conflict,
            "machines.web.app.steps[1].service.user",
        ),
        (
            "{ repository: { name: x, url: \"http://a\", key: { url: \"https://a/k\", sha256: x } } }",
            Code::InvalidValue,
            "machines.web.app.steps[1].repository.url",
        ),
        (
            "{ firewall: { allow: [99999] } }",
            Code::InvalidValue,
            "machines.web.app.steps[1].firewall.allow[0]",
        ),
        ("{ unless: x, run: a.sh }", Code::InvalidValue, "machines.web.app.steps[1]"),
        ("{ services: nginx }", Code::UnknownField, "machines.web.app.steps[1].services"),
    ] {
        let found = mistakes(yaml);
        assert_eq!((found[0].0, found[0].1.as_str()), (code, path), "{yaml}: {found:?}");
    }
}

#[test]
fn a_version_written_into_the_name_moves_to_version() {
    let found = mistakes("{ packages: [\"nginx=1.2\"] }");
    assert_eq!(found[0].2.as_deref(), Some("`{ name: nginx, version: \"1.2\" }`"));
}

#[test]
fn a_step_does_one_thing() {
    let e = serde_yaml_ng::from_str::<Step>("{ user: { name: a }, group: { name: b } }")
        .unwrap_err()
        .to_string();
    assert!(e.contains("does one thing"), "{e}");
    let alone = serde_yaml_ng::from_str::<Step>("{ creates: /a }").unwrap_err().to_string();
    assert!(alone.contains("goes with `run` or `command`"), "{alone}");
}

#[test]
fn only_reboot_playbook_and_powershell_lack_a_container_form() {
    for (yaml, none) in [
        ("reboot", false),
        ("{ playbook: a.yml }", false),
        ("{ run: a.ps1 }", false),
        ("{ run: a.sh }", true),
        ("{ service: nginx }", true),
        ("{ firewall: { allow: [80] } }", true),
    ] {
        assert_eq!(step(yaml).container_refusal().is_none(), none, "{yaml}");
    }
}
