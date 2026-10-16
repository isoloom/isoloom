//! The Ansible CM renderer: a recipe's steps as native tasks (one role per machine), `site.yml`
//! and `requirements.yml`. The end-state parity with the shell library is proven by the resource
//! conformance harness (`cargo xtask conformance --renderer ansible`); here the task shapes, the
//! restart notifications and the project layout are pinned.

use std::collections::BTreeMap;

use isoloom_plan::{Environment, Lab};
use isoloom_render::GeneratedFile;
use isoloom_render::configure::Configured;
use isoloom_spec::model::Target;

use super::{project, role_files, role_name};

/// The environment `target` plans for a one-machine lab whose machine `m` has `body`
/// (`os: …, app: …`), with `templates` as the project's template texts.
fn env(m: &str, target: Target, templates: &[(&str, &str)]) -> Environment {
    let yaml = format!("version: 1\nname: lab\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  m: {{ networks: {{ lan: 10 }}, {m} }}\n");
    let lab = Lab::new(isoloom_spec::parse(&yaml).expect("the test spec parses"));
    assert_eq!(lab.problems(), [], "the test spec is valid");
    let texts: BTreeMap<_, _> = templates.iter().filter_map(|(p, t)| Some((p.parse().ok()?, (*t).to_owned()))).collect();
    let lab = lab.with_templates(texts);
    Environment::build(&lab.for_target(target), target, None)
}

/// Machine `m`'s role `tasks/main.yml` text, rendered for `target`.
fn tasks(m: &str, target: Target, templates: &[(&str, &str)]) -> String {
    let e = env(m, target, templates);
    let c = Configured::of(&e, "m").expect("a recipe");
    let files = role_files(&c, "roles/isoloom_machine_m");
    file(&files, "roles/isoloom_machine_m/tasks/main.yml")
}

/// The contents of the generated file at `path`.
fn file(files: &[GeneratedFile], path: &str) -> String {
    files.iter().find(|f| f.path == path).map_or_else(
        || panic!("no {path} in {files:?}", files = files.iter().map(|f| &f.path).collect::<Vec<_>>()),
        |f| f.contents.clone(),
    )
}

#[test]
fn a_debian_recipe_uses_apt_and_deb822_native_modules() {
    let t = tasks(
        "os: debian@12, app: { steps: [{ packages: [{ name: nginx, version: \"1.30.6-1~bookworm\" }] }] }",
        Target::Vagrant,
        &[],
    );
    assert!(t.contains("ansible.builtin.apt:"), "{t}");
    assert!(t.contains("nginx=1.30.6-1~bookworm"), "{t}");
    assert!(t.contains("update_cache: true"), "{t}");
}

#[test]
fn a_rocky_package_takes_dnf_and_the_rpm_version_join() {
    let t = tasks(
        "os: rocky@9, app: { steps: [{ packages: [{ name: nginx, version: \"1.30.6\" }] }] }",
        Target::Vagrant,
        &[],
    );
    assert!(t.contains("ansible.builtin.dnf:"), "{t}");
    assert!(t.contains("nginx-1.30.6"), "{t}");
}

#[test]
fn an_alpine_package_is_a_guarded_command_not_the_community_apk_module() {
    let t = tasks("os: alpine@3.24, app: { steps: [{ packages: [curl] }] }", Target::Vagrant, &[]);
    assert!(t.contains("ansible.builtin.command:"), "{t}");
    assert!(t.contains("apk"), "{t}");
    assert!(!t.contains("community"), "{t}");
    assert!(t.contains("changed_when:"), "{t}");
}

#[test]
fn a_user_hashes_its_password_and_appends_its_groups() {
    let t = tasks(
        "os: debian@12, app: { steps: [{ user: { name: alice, uid: 1500, groups: [sudo], password: winter } }] }",
        Target::Vagrant,
        &[],
    );
    assert!(t.contains("ansible.builtin.user:"), "{t}");
    assert!(t.contains("append: true"), "{t}");
    assert!(t.contains("$6$"), "the password is a crypt hash, not the literal: {t}");
    assert!(!t.contains("winter"), "the literal password never reaches the output: {t}");
}

#[test]
fn a_command_guard_becomes_creates_or_a_registered_check() {
    let creates = tasks(
        "os: debian@12, app: { steps: [{ command: a2enmod rewrite, creates: /etc/apache2/mods-enabled/rewrite.load }] }",
        Target::Vagrant,
        &[],
    );
    assert!(
        creates.contains("ansible.builtin.shell:") && creates.contains("creates: /etc/apache2/mods-enabled/rewrite.load"),
        "{creates}"
    );
    let unless = tasks(
        "os: debian@12, app: { steps: [{ command: [touch, /x], unless: [test, -e, /x] }] }",
        Target::Vagrant,
        &[],
    );
    assert!(
        unless.contains("register: isoloom_machine_m_guard_0"),
        "the register is prefixed with the role name: {unless}"
    );
    assert!(unless.contains("isoloom_machine_m_guard_0.rc != 0"), "{unless}");
    assert!(unless.contains("changed_when: false"), "the guard's check changes nothing: {unless}");
}

#[test]
fn a_firewall_uses_ansible_posix_guarded_by_whether_firewalld_runs() {
    let t = tasks(
        "os: rocky@9, ports: { http: 80 }, app: { steps: [{ firewall: { allow: [http] } }] }",
        Target::Vagrant,
        &[],
    );
    assert!(t.contains("ansible.posix.firewalld:"), "{t}");
    assert!(t.contains("port: 80/tcp"), "{t}");
    assert!(t.contains("isoloom_machine_m_firewalld_0.rc == 0"), "{t}");
    assert!(!t.contains("community"), "{t}");
}

#[test]
fn a_template_copies_the_rendered_file_not_jinja() {
    let t = tasks(
        "os: debian@12, app: { steps: [{ template: { from: conf/app.conf, to: /etc/app.conf, mode: \"0640\" } }] }",
        Target::Vagrant,
        &[("conf/app.conf", "name = {{ machine.name }}\n")],
    );
    assert!(t.contains("src: rendered/conf/app.conf"), "{t}");
    assert!(t.contains("ansible.builtin.copy:"), "{t}");
    assert!(!t.contains("ansible.builtin.template"), "{t}");
    // The rendered file is written beside the role, its placeholder filled in.
    let e = env(
        "os: debian@12, app: { steps: [{ template: { from: conf/app.conf, to: /etc/app.conf } }] }",
        Target::Vagrant,
        &[("conf/app.conf", "name = {{ machine.name }}\n")],
    );
    let c = Configured::of(&e, "m").unwrap();
    let files = role_files(&c, "r");
    assert_eq!(file(&files, "r/files/rendered/conf/app.conf"), "name = m\n");
}

#[test]
fn a_service_gets_a_restart_handler_notified_by_the_earlier_steps() {
    let e = env(
        "os: debian@12, app: { steps: [{ packages: [nginx] }, { service: nginx }] }",
        Target::Vagrant,
        &[],
    );
    let c = Configured::of(&e, "m").unwrap();
    let files = role_files(&c, "r");
    let main = file(&files, "r/tasks/main.yml");
    assert!(
        main.contains("notify:\n    - \"Isoloom restart nginx\""),
        "the packages step notifies the restart: {main}"
    );
    assert!(main.contains("ansible.builtin.meta: flush_handlers"), "{main}");
    let handlers = file(&files, "r/handlers/main.yml");
    assert!(handlers.contains("name: \"Isoloom restart nginx\""), "{handlers}");
    assert!(handlers.contains("state: restarted"), "{handlers}");
}

/// The Ansible project a lab of `machines` (and its `overrides.external`) renders on the external
/// target, under `.isoloom/external`.
fn external_project(body: &str) -> Vec<GeneratedFile> {
    let yaml = format!("version: 1\nname: t\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\n{body}");
    let lab = Lab::new(isoloom_spec::parse(&yaml).expect("parses"));
    let e = Environment::build(&lab.for_target(Target::External), Target::External, None);
    project(&e, ".isoloom/external")
}

#[test]
fn an_external_project_has_a_play_and_role_per_configured_host() {
    let files = external_project(
        "machines:
  web: { networks: { lab: 5 }, os: debian@12, app: { steps: [{ packages: [nginx] }, { service: nginx }] } }
  bare: { networks: { lab: 6 }, access: true }
overrides:
  external: { machines: { web: { ssh: { address: 10.0.0.5 } }, bare: { ssh: { address: 10.0.0.6 } } } }
",
    );
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&".isoloom/external/site.yml"), "{paths:?}");
    assert!(paths.contains(&".isoloom/external/roles/isoloom_machine_web/tasks/main.yml"), "{paths:?}");
    assert!(
        !paths.contains(&".isoloom/external/requirements.yml"),
        "a firewall-free recipe names no collection: {paths:?}"
    );
    // `bare` has no recipe, so no role and no play; `web` does.
    assert!(!paths.iter().any(|p| p.contains("isoloom_machine_bare")), "{paths:?}");
    let site = file(&files, ".isoloom/external/site.yml");
    assert!(site.contains("hosts: web"), "{site}");
    assert!(site.contains("- isoloom_machine_web"), "{site}");
    assert_eq!(role_name("web"), "isoloom_machine_web");
}

#[test]
fn a_firewall_recipe_pins_ansible_posix_in_requirements() {
    let files = external_project(
        "machines:
  web: { networks: { lab: 5 }, ports: { http: 80 }, os: rocky@9, app: { steps: [{ firewall: { allow: [http] } }] } }
overrides:
  external: { machines: { web: { ssh: { address: 10.0.0.5 } } } }
",
    );
    let reqs = file(&files, ".isoloom/external/requirements.yml");
    assert!(reqs.contains("name: ansible.posix"), "{reqs}");
    assert!(reqs.contains("version:"), "{reqs}");
}
