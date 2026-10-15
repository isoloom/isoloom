//! The tier list against what the code says of the targets: each Vagrant provider exists on the
//! host OSes the `vagrant` run manifest gives it, `docker-vm`'s Vagrant providers on the same, and
//! every entry names the providers and clouds each target really has.

use std::collections::BTreeSet;

use isoloom_spec::model::Target;
use isoloom_targets::generate;
use isoloom_targets::tiers::{self, os_id};
use serde_json::Value;

use crate::testing::{Outcome, example, file};

/// The host OSes of an entry, by name.
fn exists_on(entries: &[tiers::TierEntry], id: &str) -> Outcome<BTreeSet<String>> {
    let entry = entries.iter().find(|e| e.id() == id).ok_or_else(|| format!("no tier entry `{id}`"))?;
    Ok(entry.exists_on.iter().map(|o| os_id(*o).to_owned()).collect())
}

#[test]
fn a_vagrant_provider_exists_where_the_run_manifest_says_and_so_does_docker_vm_on_it() -> Outcome {
    let entries = tiers::tiers()?;
    let files = generate(&example("hello-stack")?, Target::Vagrant)?;
    let manifest: Value = serde_json::from_str(file(&files, "vagrant/run.json")?)?;
    let providers = manifest["identity"]["vagrant"]["providers"].as_array().ok_or("no providers")?;
    assert_eq!(providers.len(), tiers::variants(Target::Vagrant).len());
    for p in providers {
        let id = p["id"].as_str().ok_or("a provider with no id")?;
        let hosts: BTreeSet<String> = p["hosts"]
            .as_array()
            .ok_or("no hosts")?
            .iter()
            .filter_map(|h| h.as_str().map(str::to_owned))
            .collect();
        assert_eq!(exists_on(&entries, &format!("vagrant/{id}"))?, hosts, "vagrant/{id}");
        if tiers::variants(Target::DockerVm).contains(&id) {
            assert_eq!(exists_on(&entries, &format!("docker-vm/{id}"))?, hosts, "docker-vm/{id}");
        }
    }
    Ok(())
}

#[test]
fn docker_vm_has_an_entry_for_each_provider_its_vagrantfile_has_a_block_for() -> Outcome {
    let files = generate(&example("hello-stack")?, Target::DockerVm)?;
    let vagrantfile = file(&files, "docker-vm/Vagrantfile")?;
    let blocks: Vec<&str> = tiers::variants(Target::DockerVm).into_iter().filter(|p| *p != "proxmox").collect();
    for p in &blocks {
        assert!(vagrantfile.contains(&format!("config.vm.provider \"{p}\"")), "no `{p}` block");
    }
    assert!(!vagrantfile.contains("config.vm.provider \"hyperv\""));
    assert!(files.iter().any(|f| f.path.ends_with("docker-vm/proxmox/main.tf")));
    Ok(())
}
