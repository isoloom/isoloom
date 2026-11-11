//! Isoloom's own Vagrant VMs (the router, the tool VM, the controller, the containers' host) are
//! named after their `isoloom-` define names everywhere an environment machine's name also goes: their
//! display names, their checks' step ids and runner files. An environment machine may be called
//! `controller`, `router`, `docker` or `containers` and still stand beside them.

use std::collections::BTreeSet;

use isoloom_manifest::{Native, Provider};
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

/// The same names on `hybrid`: `controller` a Linux VM (its TPM), `docker` a Windows one,
/// `containers` a container beside them.
const HYBRID: &str = "
version: 1
name: names
networks:
  front: { cidr: 10.61.10.0/24 }
  back: { cidr: 10.61.20.0/24 }
reach:
  - { from: front, to: back }
machines:
  controller: { os: debian@12, hardware: { tpm: required }, networks: { front: 10 }, ports: { web: 80 } }
  docker: { os: windows-server@2022, networks: { front: 11 } }
  containers: { networks: { back: 11 }, app: { image: \"nginx:1.27\" }, ports: { web: 80 } }
provision:
  - ansible: provision/site.yml
checks:
  - { from: controller, tcp: \"containers:80\" }
  - { tcp: \"containers:80\" }
";

/// Every VM name the run manifest of `target` gives, with the provider that shows it.
fn vm_names(files: &[isoloom_render::GeneratedFile], target: &str) -> Outcome<Vec<(Provider, String)>> {
    let manifest = isoloom_manifest::parse(file(files, &format!("{target}/run.json"))?)?;
    let mut names = Vec::new();
    for m in &manifest.machines {
        if let Some(Native::Vagrant { vm_names, .. }) = &m.native {
            names.extend(vm_names.iter().map(|(p, n)| (*p, n.clone())));
        }
    }
    Ok(names)
}

/// Whether `name` is a VM's name under some provider among `names`.
fn named(names: &[(Provider, String)], name: &str) -> bool {
    names.iter().any(|(_, n)| n == name)
}

#[test]
fn a_lab_machine_named_as_one_of_isoloom_s_own_vms_stands_beside_it_on_vagrant() -> Outcome {
    let lab = valid(LAB)?;
    let files = generate(&lab, Target::Vagrant)?;
    let names = vm_names(&files, "vagrant")?;
    let distinct: BTreeSet<_> = names.iter().collect();
    assert_eq!(distinct.len(), names.len(), "a provider shows each VM under its own name: {names:?}");
    for own in ["isoloom-router", "isoloom-tool-shell", "isoloom-controller"] {
        assert!(named(&names, &format!("names - {own}")), "{own}: {names:?}");
        assert!(named(&names, &format!("names-{own}")), "{own}: {names:?}");
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

#[test]
fn a_lab_machine_named_as_one_of_isoloom_s_own_vms_stands_beside_it_on_hybrid() -> Outcome {
    let lab = valid(HYBRID)?;
    let files = generate(&lab, Target::Hybrid)?;
    let vagrantfile = file(&files, "hybrid/Vagrantfile")?;
    assert!(vagrantfile.contains("v.name = \"names - isoloom-docker-host\""), "{vagrantfile}");
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.iter().any(|p| p.ends_with("hybrid/checks/controller.sh")), "{paths:?}");
    assert!(paths.iter().any(|p| p.ends_with("hybrid/checks/isoloom-controller.sh")), "{paths:?}");
    Ok(())
}

#[test]
fn docker_vms_vm_is_named_after_the_containers_host() -> Outcome {
    let lab = valid(
        "version: 1
name: names
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  docker: { networks: { lan: 10 }, app: { image: \"nginx:1.27\" } }
",
    )?;
    let files = generate(&lab, Target::DockerVm)?;
    let names = vm_names(&files, "docker-vm")?;
    assert!(named(&names, "names - isoloom-docker-host"), "{names:?}");
    assert!(named(&names, "names-isoloom-docker-host"), "{names:?}");
    Ok(())
}
