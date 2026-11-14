//! Isoloom's own Vagrant VMs (the router, the tool VM, the controller, the containers' host) are
//! named after their `isoloom-` define names everywhere an environment machine's name also goes: their
//! display names, their checks' step ids and runner files. An environment machine may be called
//! `controller`, `router`, `docker` or `containers` and still stand beside them.

use std::collections::BTreeSet;

use isoloom_spec::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, file, valid};

/// Environment machines named as Isoloom's own VMs once were, with a router (`reach`), the `shell` tool,
/// and a controller (playbooks, and checks from the networks).
const LAB: &str = "
version: 1
name: names
networks:
  front: { cidr: 10.61.10.0/24 }
  back: { cidr: 10.61.20.0/24 }
reach:
  - { from: front, to: back }
tools: { shell: {} }
machines:
  controller: { os: debian@12, networks: { front: 10 }, ports: { web: 80 } }
  router: { os: debian@12, networks: { back: 10 }, ports: { web: 80 } }
  docker: { os: debian@12, networks: { front: 11 } }
  containers: { os: debian@12, networks: { back: 11 } }
provision:
  - ansible: provision/site.yml
checks:
  - { from: controller, tcp: \"router:80\" }
  - { tcp: \"controller:80\" }
";

/// The names `target`'s Vagrantfile gives its VMs, by the provider setting that takes each
/// (`v.name`: VirtualBox and Parallels; `v.vmname`: Hyper-V; `v.guest_name`: ESXi), the copy's key
/// still to read (`#{ISOLOOM_KEY}`).
fn vm_names(files: &[isoloom_render::GeneratedFile], target: &str) -> Outcome<Vec<(&'static str, String)>> {
    let vagrantfile = file(files, &format!("{target}/Vagrantfile"))?;
    let mut names = Vec::new();
    for line in vagrantfile.lines().map(str::trim) {
        for setting in ["v.name", "v.vmname", "v.guest_name"] {
            if let Some(name) = line
                .strip_prefix(setting)
                .and_then(|r| r.strip_prefix(" = \""))
                .and_then(|r| r.strip_suffix('"'))
            {
                names.push((setting, name.to_owned()));
            }
        }
    }
    Ok(names)
}

/// Whether `name` is a VM's name under some setting among `names`.
fn named(names: &[(&'static str, String)], name: &str) -> bool {
    names.iter().any(|(_, n)| n == name)
}

#[test]
fn a_lab_machine_named_as_one_of_isoloom_s_own_vms_stands_beside_it_on_vagrant() -> Outcome {
    let lab = valid(LAB)?;
    let files = generate(&lab, Target::Vagrant)?;
    let names = vm_names(&files, "vagrant")?;
    let hyperv: Vec<_> = names.iter().filter(|(s, _)| *s == "v.vmname").collect();
    let distinct: BTreeSet<_> = hyperv.iter().collect();
    assert_eq!(distinct.len(), hyperv.len(), "Hyper-V shows each VM under its own name: {names:?}");
    for own in ["isoloom-router", "isoloom-tool-shell", "isoloom-controller"] {
        assert!(named(&names, &format!("names-#{{ISOLOOM_KEY}} - {own}")), "{own}: {names:?}");
        assert!(named(&names, &format!("names-#{{ISOLOOM_KEY}}-{own}")), "{own}: {names:?}");
    }
    let manifest = isoloom_manifest::parse(file(&files, "vagrant/run.json")?)?;
    let runners: Vec<&str> = manifest.checks.runners.iter().map(|r| r.step.id.as_str()).collect();
    assert!(
        runners.contains(&"check-controller") && runners.contains(&"check-isoloom-controller"),
        "{runners:?}"
    );
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    for runner in ["vagrant/checks/controller.sh", "vagrant/checks/isoloom-controller.sh"] {
        assert!(paths.iter().any(|p| p.ends_with(runner)), "{runner}: {paths:?}");
    }
    Ok(())
}
