//! The conformance lab: one machine of a Linux family whose recipe takes every step kind a test
//! container can hold, its project files, and what each renderer makes of it (the VM's set-up
//! steps, the container's Dockerfile and its files), rendered by Isoloom's own crates.

use std::collections::BTreeMap;
use std::path::Path;

use isoloom_plan::lifecycle::Step;
use isoloom_plan::{Environment, Lab};
use isoloom_render::GeneratedFile;
use isoloom_render::configure::dockerfile;
use isoloom_render::steps;
use isoloom_spec::model::Target;

use crate::Res;

/// A Linux family the tests run in: its OS, the package manager its repository is for, and
/// that repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Family {
    /// How the probe and the table name it.
    pub(super) id: &'static str,
    /// The machine's `os`.
    pub(super) os: &'static str,
    /// The repository's kind, as the probe names it.
    pub(super) kind: &'static str,
    /// The repository step (nginx.org's, for the family).
    repository: &'static str,
}

/// nginx.org's signing key, pinned (the catalogue's nginx entry pins the same).
const GPG_KEY: &str = "key: { url: \"https://nginx.org/keys/nginx_signing.key\", sha256: 55385da31d198fa6a5012d40ae98ecb272a6c4e8fffffba94719ffd3e87de37a }";

/// The families, each in its pinned official image.
pub(super) const FAMILIES: [Family; 4] = [
    Family {
        id: "debian-12",
        os: "debian@12",
        kind: "apt",
        repository: "{ name: nginx, url: \"https://nginx.org/packages/debian\", components: [nginx], KEY }",
    },
    Family {
        id: "ubuntu-24.04",
        os: "ubuntu@24.04",
        kind: "apt",
        repository: "{ name: nginx, url: \"https://nginx.org/packages/ubuntu\", components: [nginx], KEY }",
    },
    Family {
        id: "rocky-9",
        os: "rocky@9",
        kind: "rpm",
        repository: "{ name: nginx, url: \"https://nginx.org/packages/centos/9/$basearch/\", KEY }",
    },
    Family {
        id: "alpine-3.24",
        os: "alpine@3.24",
        kind: "apk",
        repository: "{ name: nginx, url: \"https://nginx.org/packages/alpine/v3.24/main\", key: { url: \"https://nginx.org/keys/nginx_signing.rsa.pub\", sha256: ff7cf138acc09f2a5029300ab713fe6a1440605fca72e2bab76a4da9206fec87 } }",
    },
];

/// The step kinds the recipe takes, as the table lists them. `firewall` has no state in a
/// container (no firewall runs there): the second apply's silence proves it. `service` needs an
/// init system: the VM runs prove it.
pub(super) const KINDS: [&str; 9] = ["repository", "packages", "group", "user", "file", "copy", "template", "command", "firewall"];

/// The recipe's steps, in the order they apply.
const STEPS: &str = r#"
        - repository: REPOSITORY
        - packages: [curl]
        - group: { name: isoloom-ops, gid: 1600 }
        - user: { name: isoloom-alice, uid: 1600, group: isoloom-ops, groups: [isoloom-ops], shell: /bin/sh, password: conformance }
        - file: { path: /srv/conformance/dir, state: directory, owner: isoloom-alice, mode: "0750" }
        - file: { path: /srv/conformance/link, link: /srv/conformance/dir }
        - file: { path: /srv/conformance/download, url: "https://www.apache.org/licenses/LICENSE-2.0.txt", sha256: cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30 }
        - command: [sh, -c, "touch /srv/conformance/made /srv/conformance/gone"]
          creates: /srv/conformance/made
        - file: { path: /srv/conformance/gone, state: absent }
        - copy: { from: files/one.conf, to: /srv/conformance/one.conf, owner: isoloom-alice, mode: "0640" }
        - copy: { from: files/tree/, to: /srv/conformance/tree/ }
        - template: { from: files/app.conf, to: /srv/conformance/app.conf, group: isoloom-ops, mode: "0640" }
        - command: [sh, -c, "echo ran >> /srv/conformance/unless.log"]
          unless: [test, -s, /srv/conformance/unless.log]
        - firewall: { allow: [8080] }
"#;

/// The project's files the steps read.
const FILES: [(&str, &str); 4] = [
    ("files/one.conf", "one\n"),
    ("files/tree/a.txt", "a\n"),
    ("files/tree/sub/b.txt", "b\n"),
    ("files/app.conf", "name = {{ machine.name }}\n"),
];

/// The lab of one machine `m` of `family`, its templates read.
fn lab(family: Family) -> Res<Lab> {
    let steps = STEPS.replace("REPOSITORY", &family.repository.replace("KEY", GPG_KEY));
    let yaml = format!(
        "version: 1\nname: conformance\nnetworks: {{ lab: {{ cidr: 10.99.0.0/24 }} }}\nmachines:\n  m:\n    os: {}\n    networks: {{ lab: 10 }}\n    app:\n      steps:{steps}",
        family.os
    );
    let lab = Lab::parse(&yaml).map_err(|p| format!("the conformance lab of {}: {p:?}", family.id))?;
    let problems = lab.problems();
    if !problems.is_empty() {
        return Err(format!("the conformance lab of {}: {problems:?}", family.id).into());
    }
    let texts: BTreeMap<_, _> = FILES
        .iter()
        .filter(|(path, _)| path.ends_with("app.conf"))
        .filter_map(|(path, text)| Some((path.parse().ok()?, (*text).to_owned())))
        .collect();
    Ok(lab.with_templates(texts))
}

/// The project folder: its files, under `dir`.
pub(super) fn project(dir: &Path) -> Res<()> {
    for (path, text) in FILES {
        write(dir, path, text)?;
    }
    Ok(())
}

/// `text` at `path` under `dir`, its folders made.
pub(super) fn write(dir: &Path, path: &str, text: &str) -> Res<()> {
    let at = dir.join(path);
    if let Some(parent) = at.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(at, text)?;
    Ok(())
}

/// The shell rendering: the machine's set-up steps that run its recipe on a VM, as one script.
pub(super) fn vm_script(family: Family) -> Res<String> {
    let lab = lab(family)?;
    let env = Environment::build(&lab.for_target(Target::Vagrant), Target::Vagrant, None);
    let lifecycle = env.machines.get("m").map(|m| m.lifecycle.clone()).unwrap_or_default();
    let script: String = lifecycle
        .iter()
        .filter(|s| matches!(s, Step::Configure { .. }))
        .filter_map(|s| steps::linux(&env, "m", s))
        .collect();
    if script.is_empty() {
        return Err(format!("{}: the VM's set-up has no configuration step", family.id).into());
    }
    Ok(format!("#!/bin/sh\nset -eu\n{script}"))
}

/// The Dockerfile rendering: the container's Dockerfile and the files it builds with.
pub(super) fn container_files(family: Family) -> Res<Vec<GeneratedFile>> {
    let lab = lab(family)?;
    let env = Environment::build(&lab.for_target(Target::Docker), Target::Docker, None);
    let files = dockerfile::files(&env);
    if files.is_empty() {
        return Err(format!("{}: no Dockerfile was generated", family.id).into());
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_family_renders_its_recipe_both_ways() {
        for family in FAMILIES {
            let script = vm_script(family).unwrap();
            assert!(
                script.contains("isoloom_repository") && script.contains("isoloom_firewall '8080/tcp'"),
                "{}",
                family.id
            );
            let files = container_files(family).unwrap();
            assert!(files[0].contents.contains("isoloom_repository"), "{}", family.id);
            assert!(files.iter().any(|f| f.path.ends_with("rendered/files/app.conf")), "{}", family.id);
        }
    }
}
