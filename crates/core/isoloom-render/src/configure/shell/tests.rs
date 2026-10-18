//! The shell rendering: each step kind's library call, its arguments quoted, and the VM's
//! set-up step around them.

use isoloom_spec::model::Target;

use super::super::compact;
use super::*;
use crate::testing;

/// Each step's call of a machine on `os` with `steps`, in order (`None` for a step the library
/// doesn't run).
fn calls(os: &str, steps: &str) -> Vec<Option<String>> {
    calls_with(os, steps, &[])
}

/// As [`calls`], with the project's template texts.
fn calls_with(os: &str, steps: &str, templates: &[(&str, &str)]) -> Vec<Option<String>> {
    let m = format!("os: {os}, ports: {{ web: 80, dns: {{ port: 53, protocol: udp }} }}, app: {{ steps: {steps} }}");
    testing::rendered(&m, Target::Vagrant, templates, call)
}

/// The one call of a machine on `os` with `step`.
fn one(os: &str, step: &str) -> String {
    calls(os, &format!("[{step}]")).remove(0).expect("a library call")
}

const KEY: &str = "key: { url: \"https://nginx.org/keys/nginx_signing.key\", sha256: 4c3f000000000000000000000000000000000000000000000000000000000000 }";

#[test]
fn packages_are_installed_by_name_or_at_their_version() {
    assert_eq!(
        one("debian@12", "{ packages: [curl, { name: nginx, version: 1.30.6-1~bookworm }] }"),
        "isoloom_packages 'curl' 'nginx=1.30.6-1~bookworm'\n"
    );
}

#[test]
fn a_repository_is_of_the_kind_of_its_family() {
    let apt = one(
        "debian@12",
        &format!("{{ repository: {{ name: nginx, url: \"https://nginx.org/packages/debian\", components: [nginx], {KEY} }} }}"),
    );
    assert_eq!(
        apt,
        "isoloom_repository 'apt' 'nginx' 'https://nginx.org/packages/debian' 'https://nginx.org/keys/nginx_signing.key' '4c3f000000000000000000000000000000000000000000000000000000000000' '' 'bookworm' 'nginx'\n"
    );
    let rpm = one(
        "rocky@9",
        &format!("{{ repository: {{ name: nginx, url: \"https://nginx.org/packages/centos/9/x86_64/\", priority: 10, {KEY} }} }}"),
    );
    assert!(
        rpm.starts_with("isoloom_repository 'rpm' 'nginx' 'https://nginx.org/packages/centos/9/x86_64/' "),
        "{rpm}"
    );
    assert!(rpm.ends_with(" '10'\n"), "no suite or components on rpm: {rpm}");
    assert!(
        one(
            "alpine@3.19",
            &format!("{{ repository: {{ name: edge, url: \"https://dl-cdn.alpinelinux.org/alpine/edge/main\", {KEY} }} }}")
        )
        .starts_with("isoloom_repository 'apk' ")
    );
}

#[test]
fn copies_and_files_carry_their_owner_group_and_mode() {
    assert_eq!(
        one("debian@12", "{ copy: { from: site/, to: /var/www/, owner: www-data, mode: \"0755\" } }"),
        "isoloom_copy 'site/' '/var/www/' 'www-data' '' '0755'\n"
    );
    assert_eq!(
        one("debian@12", "{ file: { path: /srv/data, state: directory, owner: app, mode: \"0750\" } }"),
        "isoloom_file '/srv/data' 'app' '' '0750' 'directory'\n"
    );
    assert_eq!(
        one("debian@12", "{ file: { path: /srv/old, state: absent } }"),
        "isoloom_file '/srv/old' '' '' '' 'absent'\n"
    );
    assert_eq!(
        one("debian@12", "{ file: { path: /srv/current, link: /srv/v2 } }"),
        "isoloom_file '/srv/current' '' '' '' 'link' '/srv/v2'\n"
    );
    let sha = "9f2c".to_owned() + &"0".repeat(60);
    assert_eq!(
        one(
            "debian@12",
            &format!("{{ file: {{ path: /opt/tool.tgz, url: \"https://example.org/tool.tgz\", sha256: {sha} }} }}")
        ),
        format!("isoloom_file '/opt/tool.tgz' '' '' '' 'download' 'https://example.org/tool.tgz' '{sha}'\n")
    );
}

#[test]
fn a_template_goes_in_filled_and_encoded() {
    let found = calls_with(
        "debian@12",
        "[{ template: { from: conf/app.toml, to: /etc/app.toml, group: app, mode: \"0640\" } }]",
        &[("conf/app.toml", "name = '{{ machine.name }}'\n")],
    );
    let encoded = isoloom_spec::shell::base64(b"name = 'm'\n");
    assert_eq!(
        found[0].as_deref(),
        Some(format!("isoloom_template '/etc/app.toml' '' 'app' '0640' '{encoded}'\n").as_str())
    );
    let unread = calls("debian@12", "[{ template: { from: conf/app.toml, to: /etc/app.toml } }]");
    assert!(
        unread[0]
            .as_deref()
            .is_some_and(|l| l.starts_with("isoloom_fail 'the template conf/app.toml wasn'\\''t read")),
        "{unread:?}"
    );
}

#[test]
fn users_and_groups_carry_their_fields_and_a_hashed_password() {
    let user = one(
        "debian@12",
        "{ user: { name: alice, uid: 1500, group: ops, groups: [sudo, adm], home: /home/alice, shell: /bin/bash, password: winter } }",
    );
    assert!(
        user.starts_with("isoloom_user 'alice' '1500' 'ops' 'sudo,adm' '/home/alice' '/bin/bash' 'no' '$6$"),
        "{user}"
    );
    assert!(!user.contains("winter"), "the password goes in hashed: {user}");
    assert_eq!(
        one("debian@12", "{ user: { name: svc, system: true } }"),
        "isoloom_user 'svc' '' '' '' '' '' 'yes' ''\n"
    );
    assert_eq!(one("debian@12", "{ group: { name: ops, gid: 1600 } }"), "isoloom_group 'ops' '1600' 'no'\n");
}

#[test]
fn a_service_is_the_unit_its_family_names_and_one_isoloom_defines_comes_with_its_unit() {
    assert_eq!(one("debian@12", "{ service: ssh }"), "isoloom_service 'ssh' 'yes'\n");
    assert_eq!(one("rocky@9", "{ service: ssh }"), "isoloom_service 'sshd' 'yes'\n");
    assert_eq!(
        one("debian@12", "{ service: { name: nginx, enabled: false } }"),
        "isoloom_service 'nginx' 'no'\n"
    );
    let defined = one("debian@12", "{ service: { name: api, run: [/srv/api, --port, '8000'], user: api, cwd: /srv } }");
    assert!(defined.starts_with("isoloom_unit 'api' '[Unit]\n"), "{defined}");
    assert!(
        defined.contains("ExecStart=\"/srv/api\" \"--port\" \"8000\"\nUser=api\nWorkingDirectory=/srv\n"),
        "{defined}"
    );
    assert!(defined.ends_with("'\nisoloom_service 'api' 'yes'\n"), "{defined}");
    assert!(one("alpine@3.19", "{ service: { name: api, run: [/srv/api] } }").starts_with("isoloom_unit 'api' '#!/sbin/openrc-run\n"));
}

#[test]
fn a_command_runs_behind_its_guard() {
    assert_eq!(
        one("debian@12", "{ command: a2enmod rewrite, creates: /etc/apache2/mods-enabled/rewrite.load }"),
        "isoloom_command 'creates' '1' '/etc/apache2/mods-enabled/rewrite.load' 'sh' '-c' 'a2enmod rewrite'\n"
    );
    assert_eq!(
        one("debian@12", "{ command: [useradd, -r, svc], unless: [id, svc] }"),
        "isoloom_command 'unless' '2' 'id' 'svc' 'useradd' '-r' 'svc'\n"
    );
    assert_eq!(
        one("debian@12", "{ command: [touch, /done], unless: test -e /done }"),
        "isoloom_command 'unless' '3' 'sh' '-c' 'test -e /done' 'touch' '/done'\n"
    );
}

#[test]
fn a_firewall_opens_its_ports_by_number_and_transport() {
    assert_eq!(
        one("rocky@9", "{ firewall: { allow: [web, dns, 8443] } }"),
        "isoloom_firewall '80/tcp' '53/udp' '8443/tcp'\n"
    );
}

#[test]
fn scripts_reboots_and_playbooks_are_the_set_ups_own_steps() {
    assert_eq!(calls("debian@12", "[{ run: a.sh }, reboot, { playbook: p.yml }]"), [None, None, None]);
}

#[test]
fn a_vm_step_runs_the_library_then_its_calls_from_the_project_folder() {
    let env = testing::machine("os: debian@12, app: { steps: [{ packages: [curl] }] }", Target::Vagrant, &[]);
    let c = Configured::of(&env, "m").unwrap();
    let resources: Vec<Resource> = c.recipe().resources().cloned().collect();
    let fresh = script(&c, &resources, true);
    assert!(fresh.starts_with("(\nset -eu\nisoloom_changed() {\n"), "{fresh}");
    assert!(
        fresh.contains("\nisoloom_install() {\n") && !fresh.contains("isoloom_user()"),
        "only what the calls need: {fresh}"
    );
    assert!(
        fresh.ends_with("ISOLOOM_CHANGED=/run/isoloom/configure.changed\nrm -f \"$ISOLOOM_CHANGED\"\ncd /opt/isoloom\nisoloom_packages 'curl'\n)\n"),
        "{fresh}"
    );
    let after = script(&c, &resources, false);
    assert!(after.contains("mkdir -p /run/isoloom && : >\"$ISOLOOM_CHANGED\"\n"), "{after}");
}

#[test]
fn every_function_the_library_calls_is_one_of_its_own() {
    let functions = functions(LIBRARY);
    assert_eq!(functions.len(), LIBRARY.matches("\n}\n").count(), "every function is found");
    let names: BTreeSet<String> = functions.iter().map(|(n, _)| n.clone()).collect();
    for (name, text) in &functions {
        let unknown: Vec<String> = called(text).into_iter().filter(|c| !names.contains(c)).collect();
        assert!(unknown.is_empty(), "{name} calls {unknown:?}");
    }
    assert_eq!(
        needed(LIBRARY).lines().count(),
        compact(LIBRARY).lines().filter(|l| !l.starts_with('#')).count()
    );
}

#[test]
fn the_library_file_starts_with_the_generated_header() {
    let f = library_file();
    assert_eq!(f.path, ".isoloom/configure/lib/configure.sh");
    assert!(f.contents.starts_with("# Generated by isoloom") && f.contents.ends_with(LIBRARY));
}

/// The library and a recipe's script read as POSIX sh.
#[cfg(unix)]
#[test]
fn the_library_and_a_script_parse_as_sh() {
    let env = testing::machine(
        "os: debian@12, app: { steps: [{ packages: [curl] }, { user: { name: \"a\", password: \"it's\" } }, { command: \"echo 'x' | tr x y\", creates: /done }] }",
        Target::Vagrant,
        &[],
    );
    let c = Configured::of(&env, "m").unwrap();
    let resources: Vec<Resource> = c.recipe().resources().cloned().collect();
    let status = std::process::Command::new("sh")
        .args(["-n", "-c", &script(&c, &resources, true)])
        .status()
        .expect("sh runs");
    assert!(status.success());
}
