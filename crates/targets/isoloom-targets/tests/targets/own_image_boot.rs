//! A machine's own image (the `overrides` giving its box source, Proxmox and cloud images) and
//! its own boot (`hardware.boot`): each target renders what it can and refuses the rest with a
//! reason.

use isoloom_spec::Target;
use isoloom_targets::{Lab, refusal, refusal_on};

use crate::testing::{Outcome, generated as file, valid};

/// Machine `web` with `fields` beside its network, `others` (more machines, as YAML lines) and
/// the spec's `overrides` (YAML lines).
fn build(fields: &str, others: &str, overrides: &str) -> Outcome<Lab> {
    valid(&format!(
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
    let vagrant = file(&s, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    assert!(vagrant.contains("m.vm.box = \"lab/debian\""), "{vagrant}");
    assert!(vagrant.contains("m.vm.box_url = \"https://boxes.example/d.box\""));
    assert!(vagrant.contains("m.vm.box_server_url = \"https://boxes.example\""));
    let proxmox = file(&s, Target::Proxmox, ".isoloom/proxmox/main.tf")?;
    assert!(proxmox.contains("resource \"proxmox_download_file\" \"web_image\""), "{proxmox}");
    assert!(proxmox.contains("url                 = \"https://images.example/d.qcow2\""), "{proxmox}");
    assert!(proxmox.contains("file_id      = proxmox_download_file.web_image.id"), "{proxmox}");
    let cloud = |c: &str| file(&s, Target::CloudVm, &format!(".isoloom/cloud-vm/{c}/main.tf"));
    for (c, line) in [
        ("aws", "= \"ami-0abc\""),
        ("azure", "source_image_id = \"/subscriptions/s/"),
        ("gcp", "image = \"projects/p/global/images/d\""),
        ("digitalocean", "= \"4242\""),
        ("linode", "= \"private/42\""),
        ("oci", "= \"ocid1.image.oc1..d\""),
    ] {
        let text = cloud(c)?;
        assert!(text.contains(line), "{c}: {text}");
    }
    assert!(
        !cloud("aws")?.contains("data \"aws_ami\" \"debian_12\""),
        "no lookup for an OS no machine boots"
    );
    Ok(())
}

#[test]
fn a_proxmox_template_is_cloned_and_an_own_image_stands_in_for_the_os() -> Outcome {
    let kali = spec("os: kali, app: { steps: [{ run: s.sh }] }")?;
    assert!(refusal(&kali, Target::Proxmox).unwrap().contains("no Proxmox image for `kali`"));
    assert!(refusal(&kali, Target::CloudVm).unwrap().contains("no AWS image for `kali`"));
    let s = build(
        "os: kali, app: { steps: [{ run: s.sh }] }",
        "",
        "overrides:\n  proxmox: { machines: { web: { image: '9000' } } }\n  aws: { machines: { web: { image: ami-0abc } } }\n",
    )?;
    let proxmox = file(&s, Target::Proxmox, ".isoloom/proxmox/main.tf")?;
    assert!(proxmox.contains("  clone {\n    vm_id = 9000\n    full  = true\n  }"), "{proxmox}");
    assert!(!proxmox.contains("proxmox_download_file.kali"), "{proxmox}");
    assert_eq!(refusal(&s, Target::CloudVm), None);
    Ok(())
}

const BOOT: &str = "{ kernel: /srv/vmlinuz, initrd: /srv/initrd, cmdline: console=ttyS0, loader: /usr/share/OVMF/OVMF_CODE.fd, nvram: /srv/web_VARS.fd, firmware_format: qcow2, order: [network, hd], network: { tftp_root: /srv/tftp, file: pxelinux.0, server: 10.30.0.1 } }";

#[test]
fn libvirt_boots_the_machines_own_way() -> Outcome {
    let vagrant = file(&spec(&boot(BOOT))?, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    for line in [
        "v.kernel = \"/srv/vmlinuz\"",
        "v.initrd = \"/srv/initrd\"",
        "v.cmd_line = \"console=ttyS0\"",
        "v.loader = \"/usr/share/OVMF/OVMF_CODE.fd\"",
        "v.nvram = \"/srv/web_VARS.fd\"",
        "v.boot \"network\"\n      v.boot \"hd\"",
        "v.firmware_format = \"qcow2\"",
        "libvirt__dhcp_enabled: true, libvirt__tftp_root: \"/srv/tftp\", libvirt__dhcp_bootp_file: \"pxelinux.0\", libvirt__dhcp_bootp_server: \"10.30.0.1\"",
    ] {
        assert!(vagrant.contains(line), "{line}\n{vagrant}");
    }
    Ok(())
}

#[test]
fn a_target_or_provider_that_boots_the_image_as_it_is_refuses_and_says_why() -> Outcome {
    let s = spec(&boot(BOOT))?;
    assert_eq!(refusal_on(&s, Target::Vagrant, Some("libvirt")), None);
    let vbox = refusal_on(&s, Target::Vagrant, Some("virtualbox")).unwrap();
    assert!(vbox.contains("Vagrant's virtualbox provider boots the box as it is"), "{vbox}");
    let qemu = refusal_on(&pair(&boot(BOOT))?, Target::Vagrant, Some("qemu")).unwrap();
    assert!(qemu.contains("`hardware.boot.kernel`, `hardware.boot.initrd`"), "{qemu}");
    assert_eq!(refusal_on(&pair(&boot("{ firmware_format: raw }"))?, Target::Vagrant, Some("qemu")), None);
    let cloud = refusal(&s, Target::CloudVm).unwrap();
    assert!(cloud.contains("machine `web` boots its own way (`hardware.boot`)"), "{cloud}");
    let proxmox = refusal(&s, Target::Proxmox).unwrap();
    assert!(
        proxmox.contains("machine `web` sets `hardware.boot.kernel`: Proxmox boots the VM's disk"),
        "{proxmox}"
    );
    let cdrom = refusal(&spec(&boot("{ order: [cdrom] }"))?, Target::Proxmox).unwrap();
    assert!(cdrom.contains("no CD-ROM drive"), "{cdrom}");
    Ok(())
}

#[test]
fn proxmox_boots_uefi_in_the_machines_order() -> Outcome {
    let s = spec(&boot("{ firmware: uefi, firmware_format: qcow2, order: [network, hd] }"))?;
    let proxmox = file(&s, Target::Proxmox, ".isoloom/proxmox/main.tf")?;
    for line in [
        "= \"ovmf\"",
        "file_format       = \"qcow2\"",
        "pre_enrolled_keys = false",
        "boot_order = [\"net0\", \"virtio0\"]",
    ] {
        assert!(proxmox.contains(line), "{line}\n{proxmox}");
    }
    let bios = file(&spec(&boot("{ firmware: bios }"))?, Target::Proxmox, ".isoloom/proxmox/main.tf")?;
    assert!(bios.contains("= \"seabios\"") && !bios.contains("efi_disk"), "{bios}");
    Ok(())
}
