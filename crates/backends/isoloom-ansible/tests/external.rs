//! The `external` target as its users call it: an environment planned from a spec, rendered by
//! [`ExternalBackend`], its files read back (the inventory as Ansible reads it, the playbook and
//! roles as YAML, a check runner per machine with checks).

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use isoloom_ansible::ExternalBackend;
use isoloom_ansible::configure::role_name;
use isoloom_plan::{Environment, Lab};
use isoloom_render::GeneratedFile;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_test_support::{yaml, yaml_documents};

/// A test's outcome: a failed step is its report.
type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// The `external` plan of example `name`.
fn example(name: &str) -> Outcome<Environment> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name);
    let lab = Lab::new(isoloom_spec::load(&dir)?);
    Ok(lab.plan(&lab.for_target(Target::External), Target::External, None)?)
}

/// The `external` plan of the spec `yaml`.
fn spec(yaml: &str) -> Outcome<Environment> {
    let lab = Lab::new(isoloom_spec::parse(yaml)?);
    Ok(lab.plan(&lab.for_target(Target::External), Target::External, None)?)
}

/// The rendered file at `path`.
fn file<'f>(files: &'f [GeneratedFile], path: &str) -> Outcome<&'f str> {
    Ok(&files.iter().find(|f| f.path == path).ok_or_else(|| format!("no {path}"))?.contents)
}

/// A host of an inventory group, with its variables.
type Host = (String, BTreeMap<String, String>);

/// An inventory as Ansible reads it: each group's hosts, each with its variables.
fn inventory(text: &str) -> BTreeMap<String, Vec<Host>> {
    let host = |line: &String| {
        let mut words = line.split_whitespace();
        let name = words.next().unwrap_or_default().to_owned();
        (
            name,
            words.filter_map(|w| w.split_once('=')).map(|(k, v)| (k.to_owned(), v.to_owned())).collect(),
        )
    };
    isoloom_test_support::ini_groups(text)
        .into_iter()
        .map(|(group, lines)| (group, lines.iter().map(host).collect()))
        .collect()
}

#[test]
fn the_files_are_the_inventory_the_playbook_a_role_per_configured_machine_and_the_checks() -> Outcome {
    let files = ExternalBackend.render(&example("existing-hosts")?)?;
    let mut paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        [
            ".isoloom/external/checks/cache.sh",
            ".isoloom/external/checks/web.sh",
            ".isoloom/external/inventory.ini",
            ".isoloom/external/roles/isoloom_machine_cache/tasks/main.yml",
            ".isoloom/external/roles/isoloom_machine_web/tasks/main.yml",
            ".isoloom/external/site.yml",
        ]
    );
    Ok(())
}

#[test]
fn each_host_is_reached_as_its_external_override_says() -> Outcome {
    let files = ExternalBackend.render(&example("existing-hosts")?)?;
    let groups = inventory(file(&files, ".isoloom/external/inventory.ini")?);
    let linux = groups.get("linux").ok_or("no linux group")?;
    let hosts: Vec<&str> = linux.iter().map(|(h, _)| h.as_str()).collect();
    assert_eq!(hosts, ["web", "cache"]);
    for (host, address) in [("web", "192.168.1.20"), ("cache", "192.168.1.21")] {
        let vars = &linux.iter().find(|(h, _)| h == host).ok_or(host)?.1;
        assert_eq!(vars.get("ansible_host").map(String::as_str), Some(address), "{host}");
        assert_eq!(vars.get("ansible_user").map(String::as_str), Some("admin"), "{host}");
        assert_eq!(
            vars.get("ansible_ssh_private_key_file").map(String::as_str),
            Some("~/.ssh/id_ed25519"),
            "{host}"
        );
    }
    assert_eq!(groups.get("windows").map(Vec::len), Some(0));
    Ok(())
}

#[test]
fn the_playbook_configures_each_machine_through_its_own_role() -> Outcome {
    let files = ExternalBackend.render(&example("existing-hosts")?)?;
    let plays = yaml(file(&files, ".isoloom/external/site.yml")?);
    let plays = plays.as_sequence().ok_or("site.yml isn't a list of plays")?;
    let configured: Vec<(String, Vec<String>)> = plays
        .iter()
        .map(|p| {
            (
                p["hosts"].as_str().unwrap_or_default().to_owned(),
                p["roles"]
                    .as_sequence()
                    .into_iter()
                    .flatten()
                    .filter_map(|r| r.as_str().map(str::to_owned))
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        configured,
        [("web".to_owned(), vec![role_name("web")]), ("cache".to_owned(), vec![role_name("cache")])]
    );
    for play in plays {
        assert_eq!(play["become"].as_bool(), Some(true), "{play:?}");
    }
    // Each role is a list of tasks, every one named.
    for machine in ["web", "cache"] {
        let text = file(&files, &format!(".isoloom/external/roles/{}/tasks/main.yml", role_name(machine)))?;
        let docs = yaml_documents(text);
        let tasks = docs.first().and_then(|d| d.as_sequence()).ok_or("a role's tasks aren't a list")?;
        assert!(!tasks.is_empty(), "{machine}");
        assert!(tasks.iter().all(|t| t["name"].as_str().is_some_and(|n| !n.is_empty())), "{machine}: {tasks:?}");
    }
    Ok(())
}

#[test]
fn a_role_is_named_after_its_machine_in_ansibles_terms() {
    assert_eq!(role_name("web"), "isoloom_machine_web");
    assert_eq!(role_name("web-1"), "isoloom_machine_web_1");
}

#[test]
fn a_machine_with_nothing_to_configure_is_in_the_inventory_alone() -> Outcome {
    let env = spec(
        "version: 1\nname: home\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 5 }, os: debian@12 }\noverrides:\n  external: { machines: { web: { ssh: { address: 192.168.1.20, port: 2222 } } } }\n",
    )?;
    let files = ExternalBackend.render(&env)?;
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, [".isoloom/external/inventory.ini"]);
    let groups = inventory(file(&files, ".isoloom/external/inventory.ini")?);
    let web = groups.get("linux").and_then(|l| l.first()).ok_or("web isn't a linux host")?;
    assert_eq!(web.1.get("ansible_port").map(String::as_str), Some("2222"));
    Ok(())
}

#[test]
fn a_check_with_nowhere_to_run_is_refused_before_anything_is_written() -> Outcome {
    // A check with no `from` runs on an access machine, which an external lab doesn't have.
    let env = spec(
        "version: 1\nname: home\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 5 }, os: debian@12, ports: { web: 80 } }\nchecks:\n  - { name: up, http: 'http://web/' }\noverrides:\n  external: { machines: { web: { ssh: { address: 192.168.1.20 } } } }\n",
    )?;
    let why = ExternalBackend.refusal(&env).ok_or("not refused")?;
    assert!(ExternalBackend.render(&env).is_err(), "{why}");
    Ok(())
}

#[test]
fn every_check_runner_is_a_shell_script_with_the_machines_checks() -> Outcome {
    let files = ExternalBackend.render(&example("existing-hosts")?)?;
    let cache = file(&files, ".isoloom/external/checks/cache.sh")?;
    assert!(cache.starts_with("#!/bin/sh"), "{cache}");
    let mentioned: Vec<&str> = cache.lines().filter(|l| l.contains("transparent_hugepage")).collect();
    assert!(!mentioned.is_empty(), "the cache's own check runs there: {cache}");
    Ok(())
}
