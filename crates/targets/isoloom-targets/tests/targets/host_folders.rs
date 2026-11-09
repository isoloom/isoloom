//! `host_folders`: a folder of the computer that runs the lab, mounted in a machine both ways.
//! The targets whose machines run on that computer mount it (`docker`, `docker-vm` on a local
//! provider, `vagrant` on a provider with a two-way shared folder); every other one refuses the
//! spec, naming the field.

use isoloom_spec::Target;
use isoloom_targets::{Lab, generate, refusal, refusal_on};

use crate::testing::{Outcome, file, valid};

/// A desk with both forms (the OS's image idle, a fresh VM) mounting three folders of the
/// computer: one of the project, one of the home folder (read-only), one absolute.
const DESK: &str = "version: 1
name: shared-desk
networks: { lan: { cidr: 10.30.0.0/24 } }
machines:
  desk:
    os: debian@12
    networks: { lan: 10 }
    host_folders:
      - { from: shared, path: /srv/shared }
      - { from: '~/notes', path: /home/notes, read_only: true }
      - { from: /srv/drop, path: /srv/drop }
";

fn desk() -> Outcome<Lab> {
    valid(DESK)
}

#[test]
fn only_the_targets_whose_machines_run_on_that_computer_take_it() -> Outcome {
    let lab = desk()?;
    for t in [Target::Docker, Target::DockerVm, Target::Vagrant] {
        assert_eq!(refusal(&lab, t), None, "{}", t.id());
    }
    for t in [
        Target::Hosted,
        Target::CloudDocker,
        Target::Kubernetes,
        Target::Proxmox,
        Target::Esxi,
        Target::CloudVm,
    ] {
        let why = refusal(&lab, t).ok_or_else(|| format!("{} takes it", t.id()))?;
        assert!(
            why.contains("machine `desk` mounts a folder of the computer that runs the lab (`host_folders`)"),
            "{}: {why}",
            t.id()
        );
    }
    Ok(())
}

#[test]
fn an_existing_host_and_a_hybrid_lab_refuse_it_too() -> Outcome {
    let external = valid(&format!(
        "{DESK}overrides: {{ external: {{ machines: {{ desk: {{ ssh: {{ address: 192.0.2.10 }} }} }} }} }}\n"
    ))?;
    let existing = refusal(&external, Target::External).ok_or("external takes it")?;
    assert!(existing.contains("`host_folders`"), "{existing}");
    let hybrid = valid(&format!(
        "{DESK}  web: {{ networks: {{ lan: 11 }}, app: {{ image: nginx }} }}\n  box: {{ os: debian@12, networks: {{ lan: 12 }}, ports: {{ web: 80 }}, app: {{ steps: [{{ run: b.sh }}] }} }}\n"
    ))?;
    let why = refusal(&hybrid, Target::Hybrid).ok_or("hybrid takes it")?;
    assert!(why.contains("`host_folders`"), "{why}");
    Ok(())
}

#[test]
fn vagrant_refuses_it_on_the_providers_with_no_two_way_shared_folder() -> Outcome {
    let lab = desk()?;
    for p in ["virtualbox", "vmware_desktop", "parallels", "libvirt"] {
        assert_eq!(refusal_on(&lab, Target::Vagrant, Some(p)), None, "{p}");
    }
    let hyperv = refusal_on(&lab, Target::Vagrant, Some("hyperv")).ok_or("Hyper-V takes it")?;
    assert!(hyperv.contains("`host_folders`") && hyperv.contains("SMB"), "{hyperv}");
    let esxi = refusal_on(&lab, Target::Vagrant, Some("vmware_esxi")).ok_or("ESXi takes it")?;
    assert!(esxi.contains("run on the ESXi host"), "{esxi}");
    let qemu = refusal_on(&lab, Target::Vagrant, Some("qemu")).ok_or("QEMU takes a read-only one")?;
    assert!(qemu.contains("read-only (`read_only`)"), "{qemu}");
    let read_write = valid(&DESK.replace(", read_only: true", ""))?;
    assert_eq!(refusal_on(&read_write, Target::Vagrant, Some("qemu")), None);
    Ok(())
}

#[test]
fn docker_vm_refuses_it_on_esxi_and_proxmox_and_writes_no_proxmox_module() -> Outcome {
    let lab = desk()?;
    for p in ["virtualbox", "vmware_desktop", "parallels", "utm", "qemu", "libvirt"] {
        assert_eq!(refusal_on(&lab, Target::DockerVm, Some(p)), None, "{p}");
    }
    for p in ["vmware_esxi", "proxmox"] {
        let why = refusal_on(&lab, Target::DockerVm, Some(p)).ok_or_else(|| format!("{p} takes it"))?;
        assert!(
            why.contains("machine `desk` mounts a folder of the computer that runs the lab (`host_folders`)"),
            "{why}"
        );
    }
    let files = generate(&lab, Target::DockerVm)?;
    assert!(!files.iter().any(|f| f.path.contains("docker-vm/proxmox/")), "no Proxmox module or manifest");
    let manifest: serde_json::Value = serde_json::from_str(file(&files, "docker-vm/run.json")?)?;
    let providers = manifest
        .pointer("/identity/vagrant/providers")
        .and_then(|p| p.as_array())
        .ok_or("no providers")?;
    let refused: Vec<&str> = providers
        .iter()
        .filter(|p| p.get("refusal").is_some_and(|r| !r.is_null()))
        .filter_map(|p| p.get("id").and_then(|id| id.as_str()))
        .collect();
    assert_eq!(refused, ["vmware_esxi"]);
    Ok(())
}

#[test]
fn compose_in_the_vm_binds_each_folder_from_where_the_vm_shares_it() -> Outcome {
    let files = generate(&desk()?, Target::DockerVm)?;
    let manifest = file(&files, "docker-vm/run.json")?;
    assert!(
        manifest.contains(
            "ISOLOOM_HOST_FOLDER_DESK_0=/mnt/isoloom/desk/0 ISOLOOM_HOST_FOLDER_DESK_1=/mnt/isoloom/desk/1 ISOLOOM_HOST_FOLDER_DESK_2=/mnt/isoloom/desk/2 docker compose"
        ),
        "{manifest}"
    );
    let compose = file(&files, "docker/compose.yml")?;
    assert!(compose.contains("source: ${ISOLOOM_HOST_FOLDER_DESK_0:-../../shared}"), "{compose}");
    let vagrantfile = file(&files, "docker-vm/Vagrantfile")?;
    assert!(
        vagrantfile.contains("o.vm.synced_folder File.expand_path(\"~/notes\", ROOT), \"/mnt/isoloom/desk/1\", type: \"qemu_virtiofs\", create: true\n"),
        "read-write in the VM, read-only in its container:\n{vagrantfile}"
    );
    Ok(())
}

#[test]
fn a_spec_without_host_folders_keeps_docker_vms_proxmox_module() -> Outcome {
    let lab = valid(DESK.split("    host_folders:").next().unwrap_or_default())?;
    let files = generate(&lab, Target::DockerVm)?;
    assert!(files.iter().any(|f| f.path.ends_with("docker-vm/proxmox/main.tf")));
    assert!(!file(&files, "docker-vm/Vagrantfile")?.contains("synced_folder File.expand_path"));
    Ok(())
}
