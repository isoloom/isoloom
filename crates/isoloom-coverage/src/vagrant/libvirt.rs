//! The settings of vagrant-libvirt.

use super::{ARCH, DATA_DISKS, DEVICES, DISK, HOST_TUNING, MORE_DISKS, RESOURCES, TOOLING, USER_SETUP};
use crate::{
    Support,
    Support::{Emitted, Equivalent, Tooling},
};

pub(super) fn classify(s: &str) -> Option<Support> {
    Some(match s {
        "cpus" | "memory" => RESOURCES,
        "machine_arch" => ARCH,
        "machine_virtual_size" => DISK,
        "storage" => DATA_DISKS,
        "disks" => MORE_DISKS,
        "nested" => Emitted {
            from: "machines.*.nested_virtualization",
        },
        "tpm_model" | "tpm_type" | "tpm_version" => Emitted {
            from: "machines.*.vm.tpm (Windows 11's by default): an emulated TPM 2.0",
        },
        "tpm_path" => Tooling {
            note: "the host's own TPM device, passed through: the emulated TPM gives each VM one of its own",
        },
        "boot" => Emitted {
            from: "machines.*.vm.boot.order (one `boot` per device, in order)",
        },
        "boot_order" => Equivalent {
            via: "`boot`: each call adds its device to the order",
        },
        "kernel" => Emitted {
            from: "machines.*.vm.boot.kernel",
        },
        "cmd_line" => Emitted {
            from: "machines.*.vm.boot.cmdline",
        },
        "initrd" => Emitted {
            from: "machines.*.vm.boot.initrd",
        },
        "dtb" => Emitted {
            from: "machines.*.vm.boot.dtb",
        },
        "loader" => Emitted {
            from: "machines.*.vm.boot.loader",
        },
        "nvram" => Emitted {
            from: "machines.*.vm.boot.nvram",
        },
        "uri" | "driver" | "host" | "port" | "connect_via_ssh" | "socket" | "username" | "password" | "id_ssh_key_file" | "proxy_command" | "system_uri"
        | "qemu_use_session" | "storage_pool_name" | "storage_pool_path" | "snapshot_pool_name" | "emulator_path" => USER_SETUP,
        "default_prefix" => Emitted {
            from: "name (the environment's, so labs' domains don't collide)",
        },
        "forward_ssh_port"
        | "random_hostname"
        | "title"
        | "description"
        | "uuid"
        | "autostart"
        | "suspend_mode"
        | "mgmt_attach"
        | "host_device_exclude_prefixes"
        | "qemu_use_agent"
        | "volume_cache" => TOOLING,
        s if s.starts_with("management_network_") => Tooling {
            note: "Vagrant's management network, used to provision (like the NAT interface elsewhere)",
        },
        _ => return hardware(s),
    })
}

/// Devices and hypervisor tuning: the domain's virtual hardware, which other targets don't expose.
fn hardware(s: &str) -> Option<Support> {
    Some(match s {
        s if s.starts_with("graphics_") || s.starts_with("video_") => DEVICES,
        "keymap" | "sound_type" | "input" | "inputs" | "channel" | "channels" | "serial" | "serials" | "usb" | "usbs" | "usb_controller" | "usbctl_dev"
        | "redirdev" | "redirdevs" | "redirfilter" | "redirfilters" | "smartcard" | "smartcard_dev" | "pci" | "pcis" | "rng" | "random" | "watchdog"
        | "watchdog_dev" | "cdroms" | "floppies" => DEVICES,
        s if s.starts_with("cpu") || s.starts_with("clock_") || s.starts_with("memballoon_") || s.starts_with("disk_") => HOST_TUNING,
        "clock_timer"
        | "shares"
        | "nodeset"
        | "numa_nodes"
        | "memory_backing"
        | "memorybacking"
        | "memtune"
        | "memtunes"
        | "features"
        | "features_hyperv"
        | "hyperv_feature"
        | "kvm_hidden"
        | "launchsecurity"
        | "launchsecurity_data"
        | "machine_type"
        | "qemu_args"
        | "qemuargs"
        | "qemu_env"
        | "qemuenv"
        | "sysinfo"
        | "nic_model_type"
        | "nic_adapter_count" => HOST_TUNING,
        _ => return None,
    })
}
