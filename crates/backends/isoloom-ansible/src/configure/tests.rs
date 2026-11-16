//! The Ansible CM renderer: a recipe's steps as native tasks (one role per machine), `site.yml`
//! and `requirements.yml`. The end-state parity with the shell library is proven by the resource
//! conformance harness (`cargo xtask conformance --renderer ansible`); here the task shapes, the
//! restart notifications and the project layout are pinned.

use std::collections::BTreeMap;

use isoloom_plan::{Environment, Lab};
use isoloom_render::GeneratedFile;
use isoloom_render::configure::Configured;
use isoloom_spec::model::Target;
use serde_yaml_ng::Value;

use super::{project, role_files, role_name};
use crate::testing::untagged;

/// The environment `target` plans for a one-machine environment whose machine `m` has `body`
/// (`os: …, app: …`), with `templates` as the project's template texts.
fn env(m: &str, target: Target, templates: &[(&str, &str)]) -> Environment {
    let yaml = format!("version: 1\nname: lab\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  m: {{ networks: {{ lan: 10 }}, {m} }}\n");
    let lab = Lab::new(isoloom_spec::parse(&yaml).expect("the test spec parses"));
    assert_eq!(lab.problems(), [], "the test spec is valid");
    let texts: BTreeMap<_, _> = templates.iter().filter_map(|(p, t)| Some((p.parse().ok()?, (*t).to_owned()))).collect();
    let lab = lab.with_templates(texts);
    lab.plan(&lab.for_target(target), target, None).expect("the spec plans")
}

/// Machine `m`'s role `tasks/main.yml` text, rendered for `target`.
fn tasks(m: &str, target: Target, templates: &[(&str, &str)]) -> String {
    let e = env(m, target, templates);
    let c = Configured::of(&e, "m").expect("a recipe");
    let files = role_files(&c, "roles/isoloom_machine_m").expect("rendered");
    file(&files, "roles/isoloom_machine_m/tasks/main.yml")
}

/// The contents of the generated file at `path`.
fn file(files: &[GeneratedFile], path: &str) -> String {
    files.iter().find(|f| f.path == path).map_or_else(
        || panic!("no {path} in {files:?}", files = files.iter().map(|f| &f.path).collect::<Vec<_>>()),
        |f| f.contents.clone(),
    )
}

/// Each task of `tasks` (a role's or the handlers' list) that runs `module`.
fn using<'t>(tasks: &'t Value, module: &str) -> Vec<&'t Value> {
    tasks.as_sequence().into_iter().flatten().filter(|t| t.get(module).is_some()).collect()
}

/// The one task of `tasks` that runs `module`.
fn one<'t>(tasks: &'t Value, module: &str) -> &'t Value {
    match using(tasks, module).as_slice() {
        [task] => task,
        found => panic!("{} tasks run {module}: {tasks:?}", found.len()),
    }
}

/// The scalar text at `key` of `value`.
fn text_at<'v>(value: &'v Value, key: &str) -> Option<&'v str> {
    value.get(key).and_then(Value::as_str)
}

/// The texts of the list at `key` of `value`.
fn texts_at<'v>(value: &'v Value, key: &str) -> Vec<&'v str> {
    value
        .get(key)
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

#[test]
fn a_debian_recipe_uses_apt_and_deb822_native_modules() {
    let t = tasks(
        "os: debian@12, app: { steps: [{ packages: [{ name: nginx, version: \"1.30.6-1~bookworm\" }] }] }",
        Target::Vagrant,
        &[],
    );
    let tasks = untagged(&t);
    let apt = &one(&tasks, "ansible.builtin.apt")["ansible.builtin.apt"];
    assert_eq!(texts_at(apt, "name"), ["nginx=1.30.6-1~bookworm"], "{t}");
    assert_eq!(apt["update_cache"].as_bool(), Some(true), "{t}");
}

#[test]
fn a_rocky_package_takes_dnf_and_the_rpm_version_join() {
    let t = tasks(
        "os: rocky@9, app: { steps: [{ packages: [{ name: nginx, version: \"1.30.6\" }] }] }",
        Target::Vagrant,
        &[],
    );
    let tasks = untagged(&t);
    assert_eq!(
        texts_at(&one(&tasks, "ansible.builtin.dnf")["ansible.builtin.dnf"], "name"),
        ["nginx-1.30.6"],
        "{t}"
    );
}

#[test]
fn an_alpine_package_is_a_guarded_command_not_the_community_apk_module() {
    let t = tasks("os: alpine@3.24, app: { steps: [{ packages: [curl] }] }", Target::Vagrant, &[]);
    let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&t).expect("the tasks are YAML");
    let commands: Vec<&serde_yaml_ng::Value> = parsed
        .as_sequence()
        .expect("a task list")
        .iter()
        .filter_map(|task| task.get("ansible.builtin.command"))
        .collect();
    let argv: Vec<&str> = match commands.as_slice() {
        [one] => one
            .get("argv")
            .and_then(|a| a.as_sequence())
            .expect("an argv")
            .iter()
            .filter_map(|a| a.as_str())
            .collect(),
        _ => panic!("one command task, not {}: {t}", commands.len()),
    };
    assert_eq!(argv, ["apk", "add", "--no-cache", "curl"], "{t}");
    assert!(!t.contains("community."), "no community module: {t}");
    let guarded = parsed
        .as_sequence()
        .unwrap()
        .iter()
        .find(|task| task.get("ansible.builtin.command").is_some())
        .and_then(|task| task.get("changed_when"));
    assert!(
        guarded.and_then(|g| g.as_str()).is_some_and(|g| g.contains("isoloom_apk.stdout")),
        "changed only when apk installed something: {t}"
    );
}

#[test]
fn a_user_hashes_its_password_and_appends_its_groups() {
    let t = tasks(
        "os: debian@12, app: { steps: [{ user: { name: alice, uid: 1500, groups: [sudo], password: winter } }] }",
        Target::Vagrant,
        &[],
    );
    let tasks = untagged(&t);
    let user = &one(&tasks, "ansible.builtin.user")["ansible.builtin.user"];
    assert_eq!(texts_at(user, "groups"), ["sudo"], "{t}");
    assert_eq!(user["append"].as_bool(), Some(true), "{t}");
    assert_eq!(user["uid"].as_u64(), Some(1500), "{t}");
    assert!(
        text_at(user, "password").is_some_and(|p| p.starts_with("$6$")),
        "the password is a crypt hash, not the literal: {t}"
    );
    assert!(!t.contains("winter"), "the literal password never reaches the output: {t}");
}

#[test]
fn a_command_guard_becomes_creates_or_a_registered_check() {
    let creates = tasks(
        "os: debian@12, app: { steps: [{ command: a2enmod rewrite, creates: /etc/apache2/mods-enabled/rewrite.load }] }",
        Target::Vagrant,
        &[],
    );
    let creating = untagged(&creates);
    let shell = &one(&creating, "ansible.builtin.shell")["ansible.builtin.shell"];
    assert_eq!(text_at(shell, "creates"), Some("/etc/apache2/mods-enabled/rewrite.load"), "{creates}");
    let unless = tasks(
        "os: debian@12, app: { steps: [{ command: [touch, /x], unless: [test, -e, /x] }] }",
        Target::Vagrant,
        &[],
    );
    let tasks = untagged(&unless);
    let tasks = tasks.as_sequence().expect("a task list");
    // The guard's check, registered under the role's own prefix, changes nothing; the command runs
    // when it failed.
    let check = tasks
        .iter()
        .find(|t| text_at(t, "register") == Some("isoloom_machine_m_guard_0"))
        .unwrap_or_else(|| panic!("the register is prefixed with the role name: {unless}"));
    assert_eq!(check["changed_when"].as_bool(), Some(false), "{unless}");
    let guarded: Vec<&Value> = tasks
        .iter()
        .filter(|t| text_at(t, "when") == Some("isoloom_machine_m_guard_0.rc != 0"))
        .collect();
    assert_eq!(guarded.len(), 1, "{unless}");
}

#[test]
fn a_firewall_uses_ansible_posix_guarded_by_whether_firewalld_runs() {
    let t = tasks(
        "os: rocky@9, ports: { http: 80 }, app: { steps: [{ firewall: { allow: [http] } }] }",
        Target::Vagrant,
        &[],
    );
    let tasks = untagged(&t);
    let firewalld = one(&tasks, "ansible.posix.firewalld");
    assert_eq!(text_at(&firewalld["ansible.posix.firewalld"], "port"), Some("80/tcp"), "{t}");
    assert_eq!(text_at(firewalld, "when"), Some("isoloom_machine_m_firewalld_0.rc == 0"), "{t}");
    assert!(!t.contains("community"), "{t}");
}

#[test]
fn a_template_copies_the_rendered_file_not_jinja() {
    let t = tasks(
        "os: debian@12, app: { steps: [{ template: { from: conf/app.conf, to: /etc/app.conf, mode: \"0640\" } }] }",
        Target::Vagrant,
        &[("conf/app.conf", "name = {{ machine.name }}\n")],
    );
    let tasks = untagged(&t);
    let copy = &one(&tasks, "ansible.builtin.copy")["ansible.builtin.copy"];
    assert_eq!(text_at(copy, "src"), Some("rendered/conf/app.conf"), "{t}");
    assert_eq!(text_at(copy, "mode"), Some("0640"), "{t}");
    assert!(using(&tasks, "ansible.builtin.template").is_empty(), "{t}");
    // The rendered file is written beside the role, its placeholder filled in.
    let e = env(
        "os: debian@12, app: { steps: [{ template: { from: conf/app.conf, to: /etc/app.conf } }] }",
        Target::Vagrant,
        &[("conf/app.conf", "name = {{ machine.name }}\n")],
    );
    let c = Configured::of(&e, "m").unwrap();
    let files = role_files(&c, "r").unwrap();
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
    let files = role_files(&c, "r").unwrap();
    let main = file(&files, "r/tasks/main.yml");
    let tasks = untagged(&main);
    assert_eq!(
        texts_at(one(&tasks, "ansible.builtin.apt"), "notify"),
        ["Isoloom restart nginx"],
        "the packages step notifies the restart: {main}"
    );
    assert_eq!(
        text_at(one(&tasks, "ansible.builtin.meta"), "ansible.builtin.meta"),
        Some("flush_handlers"),
        "{main}"
    );
    let handlers = file(&files, "r/handlers/main.yml");
    let handlers = untagged(&handlers);
    let [restart] = handlers.as_sequence().map(Vec::as_slice).unwrap_or_default() else {
        panic!("one handler: {handlers:?}")
    };
    assert_eq!(text_at(restart, "name"), Some("Isoloom restart nginx"), "{handlers:?}");
    let restarted: Vec<&str> = restart
        .as_mapping()
        .into_iter()
        .flatten()
        .filter_map(|(_, args)| text_at(args, "state"))
        .collect();
    assert_eq!(restarted, ["restarted"], "{handlers:?}");
}

/// The Ansible project an environment of `machines` (and its `overrides.external`) renders on the external
/// target, under `.isoloom/external`.
fn external_project(body: &str) -> Vec<GeneratedFile> {
    let yaml = format!("version: 1\nname: t\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\n{body}");
    let lab = Lab::new(isoloom_spec::parse(&yaml).expect("parses"));
    let e = lab.plan(&lab.for_target(Target::External), Target::External, None).expect("the spec plans");
    project(&e, ".isoloom/external").expect("rendered")
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
    let site = untagged(&file(&files, ".isoloom/external/site.yml"));
    let plays: Vec<(&str, Vec<&str>)> = site
        .as_sequence()
        .into_iter()
        .flatten()
        .map(|p| (text_at(p, "hosts").unwrap_or_default(), texts_at(p, "roles")))
        .collect();
    assert_eq!(plays, [("web", vec![role_name("web").as_str()])]);
}

#[test]
fn a_hyphenated_machine_gets_a_role_and_variables_jinja_reads_as_names() {
    let files = external_project(
        "machines:
  web-front: { networks: { lab: 5 }, os: debian@12, app: { steps: [{ run: setup/web.sh }, { packages: [nginx] }] } }
overrides:
  external: { machines: { web-front: { ssh: { address: 10.0.0.5 } } } }
",
    );
    let site = untagged(&file(&files, ".isoloom/external/site.yml"));
    assert_eq!(text_at(&site[0], "hosts"), Some("web-front"), "{site:?}");
    assert_eq!(texts_at(&site[0], "roles"), ["isoloom_machine_web_front"], "{site:?}");
    let text = file(&files, ".isoloom/external/roles/isoloom_machine_web_front/tasks/main.yml");
    // `{{ isoloom_machine_web-front_file }}` would be a subtraction.
    let script = one(&untagged(&text), "ansible.builtin.script").clone();
    assert_eq!(
        text_at(&script["ansible.builtin.script"], "cmd"),
        Some("{{ isoloom_project }}/{{ isoloom_machine_web_front_file }}"),
        "{text}"
    );
    assert_eq!(text_at(&script["vars"], "isoloom_machine_web_front_file"), Some("setup/web.sh"), "{text}");
    assert!(!text.contains("web-front_"), "{text}");
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
    let posix = isoloom_catalogue::Catalogue::embedded()
        .expect("catalogue")
        .collections()
        .get("ansible.posix")
        .expect("posix pinned")
        .clone();
    // As `ansible-galaxy collection install -r` reads it: a collection by URL is `name: <url>`
    // with `type: url` (a `source:` would be read as a Galaxy server), and nothing else.
    let read: serde_yaml_ng::Value = serde_yaml_ng::from_str(&reqs).expect("YAML");
    let entries = read["collections"].as_sequence().expect("a list of collections");
    assert_eq!(entries.len(), 1, "{reqs}");
    let entry = entries[0].as_mapping().expect("a collection is a mapping");
    let keys: Vec<&str> = entry.keys().filter_map(serde_yaml_ng::Value::as_str).collect();
    assert_eq!(keys, ["name", "type"], "{reqs}");
    assert_eq!(entries[0]["name"].as_str(), Some(posix.url.to_string().as_str()), "{reqs}");
    assert_eq!(entries[0]["type"].as_str(), Some("url"), "{reqs}");
    // The pinned sha256, which galaxy doesn't check, is named for the user to check it.
    let sha = posix.sha256.to_string();
    assert_eq!(sha.len(), 64, "a sha256 is 64 hex chars: {sha}");
    let named = format!("# ansible.posix {}: sha256 {sha}", posix.version);
    assert!(reqs.lines().any(|l| l == named), "{reqs}");
}

#[test]
fn a_windows_packages_step_is_an_error_not_a_module_outside_the_builtin_ones() {
    // The lab's lowering refuses it on a host that already exists; a recipe that reaches the
    // renderer all the same (here, planned for another target) is a bug, said as one.
    let e = env("os: windows-server@2022, app: { steps: [{ packages: [Web-Server] }] }", Target::Vagrant, &[]);
    let c = Configured::of(&e, "m").unwrap();
    let err = role_files(&c, "r").unwrap_err();
    assert!(matches!(&err, crate::Error::WindowsPackages { machine } if machine == "m"), "{err:?}");
    assert_eq!(err.to_string(), "external: machine `m`: a `packages` step on Windows has no Ansible task here");
}

#[test]
fn the_specs_text_reaches_ansible_unsafe_so_jinja_in_it_is_never_evaluated() {
    let t = tasks(
        r#"os: debian@12, app: { steps: [
          { command: "echo {{ lookup('pipe', 'id') }}", creates: /x },
          { copy: { from: "site/{{ x }}.html", to: "/srv/{{ x }}.html" } },
          { file: { path: "/srv/{% raw %}", state: directory } },
          { service: { name: app, run: [/usr/bin/app, --greet, "{{ who }}"] } },
          { run: "setup/{{ y }}.sh" } ] }"#,
        Target::Vagrant,
        &[],
    );
    let tasks: Value = serde_yaml_ng::from_str(&t).expect("YAML");
    let tasks = tasks.as_sequence().expect("a task list");
    // A value as the file has it: `!unsafe` and its text, or plain.
    let tagged = |v: &Value| match v {
        Value::Tagged(tagged) if tagged.tag == "unsafe" => (true, tagged.value.as_str().unwrap_or_default().to_owned()),
        other => (false, other.as_str().unwrap_or_default().to_owned()),
    };
    let unsafe_ = |s: &str| (true, s.to_owned());
    let plain = |s: &str| (false, s.to_owned());
    let task = |module: &str, key: &str, value: &str| {
        tasks
            .iter()
            .find(|task| task.get(module).and_then(|m| m.get(key)).is_some_and(|v| tagged(v).1 == value))
            .unwrap_or_else(|| panic!("no {module} task with {key} {value}: {t}"))
    };
    let shell = task("ansible.builtin.shell", "cmd", "echo {{ lookup('pipe', 'id') }}");
    assert_eq!(tagged(&shell["ansible.builtin.shell"]["cmd"]), unsafe_("echo {{ lookup('pipe', 'id') }}"));
    let copy = task("ansible.builtin.copy", "dest", "/srv/{{ x }}.html");
    assert_eq!(tagged(&copy["ansible.builtin.copy"]["dest"]), unsafe_("/srv/{{ x }}.html"));
    // A project file is the play's folder joined to a task variable, which holds the spec's path.
    assert_eq!(
        tagged(&copy["ansible.builtin.copy"]["src"]),
        plain("{{ isoloom_project }}/{{ isoloom_machine_m_file }}")
    );
    assert_eq!(tagged(&copy["vars"]["isoloom_machine_m_file"]), unsafe_("site/{{ x }}.html"));
    let dir = task("ansible.builtin.file", "path", "/srv/{% raw %}");
    assert_eq!(tagged(&dir["ansible.builtin.file"]["path"]), unsafe_("/srv/{% raw %}"));
    let unit = tasks
        .iter()
        .filter_map(|each| each.get("ansible.builtin.copy")?.get("content"))
        .find(|c| tagged(c).1.contains("--greet"))
        .unwrap_or_else(|| panic!("no unit: {t}"));
    assert!(tagged(unit).0, "a unit holding Jinja is unsafe: {t}");
    let script = task("ansible.builtin.script", "cmd", "{{ isoloom_project }}/{{ isoloom_machine_m_file }}");
    assert_eq!(
        tagged(&script["ansible.builtin.script"]["cmd"]),
        plain("{{ isoloom_project }}/{{ isoloom_machine_m_file }}")
    );
    assert_eq!(tagged(&script["vars"]["isoloom_machine_m_file"]), unsafe_("setup/{{ y }}.sh"));
}

#[test]
fn each_play_climbs_from_its_folder_back_to_the_project() {
    assert_eq!(super::project_var(".isoloom/external"), "{{ (playbook_dir + '/../..') | realpath }}");
    assert_eq!(super::project_var("./.isoloom-2/external/"), "{{ (playbook_dir + '/../..') | realpath }}");
    assert_eq!(super::project_var("a/b/c"), "{{ (playbook_dir + '/../../..') | realpath }}");
}

/// An apt pin names the repository's host alone: a port or a user in its URL would pin an
/// origin apt never matches, so the priority would silently not apply.
#[test]
fn an_apt_pin_names_the_host_without_a_port_or_a_user() {
    let key = "key: { url: \"https://repo.example.com/key.asc\", sha256: 4c3f000000000000000000000000000000000000000000000000000000000000 }";
    for url in ["https://repo.example.com:8443/deb", "https://user@repo.example.com/deb"] {
        let m = format!("os: debian@12, app: {{ steps: [{{ repository: {{ name: ex, url: \"{url}\", components: [main], priority: 900, {key} }} }}] }}");
        let tasks: serde_yaml_ng::Value = serde_yaml_ng::from_str(&tasks(&m, Target::Vagrant, &[])).expect("YAML");
        let pins: Vec<&str> = tasks
            .as_sequence()
            .into_iter()
            .flatten()
            .filter_map(|t| t.get("ansible.builtin.copy")?.get("content")?.as_str())
            .filter(|c| c.contains("Pin:"))
            .collect();
        assert_eq!(pins, ["Package: *\nPin: origin repo.example.com\nPin-Priority: 900\n"], "{url}");
    }
}

/// A repository of a Rocky machine is its key, checked, then a `.repo` file naming it; an
/// Alpine machine's is its key under `/etc/apk/keys`, then a line of `/etc/apk/repositories`.
#[test]
fn a_repository_is_its_checked_key_then_the_package_managers_own_entry() {
    let key = "key: { url: \"https://repo.example.com/vendor.rsa.pub\", sha256: 4c3f000000000000000000000000000000000000000000000000000000000000 }";
    let sha = "sha256:4c3f000000000000000000000000000000000000000000000000000000000000";
    let rocky = tasks(
        &format!("os: rocky@9, app: {{ steps: [{{ repository: {{ name: vendor, url: \"https://repo.example.com/rpm\", priority: 10, {key} }} }}] }}"),
        Target::Vagrant,
        &[],
    );
    let rocky = untagged(&rocky);
    let download = &one(&rocky, "ansible.builtin.get_url")["ansible.builtin.get_url"];
    assert_eq!(text_at(download, "dest"), Some("/etc/pki/rpm-gpg/isoloom-vendor.asc"), "{rocky:?}");
    assert_eq!(text_at(download, "checksum"), Some(sha), "{rocky:?}");
    let repo = &one(&rocky, "ansible.builtin.yum_repository")["ansible.builtin.yum_repository"];
    for (field, value) in [
        ("name", "isoloom-vendor"),
        ("baseurl", "https://repo.example.com/rpm"),
        ("gpgkey", "file:///etc/pki/rpm-gpg/isoloom-vendor.asc"),
        ("priority", "10"),
    ] {
        assert_eq!(text_at(repo, field), Some(value), "{field}: {repo:?}");
    }
    assert_eq!(repo["gpgcheck"].as_bool(), Some(true), "{repo:?}");

    let alpine = tasks(
        &format!("os: alpine@3.24, app: {{ steps: [{{ repository: {{ name: vendor, url: \"https://repo.example.com/apk\", {key} }} }}] }}"),
        Target::Vagrant,
        &[],
    );
    let alpine = untagged(&alpine);
    let apk_key = &one(&alpine, "ansible.builtin.get_url")["ansible.builtin.get_url"];
    assert_eq!(text_at(apk_key, "dest"), Some("/etc/apk/keys/vendor.rsa.pub"), "{alpine:?}");
    let line = &one(&alpine, "ansible.builtin.lineinfile")["ansible.builtin.lineinfile"];
    assert_eq!(text_at(line, "path"), Some("/etc/apk/repositories"), "{line:?}");
    assert_eq!(text_at(line, "line"), Some("https://repo.example.com/apk"), "{line:?}");
}

/// Each state of a `file` step is the `file` module's (a folder, removed, a link) or a checked
/// download, with its owner and mode where it has them.
#[test]
fn a_file_step_is_the_file_module_or_a_checked_download() {
    let sha = "4c3f000000000000000000000000000000000000000000000000000000000000";
    let t = tasks(
        &format!(
            "os: debian@12, app: {{ steps: [{{ file: {{ path: /srv/d, state: directory, owner: www-data, mode: '0750' }} }}, {{ file: {{ path: /srv/old, state: absent }} }}, {{ file: {{ path: /srv/current, link: /srv/d }} }}, {{ file: {{ path: /srv/tool, url: 'https://dl.example.com/tool', sha256: {sha} }} }}] }}"
        ),
        Target::Vagrant,
        &[],
    );
    let tasks = untagged(&t);
    let files: Vec<&Value> = using(&tasks, "ansible.builtin.file")
        .into_iter()
        .map(|task| &task["ansible.builtin.file"])
        .collect();
    let fields = |f: &Value| -> Vec<(String, String)> {
        f.as_mapping()
            .into_iter()
            .flatten()
            .map(|(k, v)| {
                let v = v.as_str().map_or_else(|| v.as_bool().map(|b| b.to_string()).unwrap_or_default(), str::to_owned);
                (k.as_str().unwrap_or_default().to_owned(), v)
            })
            .collect()
    };
    let pairs = |p: &[(&str, &str)]| p.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect::<Vec<_>>();
    assert_eq!(
        files.iter().map(|f| fields(f)).collect::<Vec<_>>(),
        [
            pairs(&[("path", "/srv/d"), ("state", "directory"), ("owner", "www-data"), ("mode", "0750")]),
            pairs(&[("path", "/srv/old"), ("state", "absent")]),
            pairs(&[("path", "/srv/current"), ("src", "/srv/d"), ("state", "link"), ("force", "true")]),
        ],
        "{t}"
    );
    let download = &one(&tasks, "ansible.builtin.get_url")["ansible.builtin.get_url"];
    assert_eq!(
        fields(download),
        pairs(&[
            ("url", "https://dl.example.com/tool"),
            ("dest", "/srv/tool"),
            ("checksum", &format!("sha256:{sha}"))
        ]),
        "{t}"
    );
}

#[test]
fn a_group_step_is_the_group_module_with_its_gid() {
    let t = tasks(
        "os: debian@12, app: { steps: [{ group: { name: ops, gid: 1600, system: true } }] }",
        Target::Vagrant,
        &[],
    );
    let tasks = untagged(&t);
    let group = &one(&tasks, "ansible.builtin.group")["ansible.builtin.group"];
    assert_eq!(text_at(group, "name"), Some("ops"), "{t}");
    assert_eq!(group["gid"].as_u64(), Some(1600), "{t}");
    assert_eq!(group["system"].as_bool(), Some(true), "{t}");
}
