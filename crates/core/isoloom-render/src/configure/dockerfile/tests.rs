//! The Dockerfile: each step kind's instructions, from the OS's image, and what it builds with.

use isoloom_spec::model::Target;

use super::*;
use crate::testing;

/// The generated files of a machine `m` (YAML) on Docker, with the project's template texts.
fn built(m: &str, templates: &[(&str, &str)]) -> Vec<GeneratedFile> {
    files(&testing::machine(m, Target::Docker, templates))
}

/// The instructions of Dockerfile `d` in order, its comments and blank lines left out; a `FROM`
/// image's digest written `@sha256:…`.
fn instructions(d: &str) -> Vec<String> {
    d.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.split_once("@sha256:") {
            Some((image, _)) if l.starts_with("FROM ") => format!("{image}@sha256:…"),
            _ => l.to_owned(),
        })
        .collect()
}

/// The Dockerfile of a machine on `os` with `steps`.
fn dockerfile(os: &str, steps: &str) -> String {
    let found = built(&format!("os: {os}, app: {{ steps: {steps} }}"), &[]);
    found.into_iter().next().expect("a Dockerfile").contents
}

#[test]
fn steps_become_instructions_from_the_os_image() {
    let d = dockerfile(
        "debian@12",
        "[{ packages: [nginx] }, { copy: { from: site/, to: /var/www/, owner: www-data, mode: \"0755\" } }, { run: provision/web.sh }]",
    );
    assert!(
        d.starts_with(&format!(
            "{}# The container form of machine `m`: its OS's image and its steps.\nFROM ",
            crate::header("#")
        )),
        "{d}"
    );
    assert_eq!(
        instructions(&d),
        [
            "FROM debian:12@sha256:…",
            "WORKDIR /opt/isoloom",
            "COPY [\".isoloom/configure/lib/configure.sh\", \"/usr/local/lib/isoloom/configure.sh\"]",
            "RUN . /usr/local/lib/isoloom/configure.sh && isoloom_packages 'nginx'",
            "COPY --chown=www-data --chmod=0755 [\"site/\", \"/var/www/\"]",
            "COPY [\"provision/web.sh\", \"provision/web.sh\"]",
            "RUN sh 'provision/web.sh'",
        ],
        "nothing to run: no ENTRYPOINT, the target keeps it idle"
    );
}

#[test]
fn scripts_alone_need_no_library() {
    assert_eq!(
        instructions(&dockerfile("debian@12", "[{ run: a.sh, creates: /done }]")),
        [
            "FROM debian:12@sha256:…",
            "WORKDIR /opt/isoloom",
            "COPY [\"a.sh\", \"a.sh\"]",
            "RUN [ -e '/done' ] || sh 'a.sh'"
        ]
    );
}

#[test]
fn every_kind_with_a_container_form_is_a_library_call_or_a_copy() {
    let key = format!("key: {{ url: \"https://a.test/k\", sha256: {} }}", "0".repeat(64));
    let d = dockerfile(
        "rocky@9",
        &format!(
            "[{{ repository: {{ name: vendor, url: \"https://a.test/rpm\", {key} }} }}, {{ packages: [{{ name: curl, version: 7.76.1-31.el9 }}] }}, {{ group: {{ name: ops }} }}, {{ user: {{ name: alice, group: ops }} }}, {{ file: {{ path: /srv, state: directory }} }}, {{ command: [touch, /done], creates: /done }}, {{ firewall: {{ allow: [8443] }} }}]"
        ),
    );
    let functions: Vec<String> = instructions(&d)
        .iter()
        .filter_map(|l| l.strip_prefix("RUN . /usr/local/lib/isoloom/configure.sh && "))
        .map(|call| call.split_once(' ').map_or(call, |(f, _)| f).to_owned())
        .collect();
    assert_eq!(
        functions,
        [
            "isoloom_repository",
            "isoloom_packages",
            "isoloom_group",
            "isoloom_user",
            "isoloom_file",
            "isoloom_command"
        ],
        "a container has no host firewall: {d}"
    );
}

#[test]
fn a_template_is_copied_filled_in_with_its_owner_and_mode() {
    let found = built(
        "os: debian@12, app: { steps: [{ template: { from: conf/app.toml, to: /etc/app.toml, owner: app, group: ops } }] }",
        &[("conf/app.toml", "host = '{{ machine.name }}'\n")],
    );
    assert_eq!(
        instructions(&found[0].contents).last().map(String::as_str),
        Some("COPY --chown=app:ops --chmod=0644 [\".isoloom/configure/m/rendered/conf/app.toml\", \"/etc/app.toml\"]")
    );
    for (owner, chown) in [("owner: app", "--chown=app "), ("group: ops", "--chown=root:ops "), ("mode: \"0600\"", "")] {
        let one = built(
            &format!("os: debian@12, app: {{ steps: [{{ template: {{ from: conf/app.toml, to: /etc/app.toml, {owner} }} }}] }}"),
            &[("conf/app.toml", "x\n")],
        );
        let mode = if chown.is_empty() { "0600" } else { "0644" };
        assert_eq!(
            instructions(&one[0].contents).last().map(String::as_str),
            Some(format!("COPY {chown}--chmod={mode} [\".isoloom/configure/m/rendered/conf/app.toml\", \"/etc/app.toml\"]").as_str()),
            "{owner}"
        );
    }
    let rendered = found
        .iter()
        .find(|f| f.path == ".isoloom/configure/m/rendered/conf/app.toml")
        .expect("the filled template");
    assert_eq!(rendered.contents, "host = 'm'\n");
    let unread = built("os: debian@12, app: { steps: [{ template: { from: conf/app.toml, to: /etc/app.toml } }] }", &[]);
    assert_eq!(
        instructions(&unread[0].contents).last().map(String::as_str),
        Some(
            "RUN . /usr/local/lib/isoloom/configure.sh && isoloom_fail 'the template conf/app.toml wasn'\\''t in the project when the files were generated: add it, then generate again'"
        )
    );
    assert_eq!(unread.len(), 2, "the Dockerfile and the library, nothing filled in to copy: {unread:?}");
}

#[test]
fn a_container_that_runs_services_builds_its_supervisor_in_as_its_entrypoint() {
    let found = built(
        "os: debian@12, ports: { web: 80 }, app: { steps: [{ packages: [nginx] }, { service: nginx }] }",
        &[],
    );
    let paths: Vec<&str> = found.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            ".isoloom/docker/build/m/Dockerfile",
            ".isoloom/configure/lib/configure.sh",
            ".isoloom/configure/m/run"
        ]
    );
    assert_eq!(
        instructions(&found[0].contents)[3..],
        [
            "RUN . /usr/local/lib/isoloom/configure.sh && isoloom_packages 'nginx'",
            "COPY --chmod=0755 [\".isoloom/configure/m/run\", \"/usr/local/bin/isoloom-run\"]",
            "ENTRYPOINT [\"/usr/local/bin/isoloom-run\"]",
            "CMD []"
        ],
        "the supervisor starts the service, not the build"
    );
}

#[test]
fn each_container_built_from_steps_gets_its_dockerfile_and_a_vm_none() {
    let yaml = "version: 1\nname: d\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12, app: { steps: [{ packages: [curl] }] } }\n  img: { networks: { lan: 11 }, app: { image: nginx } }\n";
    let found = files(&testing::env(yaml, Target::Docker));
    let paths: Vec<&str> = found.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, [".isoloom/docker/build/web/Dockerfile", ".isoloom/configure/lib/configure.sh"]);
    assert_eq!(instructions(&found[0].contents)[0], "FROM debian:12@sha256:…");
    assert_eq!(files(&testing::env(yaml, Target::Vagrant)), [], "a VM sets itself up");
}

#[test]
fn a_bare_official_image_keeps_its_own_working_folder() {
    // A catalogue entry that is only an official image, with no install or configure steps and
    // the image's own entrypoint, must not have its working folder overridden: an app may rely on
    // it to find its files relative to its image's WORKDIR.
    let d = built("os: debian@12, service: mongodb", &[]).into_iter().next().expect("a Dockerfile").contents;
    assert_eq!(
        instructions(&d),
        ["FROM docker.io/library/mongo:7@sha256:…"],
        "no WORKDIR, and the image's own entrypoint runs"
    );
}

#[test]
fn each_family_installs_with_its_library_call() {
    for os in ["alpine@3.19", "rocky@9", "centos@7", "ubuntu@24.04"] {
        let d = dockerfile(os, "[{ packages: [curl] }]");
        assert_eq!(
            instructions(&d).last().map(String::as_str),
            Some("RUN . /usr/local/lib/isoloom/configure.sh && isoloom_packages 'curl'"),
            "{os}"
        );
    }
}
