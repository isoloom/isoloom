//! The Proxmox module of a few examples, rendered from their plan, as snapshots (not run on a
//! real host); its determinism; what it refuses.

use crate::testing::{Outcome, example, file, planned, spec};
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::ProxmoxBackend;
use isoloom_test_support::{hcl_nested, hcl_resource, hcl_text};

/// The Proxmox module of example `name`.
fn module(name: &str) -> Outcome<String> {
    let files = ProxmoxBackend.render(&planned(&example(name)?, Target::Proxmox)?)?;
    Ok(file(&files, "proxmox/main.tf")?.to_owned())
}

/// The modules a reviewer reads as files; windows-hello is a clone of a Windows template, set up
/// by cloudbase-init.
#[test]
fn the_proxmox_module_of_the_examples() -> Outcome {
    for name in ["hello-stack", "segmented", "ansible-pair", "windows-hello"] {
        insta::assert_snapshot!(format!("proxmox_{}", name.replace('-', "_")), module(name)?);
    }
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for name in ["hello-stack", "ansible-pair", "existing-hosts"] {
        let e = planned(&example(name)?, Target::Proxmox)?;
        assert_eq!(ProxmoxBackend.render(&e), ProxmoxBackend.render(&e), "{name}");
    }
    Ok(())
}

/// A spec of one machine, `w`, with `fields` (its OS, steps and hardware).
fn one_vm(fields: &str) -> Outcome<isoloom_plan::Lab> {
    spec(&format!(
        "version: 1\nname: t\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  w: {{ networks: {{ lab: 10 }}, {fields} }}\n"
    ))
}

/// VM `w` of the Proxmox module of `lab`.
fn vm(lab: &isoloom_plan::Lab) -> Outcome<hcl::Body> {
    let files = ProxmoxBackend.render(&planned(lab, Target::Proxmox)?)?;
    Ok(hcl_resource(file(&files, "proxmox/main.tf")?, "proxmox_virtual_environment_vm", "w"))
}

#[test]
fn an_arm64_vm_running_vms_inside_is_refused_and_an_amd64_one_takes_the_servers_cpu() -> Outcome {
    let nested = |arch: &str| one_vm(&format!("os: debian@12, hardware: {{ arch: {arch}, nested_virtualization: true }}"));
    let why = ProxmoxBackend.refusal(&planned(&nested("arm64")?, Target::Proxmox)?).unwrap_or_default();
    assert!(
        why.contains("machine `w` runs VMs inside (`nested_virtualization`), and an arm64 VM's CPU is emulated"),
        "{why}"
    );
    let amd = vm(&nested("amd64")?)?;
    let cpu = hcl_nested(&amd, "cpu");
    assert_eq!(cpu.iter().map(|c| hcl_text(c.body(), "type")).collect::<Vec<_>>(), ["host"]);
    Ok(())
}

#[test]
fn a_tpm_is_a_tpm_state_on_the_servers_datastore() -> Outcome {
    let tpm = one_vm("os: debian@12, app: { steps: [{ run: a.sh }] }, hardware: { tpm: required }")?;
    assert_eq!(hcl_nested(&vm(&tpm)?, "tpm_state").len(), 1);
    let none = one_vm("os: debian@12, app: { steps: [{ run: a.sh }] }")?;
    assert_eq!(hcl_nested(&vm(&none)?, "tpm_state").len(), 0);
    Ok(())
}

#[test]
fn proxmox_refuses_what_its_windows_machines_cant_have_yet() -> Outcome {
    // Validation refuses a playbook step on Windows; the backend still guards its own plan.
    let yaml = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  w: { networks: { lab: 10 }, os: windows-server@2022, app: { steps: [{ playbook: a.yml }] } }\n";
    let why = crate::testing::refusal(&ProxmoxBackend, yaml)?.ok_or("Proxmox takes a playbook step on Windows")?;
    assert!(why.starts_with("machine `w`: Windows provisioning steps are PowerShell scripts"), "{why}");
    Ok(())
}

#[test]
fn a_boot_proxmox_has_no_setting_for_is_refused_naming_the_field() -> Outcome {
    for (os, boot, why) in [
        (
            "debian@12",
            "{ kernel: /boot/vmlinuz }",
            "machine `w` sets `hardware.boot.kernel`: Proxmox boots the VM's disk through its own firmware (`firmware`, `order` and `firmware_format` apply); Vagrant's libvirt provider boots it",
        ),
        (
            "windows-server@2022",
            "{ firmware: bios }",
            "machine `w` sets `hardware.boot.firmware`: a Windows machine boots its template's UEFI on Proxmox",
        ),
        (
            "debian@12",
            "{ order: [cdrom, hd] }",
            "machine `w` boots from `cdrom`: its Proxmox VM has no CD-ROM drive",
        ),
        (
            "debian@12",
            "{ firmware: bios, firmware_format: raw }",
            "machine `w` sets `hardware.boot.firmware_format`: on Proxmox it is the EFI disk's, with `firmware: uefi`",
        ),
    ] {
        // Planned unchecked: validation refuses a `firmware_format` without UEFI before the
        // backend, which still guards its own plan.
        let yaml = format!(
            "version: 1\nname: t\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  w: {{ networks: {{ lab: 10 }}, os: {os}, hardware: {{ boot: {boot} }} }}\n"
        );
        assert_eq!(crate::testing::refusal(&ProxmoxBackend, &yaml)?.as_deref(), Some(why), "{boot}");
    }
    // UEFI with its EFI disk's format: taken.
    let uefi = one_vm("os: debian@12, hardware: { boot: { firmware: uefi, firmware_format: qcow2 } }")?;
    assert_eq!(ProxmoxBackend.refusal(&planned(&uefi, Target::Proxmox)?), None);
    Ok(())
}
