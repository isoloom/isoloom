//! `host_folders`: a folder of the computer that runs the environment, mounted in a machine both ways.
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
    for t in [Target::Docker, Target::Vagrant] {
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
            why.contains("machine `desk` mounts a folder of the computer that runs the environment (`host_folders`)"),
            "{}: {why}",
            t.id()
        );
    }
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
fn vagrant_refuses_qemu_for_any_vm_with_a_read_only_folder_not_only_the_first() -> Outcome {
    let lab = valid(
        "version: 1
name: two-desks
networks: { lan: { cidr: 10.30.0.0/24 } }
machines:
  a: { os: debian@12, networks: { lan: 10 }, host_folders: [{ from: shared, path: /srv/shared }] }
  b: { os: debian@12, networks: { lan: 11 }, host_folders: [{ from: notes, path: /notes, read_only: true }] }
",
    )?;
    let qemu = refusal_on(&lab, Target::Vagrant, Some("qemu")).ok_or("QEMU takes b's read-only folder")?;
    assert!(qemu.contains("machine `b`") && qemu.contains("read-only (`read_only`)"), "{qemu}");
    assert_eq!(refusal_on(&lab, Target::Vagrant, Some("virtualbox")), None);
    Ok(())
}

#[test]
fn vagrants_vagrantfile_stops_up_on_a_provider_it_does_not_know() -> Outcome {
    let vagrantfile = generate(&desk()?, Target::Vagrant)?;
    let vagrantfile = file(&vagrantfile, "vagrant/Vagrantfile")?;
    assert!(
        vagrantfile.contains("Isoloom knows no two-way shared folder of the `#{machine.provider_name}` provider\" unless ["),
        "{vagrantfile}"
    );
    Ok(())
}
