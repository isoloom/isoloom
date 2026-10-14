//! A VM's own image (`vm.image`'s box URL, Proxmox and cloud images) and its own boot
//! (`vm.boot`): what each names must be the kind of reference its target takes, and the boot's
//! fields must hang together (an initrd with a kernel, a network boot on a network).

use std::collections::HashMap;
use std::net::Ipv4Addr;

use super::Problems;
use crate::model::{Boot, BootDevice, Firmware, NetworkBoot, Spec, VmImage};

/// Every machine's `vm.image` and `vm.boot`, then the controller's image, then the network
/// boots that share a network.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        let at = format!("machines.{name}.vm");
        if let Some(i) = &vm.image {
            image(&format!("{at}.image"), i, p);
        }
        if let Some(b) = &vm.boot {
            boot(&format!("{at}.boot"), b, p);
            if b.network.is_some() && m.networks.is_empty() {
                p.add(
                    format!("{at}.boot.network"),
                    "a network boot is served on the machine's first network: give it one in `networks`",
                );
            }
        }
    }
    controller(spec, p);
    shared_networks(spec, p);
}

/// The references a machine's image gives, each in the form its target takes.
fn image(at: &str, i: &VmImage, p: &mut Problems) {
    if (i.box_url.is_some() || i.box_server_url.is_some()) && i.vagrant.is_none() {
        p.add(
            format!("{at}.vagrant"),
            "name the box in `vagrant` too: Vagrant keeps the box it downloads from `box_url` (or finds on `box_server_url`) under that name",
        );
    }
    if let Some(u) = &i.box_url
        && !is_url(u)
        && (u.starts_with('/') || u.split('/').any(|s| s == ".."))
    {
        p.add(format!("{at}.box_url"), "a URL (https://, file://) or a .box file inside the project folder");
    }
    if let Some(u) = i.box_server_url.as_ref().filter(|u| !u.starts_with("http://") && !u.starts_with("https://")) {
        p.add(format!("{at}.box_server_url"), format!("`{u}` isn't an http(s) URL"));
    }
    if let Some(x) = i
        .proxmox
        .as_ref()
        .filter(|x| !template_id(x) && !x.starts_with("http://") && !x.starts_with("https://"))
    {
        p.add(
            format!("{at}.proxmox"),
            format!("`{x}` is neither a cloud image's http(s) URL nor a template VM's id (a number from 100)"),
        );
    }
    let prefixed = [
        ("aws", &i.aws, "ami-", "an AMI id (`ami-...`)"),
        ("azure", &i.azure, "/subscriptions/", "an image's resource id (`/subscriptions/...`)"),
        ("gcp", &i.gcp, "projects/", "`projects/<project>/global/images/<image>`"),
        ("oci", &i.oci, "ocid1.image.", "an image OCID (`ocid1.image...`)"),
    ];
    for (field, value, prefix, what) in prefixed {
        if let Some(v) = value.as_ref().filter(|v| !v.starts_with(prefix)) {
            p.add(format!("{at}.{field}"), format!("`{v}` isn't {what}"));
        }
    }
    for (field, value) in [("digitalocean", &i.digitalocean), ("linode", &i.linode)] {
        if value.as_ref().is_some_and(|v| v.trim().is_empty() || v.contains(char::is_whitespace)) {
            p.add(format!("{at}.{field}"), "an image's slug or id, without spaces");
        }
    }
}

fn is_url(s: &str) -> bool {
    s.split_once("://")
        .is_some_and(|(scheme, _)| !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphabetic()))
}

/// A Proxmox VM id: 100 to 999999999.
fn template_id(s: &str) -> bool {
    s.parse::<u32>().is_ok_and(|n| (100..=999_999_999).contains(&n))
}

/// The boot's fields: host paths are absolute, each part has what it hangs on.
fn boot(at: &str, b: &Boot, p: &mut Problems) {
    let paths = [
        ("kernel", &b.kernel),
        ("initrd", &b.initrd),
        ("dtb", &b.dtb),
        ("loader", &b.loader),
        ("nvram", &b.nvram),
    ];
    for (field, value) in paths {
        if let Some(v) = value.as_ref().filter(|v| !v.starts_with('/')) {
            p.add(format!("{at}.{field}"), format!("`{v}` must be an absolute path on the hypervisor's host"));
        }
    }
    for (field, set) in [("initrd", b.initrd.is_some()), ("cmdline", b.cmdline.is_some()), ("dtb", b.dtb.is_some())] {
        if set && b.kernel.is_none() {
            p.add(format!("{at}.{field}"), format!("`{field}` goes with a `kernel`: give the kernel it boots"));
        }
    }
    if b.firmware == Some(Firmware::Bios) && (b.loader.is_some() || b.firmware_format.is_some()) {
        p.add(
            format!("{at}.firmware"),
            "`loader` and `firmware_format` are a UEFI firmware's: `firmware: uefi`, or leave them out",
        );
    }
    if b.nvram.is_some() && b.loader.is_none() {
        p.add(
            format!("{at}.nvram"),
            "`nvram` keeps a UEFI firmware's variables: give the firmware in `loader`",
        );
    }
    for (i, d) in b.order.iter().enumerate() {
        if b.order[..i].contains(d) {
            p.add(format!("{at}.order[{i}]"), format!("`{}` is listed twice", d.id()));
        }
    }
    if let Some(n) = &b.network {
        network(&format!("{at}.network"), n, b, p);
    }
}

fn network(at: &str, n: &NetworkBoot, b: &Boot, p: &mut Problems) {
    if !b.order.contains(&BootDevice::Network) {
        p.add(at, "add `network` to `boot.order`, or the machine never boots from it");
    }
    if n.tftp_root.is_none() && n.file.is_none() {
        p.add(at, "give the TFTP folder (`tftp_root`) or the boot file (`file`) it serves");
    }
    if let Some(r) = n.tftp_root.as_ref().filter(|r| !r.starts_with('/')) {
        p.add(format!("{at}.tftp_root"), format!("`{r}` must be an absolute path on the hypervisor's host"));
    }
    if let Some(s) = n.server.as_ref().filter(|s| s.parse::<Ipv4Addr>().is_err()) {
        p.add(format!("{at}.server"), format!("`{s}` isn't an IPv4 address"));
    }
    if n.server.is_some() && n.file.is_none() {
        p.add(format!("{at}.server"), "the server holds the boot `file`: give the file too");
    }
}

/// The controller's image is a Vagrant box: the other targets build it from their own image.
fn controller(spec: &Spec, p: &mut Problems) {
    let Some(i) = spec.controller.as_ref().and_then(|c| c.image.as_ref()) else {
        return;
    };
    let others = [
        ("box_url", i.box_url.is_some()),
        ("box_server_url", i.box_server_url.is_some()),
        ("proxmox", i.proxmox.is_some()),
        ("aws", i.aws.is_some()),
        ("azure", i.azure.is_some()),
        ("gcp", i.gcp.is_some()),
        ("digitalocean", i.digitalocean.is_some()),
        ("linode", i.linode.is_some()),
        ("oci", i.oci.is_some()),
    ];
    for (field, _) in others.iter().filter(|(_, set)| *set) {
        p.add(
            format!("controller.image.{field}"),
            "the controller's image is a Vagrant box (`vagrant`, `vagrant_version`): every other target builds it from Debian",
        );
    }
}

/// A network's DHCP and TFTP server is the network's own: the machines whose network boot is
/// served on the same network must agree on it.
fn shared_networks(spec: &Spec, p: &mut Problems) {
    let mut served: HashMap<&str, (&str, &NetworkBoot)> = HashMap::new();
    for (name, m) in &spec.machines {
        let Some(n) = m.vm.as_ref().and_then(|v| v.boot.as_ref()).and_then(|b| b.network.as_ref()) else {
            continue;
        };
        let Some(net) = m.networks.keys().next() else { continue };
        match served.get(net.as_str()) {
            Some((other, theirs)) if *theirs != n => p.add(
                format!("machines.{name}.vm.boot.network"),
                format!("network `{net}` already serves `{other}`'s network boot: one TFTP folder and boot file per network"),
            ),
            Some(_) => {}
            None => {
                served.insert(net, (name, n));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{parse, validate};

    fn problems(machine: &str) -> Vec<String> {
        let spec = parse(&format!(
            "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n  a: {{ networks: {{ lan: 10 }}, vm: {machine} }}\n  b: {{ networks: {{ lan: 11 }}, vm: {{ os: debian-12, boot: {{ order: [network], network: {{ file: other.0 }} }} }} }}\n"
        ))
        .unwrap();
        validate(&spec).into_iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn a_box_url_needs_the_box_name_and_a_url_or_a_project_file() {
        let p = problems("{ os: debian-12, image: { box_url: /tmp/x.box, box_server_url: ftp://x } }");
        assert!(p.iter().any(|p| p.starts_with("machines.a.vm.image.vagrant: name the box")), "{p:?}");
        assert!(p.iter().any(|p| p.starts_with("machines.a.vm.image.box_url:")), "{p:?}");
        assert!(p.iter().any(|p| p.starts_with("machines.a.vm.image.box_server_url:")), "{p:?}");
        let ok = problems("{ os: debian-12, image: { vagrant: my/box, box_url: boxes/my.box } }");
        assert!(!ok.iter().any(|p| p.starts_with("machines.a.vm.image")), "{ok:?}");
    }

    #[test]
    fn each_cloud_image_is_in_its_clouds_form() {
        let p = problems("{ os: debian-12, image: { proxmox: '42', aws: ubuntu, azure: x, gcp: debian, oci: y, linode: 'a b' } }");
        for field in ["proxmox", "aws", "azure", "gcp", "oci", "linode"] {
            assert!(p.iter().any(|p| p.starts_with(&format!("machines.a.vm.image.{field}:"))), "{field}: {p:?}");
        }
        let ok = problems(
            "{ os: debian-12, image: { proxmox: '9000', aws: ami-1, azure: /subscriptions/s, gcp: projects/p/global/images/i, oci: ocid1.image.oc1..x, linode: private/1, digitalocean: '123' } }",
        );
        assert!(!ok.iter().any(|p| p.starts_with("machines.a.vm.image")), "{ok:?}");
    }

    #[test]
    fn a_boots_parts_hang_on_what_they_need() {
        let p = problems("{ os: debian-12, boot: { initrd: /i, cmdline: x, dtb: d, nvram: /n, order: [hd, hd], network: { tftp_root: t, server: x } } }");
        for want in [
            "boot.initrd: `initrd` goes with a `kernel`",
            "boot.dtb: `d` must be an absolute path",
            "boot.nvram: `nvram` keeps",
            "boot.order[1]: `hd` is listed twice",
            "boot.network: add `network`",
            "boot.network.tftp_root:",
            "boot.network.server: `x` isn't",
            "boot.network: network `lan` already serves `a`",
        ] {
            assert!(p.iter().any(|p| p.contains(want)), "{want}: {p:?}");
        }
    }

    #[test]
    fn the_controller_image_is_a_box() {
        let spec = parse("version: 1\nname: t\nnetworks: { lan: { cidr: 10.0.0.0/24 } }\nmachines: { a: { networks: { lan: 10 }, vm: { os: debian-12 } } }\ncontroller: { image: { vagrant: x/y, aws: ami-1 } }\n").unwrap();
        let p: Vec<String> = validate(&spec).into_iter().map(|p| p.to_string()).collect();
        assert!(p.iter().any(|p| p.starts_with("controller.image.aws:")), "{p:?}");
    }
}
