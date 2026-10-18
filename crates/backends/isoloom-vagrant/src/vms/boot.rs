//! A machine's own image and boot in the Vagrantfile: where its box comes from (`box_url`,
//! `box_server_url`), the libvirt domain's kernel, firmware and boot order, QEMU's firmware
//! format, and the network boot its libvirt network serves.
//!
//! Every provider's block is in one Vagrantfile, so a provider that can't boot the machine's
//! own way says so through [`refusal`] (`refusal_on` with the provider) instead of ignoring it.

use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::model::{Boot, Firmware, NetworkBoot, VmImage};

use super::provider::Provider;
use crate::ruby::{Call, Ruby, Value};

/// Where Vagrant gets the box: a URL as written, a box file in the project from the project's
/// folder (the Vagrantfile sits two levels below it), and the box server to look it up on.
pub(crate) fn source(r: &mut Ruby, image: Option<&VmImage>) {
    let Some(i) = image else { return };
    if let Some(u) = &i.box_url {
        let url = if u.contains("://") {
            Value::str(u)
        } else {
            Value::raw(format!("File.join(ROOT, {})", Value::str(u)))
        };
        r.assign("m.vm.box_url", url);
    }
    if let Some(u) = &i.box_server_url {
        r.assign("m.vm.box_server_url", Value::str(u));
    }
}

/// The libvirt domain's own boot, in the provider's block (`v`): the kernel booted directly
/// with its initrd, command line and device tree, the UEFI firmware and its variables, then
/// the boot devices in order (each `v.boot` call adds one to `boot_order`).
pub(crate) fn libvirt(r: &mut Ruby, b: &Boot) {
    let paths = [
        ("v.kernel", &b.kernel),
        ("v.initrd", &b.initrd),
        ("v.cmd_line", &b.cmdline),
        ("v.dtb", &b.dtb),
        ("v.loader", &b.loader),
        ("v.nvram", &b.nvram),
    ];
    for (setting, value) in paths {
        if let Some(v) = value {
            r.assign(setting, Value::str(v));
        }
    }
    for d in &b.order {
        r.call(Call::new("v.boot").arg(Value::str(d.id())));
    }
}

/// QEMU's own boot: the format of its UEFI firmware files.
pub(crate) fn qemu(r: &mut Ruby, b: &Boot) {
    if let Some(f) = b.firmware_format {
        r.assign("v.firmware_format", Value::str(f.id()));
    }
}

/// A private network's libvirt options for the network boot it serves: DHCP on (a booting
/// machine asks it for the boot file), the TFTP folder, and the file with its server.
pub(crate) fn network(call: Call, served: Option<&NetworkBoot>) -> Call {
    let Some(n) = served else {
        return call.kw("libvirt__dhcp_enabled", false);
    };
    let mut call = call.kw("libvirt__dhcp_enabled", true);
    if let Some(t) = &n.tftp_root {
        call = call.kw("libvirt__tftp_root", Value::str(t));
    }
    if let Some(f) = &n.file {
        call = call.kw("libvirt__dhcp_bootp_file", Value::str(f));
    }
    if let Some(s) = &n.server {
        call = call.kw("libvirt__dhcp_bootp_server", Value::str(s));
    }
    call
}

/// Why `provider` can't boot a machine of `env` its own way, if it can't. libvirt boots every
/// field; QEMU sets the firmware's format only (it boots the box's disk through its own
/// firmware); the other providers boot the box as it is.
pub(crate) fn refusal(env: &Environment, provider: Provider<'_>) -> Option<String> {
    let (name, b) = env
        .machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Vm)
        .find_map(|(n, mp)| Some((n, mp.boot.as_ref()?)))?;
    match provider {
        Provider::Libvirt => (b.firmware == Some(Firmware::Uefi) && b.loader.is_none()).then(|| {
            format!("machine `{name}` sets `hardware.boot.firmware: uefi`: libvirt boots UEFI from the firmware file in `hardware.boot.loader`; give it")
        }),
        Provider::Qemu => {
            let beyond = b.beyond_firmware_format();
            (!beyond.is_empty()).then(|| {
                format!(
                    "machine `{name}` sets `hardware.boot.{}`: Vagrant's QEMU provider boots the box's disk through its own firmware (only `firmware_format` applies); the libvirt provider boots it",
                    beyond.join("`, `hardware.boot.")
                )
            })
        }
        other => Some(format!(
            "machine `{name}` boots its own way (`hardware.boot`): Vagrant's {} provider boots the box as it is; the libvirt provider boots it",
            other.id()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::environment;

    fn env(boot: &str) -> Environment {
        environment(&format!(
            "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n  a: {{ networks: {{ lan: 10 }}, os: debian@12, app: {{ steps: [{{ run: s.sh }}] }}, hardware: {{ boot: {boot} }} }}\n"
        ))
    }

    #[test]
    fn libvirt_boots_a_kernel_and_qemu_only_the_firmware_format() {
        let kernel = env("{ kernel: /boot/vmlinuz, firmware_format: raw }");
        assert_eq!(refusal(&kernel, Provider::Libvirt), None);
        let qemu = refusal(&kernel, Provider::Qemu).unwrap_or_default();
        assert!(qemu.contains("`hardware.boot.kernel`") && !qemu.contains("firmware_format`:"), "{qemu}");
        assert_eq!(refusal(&env("{ firmware_format: qcow2, firmware: uefi }"), Provider::Qemu), None);
    }

    #[test]
    fn libvirt_boots_uefi_from_the_loader_it_is_given() {
        let uefi = refusal(&env("{ firmware: uefi }"), Provider::Libvirt).unwrap_or_default();
        assert!(uefi.contains("`hardware.boot.loader`"), "{uefi}");
    }

    #[test]
    fn the_other_providers_boot_the_box_as_it_is() {
        let vbox = refusal(&env("{ kernel: /boot/vmlinuz }"), Provider::VirtualBox).unwrap_or_default();
        assert!(vbox.contains("virtualbox provider boots the box as it is"), "{vbox}");
    }

    #[test]
    fn a_box_file_in_the_project_is_found_from_the_vagrantfile() {
        let mut r = Ruby::new();
        let image = VmImage {
            vagrant: Some("my/box".into()),
            box_url: Some("boxes/my.box".into()),
            box_server_url: Some("https://boxes.example".into()),
            ..VmImage::default()
        };
        source(&mut r, Some(&image));
        let text = r.finish();
        assert!(text.contains("m.vm.box_url = File.join(ROOT, \"boxes/my.box\")"), "{text}");
        assert!(text.contains("m.vm.box_server_url = \"https://boxes.example\""), "{text}");
    }
}
