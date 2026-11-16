//! A machine's own image (the `overrides` giving its box source, Proxmox and cloud images) and
//! its own boot (`hardware.boot`): each target renders what it can and refuses the rest with a
//! reason.

use isoloom_spec::Target;
use isoloom_targets::{Lab, refusal, refusal_on};

use isoloom_test_support::{hcl_blocks, hcl_body, hcl_names, hcl_nested, hcl_one, hcl_text, hcl_values};

use crate::testing::{Outcome, define, generated as file, vagrantfile, valid_allowed};

/// Machine `web` with `fields` beside its network, `others` (more machines, as YAML lines) and
/// the spec's `overrides` (YAML lines).
fn build(fields: &str, others: &str, overrides: &str) -> Outcome<Lab> {
    valid_allowed(&format!(
        "version: 1\nname: own\nnetworks: {{ lan: {{ cidr: 10.30.0.0/24 }} }}\nmachines:\n  web: {{ networks: {{ lan: 10 }}, {fields} }}\n{others}{overrides}"
    ))
}

/// Debian set up by a script.
const DEBIAN: &str = "os: debian@12, app: { steps: [{ run: s.sh }] }";

/// Machine `web` with `fields`, alone.
fn spec(fields: &str) -> Outcome<Lab> {
    build(fields, "", "")
}

/// `web` and a second VM on its network: QEMU links VMs in pairs.
fn pair(fields: &str) -> Outcome<Lab> {
    build(fields, "  db: { networks: { lan: 11 }, os: debian@12, app: { steps: [{ run: s.sh }] } }\n", "")
}

/// Debian set up by a script, booting its own way (`boot`: the `hardware.boot` block).
fn boot(boot: &str) -> String {
    format!("{DEBIAN}, hardware: {{ boot: {boot} }}")
}

const IMAGES: &str = "overrides:
  vagrant: { machines: { web: { box: lab/debian, box_url: 'https://boxes.example/d.box', box_server_url: 'https://boxes.example' } } }
  proxmox: { machines: { web: { image: 'https://images.example/d.qcow2' } } }
  aws: { machines: { web: { image: ami-0abc } } }
  azure: { machines: { web: { image: /subscriptions/s/resourceGroups/g/providers/Microsoft.Compute/images/d } } }
  gcp: { machines: { web: { image: projects/p/global/images/d } } }
  digitalocean: { machines: { web: { image: '4242' } } }
  linode: { machines: { web: { image: private/42 } } }
  oci: { machines: { web: { image: ocid1.image.oc1..d } } }
";

#[test]
fn every_target_boots_the_machines_own_image() -> Outcome {
    let s = build(DEBIAN, "", IMAGES)?;
    let vf = vagrantfile(&s)?;
    assert_eq!(
        define(&vf, "web").statements("m.vm.box"),
        [
            "m.vm.box = \"lab/debian\"",
            "m.vm.box_url = \"https://boxes.example/d.box\"",
            "m.vm.box_server_url = \"https://boxes.example\""
        ]
    );
    let proxmox = hcl_body(&file(&s, Target::Proxmox, ".isoloom/proxmox/main.tf")?);
    assert_eq!(
        hcl_text(hcl_one(&proxmox, "proxmox_download_file", "web_image"), "url"),
        "https://images.example/d.qcow2"
    );
    let disks = hcl_nested(hcl_one(&proxmox, "proxmox_virtual_environment_vm", "web"), "disk");
    let file_ids: Vec<String> = disks.iter().map(|d| hcl_text(d.body(), "file_id")).collect();
    assert_eq!(file_ids, ["proxmox_download_file.web_image.id"]);
    let cloud = |c: &str| -> Outcome<isoloom_test_support::hcl::Body> { Ok(hcl_body(&file(&s, Target::CloudVm, &format!(".isoloom/cloud-vm/{c}/main.tf"))?)) };
    for (c, kind, key, image) in [
        ("aws", "aws_instance", "ami", "ami-0abc"),
        (
            "azure",
            "azurerm_linux_virtual_machine",
            "source_image_id",
            "/subscriptions/s/resourceGroups/g/providers/Microsoft.Compute/images/d",
        ),
        ("gcp", "google_compute_instance", "image", "projects/p/global/images/d"),
        ("digitalocean", "digitalocean_droplet", "image", "4242"),
        ("linode", "linode_instance", "image", "private/42"),
        ("oci", "oci_core_instance", "source_id", "ocid1.image.oc1..d"),
    ] {
        let module = cloud(c)?;
        assert_eq!(hcl_values(hcl_one(&module, kind, "web"), key), [image], "{c}");
    }
    assert_eq!(
        hcl_blocks(&cloud("aws")?, "data", &["aws_ami"]).len(),
        0,
        "no lookup for an OS no machine boots"
    );
    Ok(())
}

#[test]
fn a_proxmox_template_is_cloned_and_an_own_image_stands_in_for_the_os() -> Outcome {
    let kali = spec("os: kali, app: { steps: [{ run: s.sh }] }")?;
    let why = refusal(&kali, Target::Proxmox)?.ok_or("Kali has no Proxmox image")?;
    assert!(why.contains("no Proxmox image for `kali`"), "{why}");
    // Kali is an AWS OS (its Marketplace image): only Proxmox needs the machine's own image.
    assert_eq!(refusal(&kali, Target::CloudVm).unwrap(), None);
    let s = build(
        "os: kali, app: { steps: [{ run: s.sh }] }",
        "",
        "overrides:\n  proxmox: { machines: { web: { image: '9000' } } }\n  aws: { machines: { web: { image: ami-0abc } } }\n",
    )?;
    let proxmox = hcl_body(&file(&s, Target::Proxmox, ".isoloom/proxmox/main.tf")?);
    let clone = hcl_nested(hcl_one(&proxmox, "proxmox_virtual_environment_vm", "web"), "clone");
    let cloned: Vec<(String, String)> = clone.iter().map(|c| (hcl_text(c.body(), "vm_id"), hcl_text(c.body(), "full"))).collect();
    assert_eq!(cloned, [("9000".to_owned(), "true".to_owned())]);
    let downloads = hcl_names(&proxmox, "resource", "proxmox_download_file");
    assert!(!downloads.iter().any(|d| d.contains("kali")), "no Kali image to download: {downloads:?}");
    assert_eq!(refusal(&s, Target::CloudVm).unwrap(), None);
    Ok(())
}

const BOOT: &str = "{ kernel: /srv/vmlinuz, initrd: /srv/initrd, cmdline: console=ttyS0, loader: /usr/share/OVMF/OVMF_CODE.fd, nvram: /srv/web_VARS.fd, firmware_format: qcow2, order: [network, hd], network: { tftp_root: /srv/tftp, file: pxelinux.0, server: 10.30.0.1 } }";

#[test]
fn libvirt_boots_the_machines_own_way() -> Outcome {
    let vf = vagrantfile(&spec(&boot(BOOT))?)?;
    let web = define(&vf, "web");
    let libvirt = web.block("m.vm.provider \"libvirt\"").every_statement();
    for statement in [
        "v.kernel = \"/srv/vmlinuz\"",
        "v.initrd = \"/srv/initrd\"",
        "v.cmd_line = \"console=ttyS0\"",
        "v.loader = \"/usr/share/OVMF/OVMF_CODE.fd\"",
        "v.nvram = \"/srv/web_VARS.fd\"",
    ] {
        assert!(libvirt.contains(&statement), "{statement}: {libvirt:?}");
    }
    // QEMU's provider takes the firmware's format.
    let qemu = web.block("m.vm.provider \"qemu\"").every_statement();
    assert!(qemu.contains(&"v.firmware_format = \"qcow2\""), "{qemu:?}");
    let order: Vec<&&str> = libvirt.iter().filter(|s| s.starts_with("v.boot ")).collect();
    assert_eq!(order, [&"v.boot \"network\"", &"v.boot \"hd\""]);
    let tftp =
        "libvirt__dhcp_enabled: true, libvirt__tftp_root: \"/srv/tftp\", libvirt__dhcp_bootp_file: \"pxelinux.0\", libvirt__dhcp_bootp_server: \"10.30.0.1\"";
    assert!(
        web.every_statement()
            .iter()
            .any(|s| s.starts_with("m.vm.network \"private_network\"") && s.contains(tftp)),
        "{web:?}"
    );
    Ok(())
}

#[test]
fn a_target_or_provider_that_boots_the_image_as_it_is_refuses_and_says_why() -> Outcome {
    let s = spec(&boot(BOOT))?;
    assert_eq!(refusal_on(&s, Target::Vagrant, Some("libvirt")).unwrap(), None);
    let vbox = refusal_on(&s, Target::Vagrant, Some("virtualbox"))?.ok_or("virtualbox boots it")?;
    assert!(vbox.contains("Vagrant's virtualbox provider boots the box as it is"), "{vbox}");
    let qemu = refusal_on(&pair(&boot(BOOT))?, Target::Vagrant, Some("qemu"))?.ok_or("qemu boots it")?;
    assert!(qemu.contains("`hardware.boot.kernel`, `hardware.boot.initrd`"), "{qemu}");
    assert_eq!(
        refusal_on(&pair(&boot("{ firmware_format: raw }"))?, Target::Vagrant, Some("qemu")).unwrap(),
        None
    );
    let cloud = refusal(&s, Target::CloudVm)?.ok_or("the clouds boot it")?;
    assert!(cloud.contains("machine `web` boots its own way (`hardware.boot`)"), "{cloud}");
    let proxmox = refusal(&s, Target::Proxmox)?.ok_or("Proxmox boots it")?;
    assert!(
        proxmox.contains("machine `web` sets `hardware.boot.kernel`: Proxmox boots the VM's disk"),
        "{proxmox}"
    );
    let cdrom = refusal(&spec(&boot("{ order: [cdrom] }"))?, Target::Proxmox)?.ok_or("Proxmox boots a CD-ROM")?;
    assert!(cdrom.contains("no CD-ROM drive"), "{cdrom}");
    Ok(())
}

#[test]
fn proxmox_boots_uefi_in_the_machines_order() -> Outcome {
    let s = spec(&boot("{ firmware: uefi, firmware_format: qcow2, order: [network, hd] }"))?;
    let proxmox = hcl_body(&file(&s, Target::Proxmox, ".isoloom/proxmox/main.tf")?);
    let vm = hcl_one(&proxmox, "proxmox_virtual_environment_vm", "web");
    assert_eq!(hcl_text(vm, "bios"), "ovmf");
    let efi = hcl_nested(vm, "efi_disk");
    let efi: Vec<(String, String)> = efi
        .iter()
        .map(|e| (hcl_text(e.body(), "file_format"), hcl_text(e.body(), "pre_enrolled_keys")))
        .collect();
    assert_eq!(efi, [("qcow2".to_owned(), "false".to_owned())]);
    assert_eq!(hcl_values(vm, "boot_order"), ["net0", "virtio0"]);
    let bios = hcl_body(&file(&spec(&boot("{ firmware: bios }"))?, Target::Proxmox, ".isoloom/proxmox/main.tf")?);
    let bios_vm = hcl_one(&bios, "proxmox_virtual_environment_vm", "web");
    assert_eq!(hcl_text(bios_vm, "bios"), "seabios");
    assert_eq!(hcl_nested(bios_vm, "efi_disk").len(), 0, "no EFI disk");
    Ok(())
}
