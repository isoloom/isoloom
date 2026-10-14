//! Every key of the cloud-config format (cloud-init's own schema, listed in
//! `coverage/cloud-init.txt` by `scripts/refresh-cloud-init.py`), and what Isoloom does with it.
//! Isoloom writes a cloud-config document for every Proxmox VM (each machine, the router and the
//! controller) and for docker-vm's Docker host on Proxmox. The clouds hand their VMs a shell
//! script as user data instead (the auto-stop timer), so they write no cloud-config key: their
//! set-up runs over SSH.
//!
//! The nested keys listed are those of the items Isoloom writes (`users`, `write_files` and
//! `power_state`), so a field it leaves out of an item is a row too.

use super::{
    Format, Support,
    Support::{Emitted, Equivalent, InImage, NotPortable, Planned, Tooling},
};

const TOOLING: Support = Tooling {
    note: "cloud-init's own behavior (which modules run, how documents merge, what it logs), not the machine's",
};
const DEPRECATED: Support = Tooling {
    note: "a deprecated spelling of a key in this table",
};
const OS_SERVICE: Support = InImage {
    note: "one OS's own service or tool: set it in the image or the machine's provisioning (`vm.provision`)",
};
const PROVISIONING: Support = InImage {
    note: "set it in the machine's provisioning (`vm.provision`) or its image",
};
const SSH_ACCESS: Support = Tooling {
    note: "SSH access mechanics: Isoloom reaches its machines as its own user, with its own key",
};
const MIRRORS: Support = Equivalent {
    via: "constraints (the prelude points apt and dnf at the proxy and the mirrors)",
};
const DATA_DISKS: Support = Planned {
    note: "data disks, partitioned and formatted (Wave 3: persistent volumes and data disks)",
};
const USER_FIELD: Support = Tooling {
    note: "Isoloom's own user: it needs a name, sudo, a shell and the key, nothing more",
};
const POWER_FIELD: Support = Tooling {
    note: "when cloud-init restarts the machine: right away, once it is done, is what the steps need",
};
const IMAGE_USER: Support = PROVISIONING;
const FILE_FIELD: Support = Tooling {
    note: "how a file is written: Isoloom sets the path, mode, encoding and content it needs",
};

/// The cloud-init table.
pub fn format() -> Format {
    Format {
        name: "cloud-init",
        file: ".isoloom/proxmox/main.tf and .isoloom/docker-vm/proxmox/main.tf (each VM's user data)",
        source: "cloud-init 26.2's schema-cloud-config-v1.json (crates/isoloom-coverage/coverage/cloud-init.txt)",
        rows: ROWS.iter().map(|(k, s)| ((*k).to_string(), *s)).collect(),
        collapse_not_portable: false,
    }
}

/// Every key, in the list's order: the top level, then the items' keys. Data, not code: one row a
/// key.
const ROWS: &[(&str, Support)] = &[
    ("allow_public_ssh_keys", SSH_ACCESS),
    (
        "ansible",
        Equivalent {
            via: "provision[].ansible (the controller runs the playbooks)",
        },
    ),
    ("apk_repos", OS_SERVICE),
    ("apt", MIRRORS),
    ("apt_pipelining", TOOLING),
    ("apt_reboot_if_required", DEPRECATED),
    ("apt_update", DEPRECATED),
    ("apt_upgrade", DEPRECATED),
    ("authkey_hash", SSH_ACCESS),
    (
        "autoinstall",
        NotPortable {
            why: "Ubuntu's installer: the targets boot ready-made images",
        },
    ),
    (
        "bootcmd",
        Emitted {
            from: "constraints (the prelude, at every boot)",
        },
    ),
    ("byobu_by_default", OS_SERVICE),
    ("ca-certs", DEPRECATED),
    ("ca_certs", PROVISIONING),
    ("chef", OS_SERVICE),
    ("chpasswd", SSH_ACCESS),
    ("cloud_config_modules", TOOLING),
    ("cloud_final_modules", TOOLING),
    ("cloud_init_modules", TOOLING),
    ("create_hostname_file", TOOLING),
    ("device_aliases", DATA_DISKS),
    (
        "disable_ec2_metadata",
        NotPortable {
            why: "EC2's metadata service: AWS only",
        },
    ),
    ("disable_root", SSH_ACCESS),
    ("disable_root_opts", SSH_ACCESS),
    ("disk_setup", DATA_DISKS),
    (
        "drivers",
        NotPortable {
            why: "NVIDIA drivers: most targets have no GPU to pass through",
        },
    ),
    ("fan", OS_SERVICE),
    ("final_message", TOOLING),
    (
        "fqdn",
        Planned {
            note: "a machine's full name, from its name and `dns.domain`",
        },
    ),
    ("fs_setup", DATA_DISKS),
    ("groups", PROVISIONING),
    (
        "growpart",
        Equivalent {
            via: "machines.*.resources.disk_gb (the disk-grow step)",
        },
    ),
    ("grub-dpkg", DEPRECATED),
    ("grub_dpkg", OS_SERVICE),
    ("hostname", Emitted { from: "machines" }),
    ("keyboard", PROVISIONING),
    ("landscape", OS_SERVICE),
    ("launch-index", TOOLING),
    ("locale", PROVISIONING),
    ("locale_configfile", PROVISIONING),
    ("lxd", OS_SERVICE),
    (
        "manage_etc_hosts",
        Equivalent {
            via: "machines and machines.*.aliases (the hosts step names every machine)",
        },
    ),
    (
        "manage_resolv_conf",
        Equivalent {
            via: "machines.*.dns (Proxmox's own cloud-init DNS settings)",
        },
    ),
    ("mcollective", OS_SERVICE),
    ("merge_how", TOOLING),
    ("merge_type", TOOLING),
    ("migrate", TOOLING),
    ("mount_default_fields", TOOLING),
    (
        "mounts",
        Equivalent {
            via: "volumes (a shared volume is mounted from /etc/fstab)",
        },
    ),
    ("no_ssh_fingerprints", SSH_ACCESS),
    (
        "ntp",
        Equivalent {
            via: "constraints.ntp (the prelude sets the time servers)",
        },
    ),
    ("output", TOOLING),
    ("package_reboot_if_required", PROVISIONING),
    ("package_update", PROVISIONING),
    ("package_upgrade", PROVISIONING),
    (
        "packages",
        Emitted {
            from: "(the tools the steps need: nftables, curl, the guest agent...)",
        },
    ),
    ("password", SSH_ACCESS),
    ("phone_home", TOOLING),
    (
        "power_state",
        Emitted {
            from: "machines.*.vm.provision (a reboot step)",
        },
    ),
    ("prefer_fqdn_over_hostname", TOOLING),
    ("preserve_hostname", TOOLING),
    ("puppet", OS_SERVICE),
    ("random_seed", TOOLING),
    ("reporting", TOOLING),
    (
        "resize_rootfs",
        Equivalent {
            via: "machines.*.resources.disk_gb (the disk-grow step)",
        },
    ),
    (
        "resolv_conf",
        Equivalent {
            via: "machines.*.dns (Proxmox's own cloud-init DNS settings)",
        },
    ),
    ("rh_subscription", OS_SERVICE),
    (
        "rpi",
        NotPortable {
            why: "Raspberry Pi hardware: no VM target has it",
        },
    ),
    ("rsyslog", PROVISIONING),
    (
        "runcmd",
        Emitted {
            from: "machines.*.vm.provision (every step, in order)",
        },
    ),
    ("salt_minion", OS_SERVICE),
    ("snap", OS_SERVICE),
    ("spacewalk", OS_SERVICE),
    ("ssh", SSH_ACCESS),
    (
        "ssh_authorized_keys",
        Equivalent {
            via: "users.*.ssh_authorized_keys (Isoloom's user's keys)",
        },
    ),
    ("ssh_deletekeys", SSH_ACCESS),
    ("ssh_fp_console_blacklist", SSH_ACCESS),
    ("ssh_genkeytypes", SSH_ACCESS),
    ("ssh_import_id", SSH_ACCESS),
    ("ssh_key_console_blacklist", SSH_ACCESS),
    ("ssh_keys", SSH_ACCESS),
    ("ssh_publish_hostkeys", SSH_ACCESS),
    ("ssh_pwauth", SSH_ACCESS),
    ("ssh_quiet_keygen", SSH_ACCESS),
    ("swap", PROVISIONING),
    ("system_info", TOOLING),
    ("timezone", PROVISIONING),
    ("ubuntu_advantage", DEPRECATED),
    ("ubuntu_pro", OS_SERVICE),
    ("updates", TOOLING),
    ("user", USER_FIELD),
    (
        "users",
        Emitted {
            from: "(Isoloom's user, with the operator's and the controller's keys)",
        },
    ),
    ("vendor_data", TOOLING),
    ("version", TOOLING),
    ("wireguard", PROVISIONING),
    (
        "write_files",
        Emitted {
            from: "machines.*.vm.provision (the project's files, the router's and the steps' files)",
        },
    ),
    ("yum_repo_dir", MIRRORS),
    ("yum_repos", MIRRORS),
    ("zypper", OS_SERVICE),
    // The items of `users`.
    ("users.*.create-groups", DEPRECATED),
    ("users.*.create_groups", IMAGE_USER),
    ("users.*.doas", IMAGE_USER),
    ("users.*.expiredate", IMAGE_USER),
    ("users.*.gecos", IMAGE_USER),
    ("users.*.groups", IMAGE_USER),
    ("users.*.hashed-passwd", DEPRECATED),
    ("users.*.hashed_passwd", IMAGE_USER),
    ("users.*.homedir", IMAGE_USER),
    ("users.*.inactive", IMAGE_USER),
    ("users.*.lock-passwd", DEPRECATED),
    ("users.*.lock_passwd", IMAGE_USER),
    ("users.*.name", Emitted { from: "(`isoloom`)" }),
    ("users.*.no-create-home", DEPRECATED),
    ("users.*.no-log-init", DEPRECATED),
    ("users.*.no-user-group", DEPRECATED),
    ("users.*.no_create_home", IMAGE_USER),
    ("users.*.no_log_init", IMAGE_USER),
    ("users.*.no_user_group", IMAGE_USER),
    ("users.*.passwd", IMAGE_USER),
    ("users.*.plain-text-passwd", DEPRECATED),
    ("users.*.plain_text_passwd", IMAGE_USER),
    ("users.*.primary-group", DEPRECATED),
    ("users.*.primary_group", IMAGE_USER),
    ("users.*.selinux-user", DEPRECATED),
    ("users.*.selinux_user", IMAGE_USER),
    ("users.*.shell", Emitted { from: "(bash)" }),
    ("users.*.snapuser", OS_SERVICE),
    ("users.*.ssh-authorized-keys", DEPRECATED),
    ("users.*.ssh-import-id", DEPRECATED),
    ("users.*.ssh-redirect-user", DEPRECATED),
    (
        "users.*.ssh_authorized_keys",
        Emitted {
            from: "(the operator's key, and the controller's)",
        },
    ),
    ("users.*.ssh_import_id", SSH_ACCESS),
    ("users.*.ssh_redirect_user", SSH_ACCESS),
    (
        "users.*.sudo",
        Emitted {
            from: "(passwordless sudo: the steps run as root)",
        },
    ),
    ("users.*.system", IMAGE_USER),
    ("users.*.uid", IMAGE_USER),
    // The items of `write_files`.
    ("write_files.*.append", FILE_FIELD),
    (
        "write_files.*.content",
        Emitted {
            from: "machines.*.vm.provision (the project's files)",
        },
    ),
    ("write_files.*.defer", FILE_FIELD),
    (
        "write_files.*.encoding",
        Emitted {
            from: "(base64, for the project's files)",
        },
    ),
    ("write_files.*.owner", FILE_FIELD),
    (
        "write_files.*.path",
        Emitted {
            from: "machines.*.vm.provision (the project's files)",
        },
    ),
    (
        "write_files.*.permissions",
        Emitted {
            from: "(the inputs file and the reboot scripts)",
        },
    ),
    ("write_files.*.source", FILE_FIELD),
    // `power_state`.
    (
        "power_state.condition",
        Emitted {
            from: "(only while a reboot step has a part left)",
        },
    ),
    ("power_state.delay", POWER_FIELD),
    ("power_state.message", Emitted { from: "(the reboot step)" }),
    ("power_state.mode", Emitted { from: "(reboot)" }),
    ("power_state.timeout", POWER_FIELD),
];
