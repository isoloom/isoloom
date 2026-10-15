//! A machine's own image (`vm.image`'s box source, Proxmox and cloud images) and its own boot
//! (`vm.boot`): each target renders what it can and refuses the rest with a reason.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use isoloom_spec::{Target, parse};
use isoloom_targets::{generate, refusal, refusal_on};

/// Machine `web` with `vm`, and `others` (more machines, as YAML lines).
fn build(vm: &str, others: &str) -> isoloom_spec::Spec {
    let s = parse(&format!(
        "version: 1\nname: own\nnetworks: {{ lan: {{ cidr: 10.30.0.0/24 }} }}\nmachines:\n  web: {{ networks: {{ lan: 10 }}, vm: {vm} }}\n{others}"
    ))
    .unwrap();
    assert_eq!(isoloom_spec::validate(&s), vec![]);
    s
}

fn spec(vm: &str) -> isoloom_spec::Spec {
    build(vm, "")
}

/// `web` and a second VM on its network: QEMU links VMs in pairs.
fn pair(vm: &str) -> isoloom_spec::Spec {
    build(vm, "  db: { networks: { lan: 11 }, vm: { os: debian@12, provision: [s.sh] } }\n")
}

fn file(s: &isoloom_spec::Spec, target: Target, path: &str) -> String {
    let files = generate(s, target).unwrap();
    files.into_iter().find(|f| f.path == path).unwrap_or_else(|| panic!("no {path}")).contents
}

const IMAGES: &str = "{ os: debian@12, provision: [s.sh], image: { vagrant: lab/debian, box_url: 'https://boxes.example/d.box', box_server_url: 'https://boxes.example', proxmox: 'https://images.example/d.qcow2', aws: ami-0abc, azure: /subscriptions/s/resourceGroups/g/providers/Microsoft.Compute/images/d, gcp: projects/p/global/images/d, digitalocean: '4242', linode: private/42, oci: ocid1.image.oc1..d } }";

#[test]
fn every_target_boots_the_machines_own_image() {
    let s = spec(IMAGES);
    let vagrant = file(&s, Target::Vagrant, ".isoloom/vagrant/Vagrantfile");
    assert!(vagrant.contains("m.vm.box = \"lab/debian\""), "{vagrant}");
    assert!(vagrant.contains("m.vm.box_url = \"https://boxes.example/d.box\""));
    assert!(vagrant.contains("m.vm.box_server_url = \"https://boxes.example\""));
    let proxmox = file(&s, Target::Proxmox, ".isoloom/proxmox/main.tf");
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
        let text = cloud(c);
        assert!(text.contains(line), "{c}: {text}");
    }
    assert!(!cloud("aws").contains("data \"aws_ami\" \"debian_12\""), "no lookup for an OS no machine boots");
}

#[test]
fn a_proxmox_template_is_cloned_and_an_own_image_stands_in_for_the_os() {
    let kali = spec("{ os: kali, provision: [s.sh] }");
    assert!(refusal(&kali, Target::Proxmox).unwrap().contains("no Proxmox image for `kali`"));
    assert!(refusal(&kali, Target::CloudVm).unwrap().contains("no AWS image for `kali`"));
    let s = spec("{ os: kali, provision: [s.sh], image: { proxmox: '9000', aws: ami-0abc } }");
    let proxmox = file(&s, Target::Proxmox, ".isoloom/proxmox/main.tf");
    assert!(proxmox.contains("  clone {\n    vm_id = 9000\n    full  = true\n  }"), "{proxmox}");
    assert!(!proxmox.contains("proxmox_download_file.kali"), "{proxmox}");
    assert_eq!(refusal(&s, Target::CloudVm), None);
}

const BOOT: &str = "{ os: debian@12, provision: [s.sh], boot: { kernel: /srv/vmlinuz, initrd: /srv/initrd, cmdline: console=ttyS0, loader: /usr/share/OVMF/OVMF_CODE.fd, nvram: /srv/web_VARS.fd, firmware_format: qcow2, order: [network, hd], network: { tftp_root: /srv/tftp, file: pxelinux.0, server: 10.30.0.1 } } }";

#[test]
fn libvirt_boots_the_machines_own_way() {
    let vagrant = file(&spec(BOOT), Target::Vagrant, ".isoloom/vagrant/Vagrantfile");
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
}

#[test]
fn a_target_or_provider_that_boots_the_image_as_it_is_refuses_and_says_why() {
    let s = spec(BOOT);
    assert_eq!(refusal_on(&s, Target::Vagrant, Some("libvirt")), None);
    let vbox = refusal_on(&s, Target::Vagrant, Some("virtualbox")).unwrap();
    assert!(vbox.contains("Vagrant's virtualbox provider boots the box as it is"), "{vbox}");
    let qemu = refusal_on(&pair(BOOT), Target::Vagrant, Some("qemu")).unwrap();
    assert!(qemu.contains("`vm.boot.kernel`, `vm.boot.initrd`"), "{qemu}");
    assert_eq!(
        refusal_on(
            &pair("{ os: debian@12, provision: [s.sh], boot: { firmware_format: raw } }"),
            Target::Vagrant,
            Some("qemu")
        ),
        None
    );
    let cloud = refusal(&s, Target::CloudVm).unwrap();
    assert!(cloud.contains("machine `web` boots its own way (`vm.boot`)"), "{cloud}");
    let proxmox = refusal(&s, Target::Proxmox).unwrap();
    assert!(
        proxmox.contains("machine `web` sets `vm.boot.kernel`: Proxmox boots the VM's disk"),
        "{proxmox}"
    );
    let cdrom = refusal(&spec("{ os: debian@12, provision: [s.sh], boot: { order: [cdrom] } }"), Target::Proxmox).unwrap();
    assert!(cdrom.contains("no CD-ROM drive"), "{cdrom}");
}

#[test]
fn proxmox_boots_uefi_in_the_machines_order() {
    let s = spec("{ os: debian@12, provision: [s.sh], boot: { firmware: uefi, firmware_format: qcow2, order: [network, hd] } }");
    let proxmox = file(&s, Target::Proxmox, ".isoloom/proxmox/main.tf");
    for line in [
        "= \"ovmf\"",
        "file_format       = \"qcow2\"",
        "pre_enrolled_keys = false",
        "boot_order = [\"net0\", \"virtio0\"]",
    ] {
        assert!(proxmox.contains(line), "{line}\n{proxmox}");
    }
    let bios = file(
        &spec("{ os: debian@12, provision: [s.sh], boot: { firmware: bios } }"),
        Target::Proxmox,
        ".isoloom/proxmox/main.tf",
    );
    assert!(bios.contains("= \"seabios\"") && !bios.contains("efi_disk"), "{bios}");
}
