//! The settings of vagrant-qemu, for Apple Silicon Macs.

use super::{HOST_TUNING, IMAGE_BOOTS, RESOURCES, TOOLING, USER_SETUP};
use crate::{
    Support,
    Support::{Emitted, Equivalent, Planned, Tooling},
};

pub(super) fn classify(s: &str) -> Option<Support> {
    Some(match s {
        "memory" => RESOURCES,
        "smp" => Emitted {
            from: "machines.*.resources.cpus",
        },
        "arch" => Emitted {
            from: "machines.*.arch (emulated when the host's CPU is another)",
        },
        "disk_resize" => Emitted {
            from: "machines.*.resources.disk_gb, above the box's own size only (read from the box with qemu-img): the disk is an overlay of the box's, and a smaller one would cut it",
        },
        "advanced_network" | "net_mode" | "socket_opts" => Emitted {
            from: "machines.*.networks: two VMs as a listen/connect pair, any other number through a hub (a QEMU process with no machine), no root; a VM is on one network (`refusal_on` with `qemu` refuses one on more)",
        },
        "mcast_addr" => Equivalent {
            via: "socket_opts to the network's hub: any number of VMs without root, on a Mac too, where multicast doesn't send",
        },
        "tap_device" | "socket_vmnet_client" | "socket_vmnet_socket" => Equivalent {
            via: "socket_opts to the network's hub: the same network between the VMs, without root (a TAP device and the socket_vmnet daemon need it)",
        },
        "vmnet_interface" => Planned {
            note: "with `public_network` (a network bridged to the outside); vmnet-bridged needs root on the Mac",
        },
        "extra_netdev_args" => Tooling {
            note: "options of the NAT adapter Vagrant reaches the VM through (user-mode networking), not of the lab's networks",
        },
        "ssh_auto_correct" => Emitted {
            from: "(always: each VM forwards SSH from its own host port)",
        },
        "machine" | "cpu" | "net_device" | "drive_interface" | "extra_drive_args" | "extra_image_opts" | "extra_qemu_args" => HOST_TUNING,
        "firmware_format" => IMAGE_BOOTS,
        "image_path" | "qemu_bin" | "qemu_dir" | "default_qemu_dir" | "homebrew_prefix" => USER_SETUP,
        "control_port" | "debug_port" | "ssh_host" | "ssh_port" | "no_daemonize" | "other_default" | "graceful_timeout" => TOOLING,
        _ => return None,
    })
}
