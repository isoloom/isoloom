//! A machine's own image (`vm.image`'s box source, Proxmox and cloud images) and its own boot
//! (`vm.boot`), for every rendering: two sections each family adds to its own, with the probes
//! that use the fields. Their rows follow `machines.*.vm.image.winrm` in `fields::FIELDS`.

use crate::spec::status::{EXISTING_HOST, NOT_A_VM, PROBE, Status, VAGRANT_BOX};

/// A field naming another target's image.
const OTHER_TARGETS: Status = Status::NotApplicable {
    why: "names another target's image",
};

/// A machine's own boot on a target that boots its image's own kernel and firmware.
const IMAGE_BOOTS_ITSELF: Status = Status::Planned {
    note: "the VM boots its image's own kernel and firmware here",
};

/// The libvirt provider boots it; the others refuse it (`refusal_on` with the provider).
const LIBVIRT_BOOTS: Status = Status::Partial {
    how: "the libvirt provider's domain (`v.kernel`, `v.loader`, `v.boot`...) and network",
    gap: "refused on the providers that boot the box as it is (VirtualBox, VMware, Parallels, UTM, Hyper-V, ESXi, QEMU)",
    proof: PROBE,
};

/// The image fields, each with its status: Vagrant's box source, Proxmox's, then each cloud's.
const fn images(vagrant: Status, proxmox: Status, clouds: &[Status; 6]) -> [(&'static str, Status); 9] {
    [
        ("machines.*.vm.image.box_url", vagrant),
        ("machines.*.vm.image.box_server_url", vagrant),
        ("machines.*.vm.image.proxmox", proxmox),
        ("machines.*.vm.image.aws", clouds[0]),
        ("machines.*.vm.image.azure", clouds[1]),
        ("machines.*.vm.image.gcp", clouds[2]),
        ("machines.*.vm.image.digitalocean", clouds[3]),
        ("machines.*.vm.image.linode", clouds[4]),
        ("machines.*.vm.image.oci", clouds[5]),
    ]
}

/// The boot fields: the ones read from the host's files and the network boot (`files`), then
/// the firmware, its files' format and the boot order.
const fn boot(files: Status, firmware: Status, format: Status, order: Status) -> [(&'static str, Status); 12] {
    [
        ("machines.*.vm.boot.kernel", files),
        ("machines.*.vm.boot.initrd", files),
        ("machines.*.vm.boot.cmdline", files),
        ("machines.*.vm.boot.dtb", files),
        ("machines.*.vm.boot.loader", files),
        ("machines.*.vm.boot.nvram", files),
        ("machines.*.vm.boot.network.tftp_root", files),
        ("machines.*.vm.boot.network.file", files),
        ("machines.*.vm.boot.network.server", files),
        ("machines.*.vm.boot.firmware", firmware),
        ("machines.*.vm.boot.firmware_format", format),
        ("machines.*.vm.boot.order", order),
    ]
}

/// The Compose file and the Kubernetes outputs build machines from `docker:`.
pub const CONTAINER_IMAGES: &[(&str, Status)] = &images(NOT_A_VM, NOT_A_VM, &[NOT_A_VM; 6]);
/// The Compose file and the Kubernetes outputs: no VM to boot.
pub const CONTAINER_BOOT: &[(&str, Status)] = &boot(NOT_A_VM, NOT_A_VM, NOT_A_VM, NOT_A_VM);

/// Existing hosts are used as they are.
pub const EXTERNAL_IMAGES: &[(&str, Status)] = &images(EXISTING_HOST, EXISTING_HOST, &[EXISTING_HOST; 6]);
/// Existing hosts boot as they are.
pub const EXTERNAL_BOOT: &[(&str, Status)] = &boot(EXISTING_HOST, EXISTING_HOST, EXISTING_HOST, EXISTING_HOST);

/// The Vagrantfile: the box's source.
pub const VAGRANT_IMAGES: &[(&str, Status)] = &images(
    Status::Done {
        how: "`box_url` (a URL, or a .box file in the project) and `box_server_url`",
        proof: PROBE,
    },
    OTHER_TARGETS,
    &[OTHER_TARGETS; 6],
);

/// The Vagrantfile: the boot on libvirt (QEMU: UEFI and its files' format).
pub const VAGRANT_BOOT: &[(&str, Status)] = &boot(
    LIBVIRT_BOOTS,
    Status::Partial {
        how: "libvirt boots UEFI from `loader`; QEMU boots UEFI always",
        gap: "refused on the providers that boot the box as it is, and `bios` on QEMU",
        proof: PROBE,
    },
    Status::Partial {
        how: "QEMU's `firmware_format`",
        gap: "libvirt reads the format from the file; the other providers refuse it",
        proof: PROBE,
    },
    LIBVIRT_BOOTS,
);

/// The Proxmox module: a cloud image's URL or a template.
pub const PROXMOX_IMAGES: &[(&str, Status)] = &images(
    VAGRANT_BOX,
    Status::Done {
        how: "a cloud image's URL downloaded for the machine, or a template VM cloned (`clone`)",
        proof: PROBE,
    },
    &[OTHER_TARGETS; 6],
);

/// The Proxmox module: the firmware and the boot order; a boot from the host's files is refused.
pub const PROXMOX_BOOT: &[(&str, Status)] = &boot(
    IMAGE_BOOTS_ITSELF,
    Status::Done {
        how: "`bios` (`seabios`, or `ovmf` with an `efi_disk` for its variables)",
        proof: PROBE,
    },
    Status::Done {
        how: "the EFI disk's `file_format`",
        proof: PROBE,
    },
    Status::Partial {
        how: "`boot_order` (the disk, the first network device)",
        gap: "`cdrom` (refused: the VM has no CD-ROM drive)",
        proof: PROBE,
    },
);

/// A cloud's own image field: each cloud's module reads its own.
const fn cloud(how: &'static str) -> Status {
    Status::Done { how, proof: PROBE }
}

/// The cloud VM modules: each cloud its own image.
pub const CLOUD_IMAGES: &[(&str, Status)] = &images(
    VAGRANT_BOX,
    OTHER_TARGETS,
    &[
        cloud("AWS: the instance's `ami` (each cloud reads its own field)"),
        cloud("Azure: `source_image_id` (each cloud reads its own field)"),
        cloud("Google Cloud: the boot disk's `image` (each cloud reads its own field)"),
        cloud("DigitalOcean: the droplet's `image` (each cloud reads its own field)"),
        cloud("Linode: the instance's `image` (each cloud reads its own field)"),
        cloud("Oracle Cloud: `source_details.source_id` (each cloud reads its own field)"),
    ],
);

/// The cloud VM modules: a cloud VM boots its image's own way.
pub const CLOUD_BOOT: &[(&str, Status)] = &boot(IMAGE_BOOTS_ITSELF, IMAGE_BOOTS_ITSELF, IMAGE_BOOTS_ITSELF, IMAGE_BOOTS_ITSELF);

/// The first machine with the field set, as a flow mapping (`field` without `machines.*.`).
pub fn probe(field: &str) -> Option<&'static str> {
    Some(match field {
        "vm.image.box_url" => "{ vm: { image: { vagrant: debian/bookworm64, box_url: 'https://boxes.example/debian.box' } } }",
        "vm.image.box_server_url" => "{ vm: { image: { vagrant: debian/bookworm64, box_server_url: 'https://boxes.example' } } }",
        "vm.image.proxmox" => "{ vm: { image: { proxmox: '9000' } } }",
        "vm.image.aws" => "{ vm: { image: { aws: ami-0123456789abcdef0 } } }",
        "vm.image.azure" => "{ vm: { image: { azure: /subscriptions/s/resourceGroups/g/providers/Microsoft.Compute/images/i } } }",
        "vm.image.gcp" => "{ vm: { image: { gcp: projects/p/global/images/i } } }",
        "vm.image.digitalocean" => "{ vm: { image: { digitalocean: '123456' } } }",
        "vm.image.linode" => "{ vm: { image: { linode: private/123 } } }",
        "vm.image.oci" => "{ vm: { image: { oci: ocid1.image.oc1..example } } }",
        "vm.boot.kernel" => "{ vm: { boot: { kernel: /boot/vmlinuz } } }",
        "vm.boot.initrd" => "{ vm: { boot: { kernel: /boot/vmlinuz, initrd: /boot/initrd.img } } }",
        "vm.boot.cmdline" => "{ vm: { boot: { kernel: /boot/vmlinuz, cmdline: console=ttyS0 } } }",
        "vm.boot.dtb" => "{ vm: { boot: { kernel: /boot/vmlinuz, dtb: /boot/board.dtb } } }",
        "vm.boot.loader" => "{ vm: { boot: { loader: /usr/share/OVMF/OVMF_CODE.fd } } }",
        "vm.boot.nvram" => "{ vm: { boot: { loader: /usr/share/OVMF/OVMF_CODE.fd, nvram: /var/lib/a_VARS.fd } } }",
        "vm.boot.firmware" => "{ vm: { boot: { firmware: uefi } } }",
        "vm.boot.firmware_format" => "{ vm: { boot: { firmware: uefi, firmware_format: raw } } }",
        "vm.boot.order" => "{ vm: { boot: { order: [hd, network] } } }",
        "vm.boot.network.tftp_root" => "{ vm: { boot: { order: [network, hd], network: { tftp_root: /srv/tftp } } } }",
        "vm.boot.network.file" => "{ vm: { boot: { order: [network, hd], network: { file: pxelinux.0 } } } }",
        "vm.boot.network.server" => "{ vm: { boot: { order: [network, hd], network: { file: pxelinux.0, server: 10.20.0.5 } } } }",
        _ => return None,
    })
}
