//! The settings of vagrant-qemu, for Apple Silicon Macs.

use super::{HOST_TUNING, IMAGE_BOOTS, RESOURCES, TOOLING, USER_SETUP};
use crate::{
    Support,
    Support::{Emitted, Partial, Planned},
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
        "disk_resize" => Planned {
            note: "machines.*.resources.disk_gb, once the box's own size is known: the plugin makes the disk an overlay of that size, and one below the box's truncates it",
        },
        "advanced_network" | "net_mode" | "socket_opts" => Partial {
            from: "machines.*.networks",
            gap: "without root, QEMU links exactly two VMs per network, each VM on one network (`refusal_on` with `qemu` says when a spec doesn't fit)",
        },
        "socket_vmnet_client" | "socket_vmnet_socket" | "vmnet_interface" | "tap_device" | "mcast_addr" | "extra_netdev_args" => Planned {
            note: "more than two VMs on a QEMU network (vmnet or socket_vmnet, which need root on the Mac)",
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
