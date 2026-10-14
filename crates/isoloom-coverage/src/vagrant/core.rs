//! The "Vagrant" table: `config.vm.*`, the network and provisioner types, and the other
//! top-level blocks (`config.ssh`, `config.winrm`, `config.push`...). Their options have tables
//! of their own.

use super::{DISK, MORE_DISKS, NETWORKS, NO_SHARED_FOLDERS, PROVISIONERS, PUSH, TOOLING, USER_SETUP, VM, kinds, names, pairs, table};
use crate::{
    Format, Support,
    Support::{Emitted, Equivalent, NotPortable, Tooling},
};

/// The "Vagrant" table.
pub(super) fn format() -> Format {
    let mut keys: Vec<(String, Option<Support>)> = names(VM).into_iter().map(|s| (format!("config.vm.{s}"), machine(s))).collect();
    keys.extend(kinds(NETWORKS).into_iter().map(|k| (format!("config.vm.network {k}"), network(k))));
    keys.extend(
        kinds(PROVISIONERS)
            .into_iter()
            .filter(|k| *k != "*")
            .map(|k| (format!("config.vm.provision {k}"), provisioner(k))),
    );
    keys.push(("config.vm.provision isoloom_project".to_string(), provisioner("isoloom_project")));
    keys.extend(BLOCKS.iter().map(|(k, s)| ((*k).to_string(), Some(*s))));
    keys.extend(kinds(PUSH).into_iter().map(|k| (format!("config.push {k}"), Some(PUSHES))));
    keys.extend(pairs(PUSH).into_iter().map(|(k, o)| (format!("config.push {k}: {o}"), Some(PUSHES))));
    table("Vagrant", "Vagrant 2.4.9 (config.vm, network types, provisioners, communicators, pushes)", keys)
}

/// A push publishes the project somewhere (an FTP server, Heroku): nothing to do with the VMs.
const PUSHES: Support = Tooling {
    note: "`vagrant push` publishes the project elsewhere: not the environment's behavior",
};

/// The top-level blocks besides `config.vm`.
const BLOCKS: [(&str, Support); 6] = [
    (
        "config.ssh",
        Emitted {
            from: "(a jump host from ISOLOOM_SSH_PROXY_COMMAND, when set; see the communicators' table)",
        },
    ),
    (
        "config.winrm",
        Emitted {
            from: "vm.os (the Windows box's own account, over WinRM)",
        },
    ),
    (
        "config.winssh",
        Emitted {
            from: "machines.*.vm.image.winssh (a Windows box reached over OpenSSH)",
        },
    ),
    (
        "config.trigger",
        Emitted {
            from: "(Hyper-V: each network's private switch made before a VM starts and removed after the last one, its adapters attached and addressed, its virtual TPM enabled)",
        },
    ),
    ("config.vagrant", TOOLING),
    ("config.push", PUSHES),
];

fn network(kind: &str) -> Option<Support> {
    Some(match kind {
        "private_network" => Emitted {
            from: "networks and machines.*.networks",
        },
        "forwarded_port" => Emitted {
            from: "machines.*.services[].publish (on the host's loopback; `protocol: \"udp\"` with `protocol: udp`)",
        },
        "public_network" => Emitted {
            from: "networks.*.bridged (the host's LAN, through the interface it names; not on QEMU, Hyper-V or libvirt yet)",
        },
        _ => return None,
    })
}

/// Another configuration tool runs from a script step; Isoloom doesn't install it.
const ANY_TOOL: Support = Equivalent {
    via: "a `.sh` provisioning step runs the tool (the script installs it)",
};
const CONTAINERS: Support = Equivalent {
    via: "`docker:` (containers come from the Compose output; VMs run services natively)",
};

fn provisioner(kind: &str) -> Option<Support> {
    Some(match kind {
        "shell" => Emitted {
            from: "machines.*.vm.provision (.sh, .yml, .yaml, `reboot`), and Isoloom's own steps",
        },
        "file" => Equivalent {
            via: "`isoloom_project` (below) copies the project; a file provisioner follows symlinks and lists files when the Vagrantfile loads",
        },
        "isoloom_project" => Emitted {
            from: "(Isoloom's own provisioner, defined in the Vagrantfile: the project into each VM as a tar.gz built when the step runs, symlinks kept as links)",
        },
        "ansible_local" => Equivalent {
            via: "a shell provisioner running the playbook with `ansible-playbook -c local` inside the VM, the machine as its inventory (as on every VM target)",
        },
        "ansible" => Equivalent {
            via: "the same playbooks, run inside the VM by a shell provisioner",
        },
        "chef_solo" | "chef_zero" | "chef_client" | "chef_apply" | "puppet" | "puppet_server" | "salt" | "cfengine" => ANY_TOOL,
        "docker" | "podman" | "container" => CONTAINERS,
        _ => return None,
    })
}

fn machine(s: &str) -> Option<Support> {
    Some(match s {
        "define" => Emitted { from: "machines" },
        "box" => Emitted {
            from: "machines.*.vm.os (built-in boxes for every OS name), or machines.*.vm.image.vagrant",
        },
        "hostname" => Emitted { from: "the machine's name" },
        "host_name" => Tooling {
            note: "old name of `hostname`",
        },
        "boot_timeout" => Emitted { from: "(fixed: 10 minutes)" },
        "synced_folder" => Emitted {
            from: "(the default `/vagrant` share turned off: the project is copied into each VM instead)",
        },
        "network" => Emitted {
            from: "networks (see the network types below)",
        },
        "provision" => Emitted {
            from: "machines.*.vm.provision (see the provisioners below)",
        },
        "provider" => Emitted {
            from: "machines.*.resources (see each provider below)",
        },
        "disk" => DISK,
        "disks" => MORE_DISKS,
        "box_architecture" => Emitted { from: "machines.*.arch" },
        "box_version" => Emitted {
            from: "machines.*.vm.image.vagrant_version, or a built-in pin",
        },
        "box_url" => Emitted {
            from: "machines.*.vm.image.box_url (a URL, or a .box file in the project)",
        },
        "box_server_url" => Emitted {
            from: "machines.*.vm.image.box_server_url",
        },
        "communicator" | "guest" => Emitted {
            from: "vm.os (Windows uses WinRM, plain HTTP or SSL per the box's `image.winrm`)",
        },
        _ => return machine_rest(s),
    })
}

/// The settings that are neither written nor planned: other means, or Vagrant's own business.
fn machine_rest(s: &str) -> Option<Support> {
    Some(match s {
        "cloud_init" | "cloud_init_configs" | "cloud_init_first_boot_only" => Equivalent {
            via: "provisioning steps run any setup",
        },
        "base_mac" | "base_address" => NotPortable {
            why: "cloud VMs get their MAC and addresses from the provider",
        },
        "usable_port_range" => Tooling {
            note: "how Vagrant picks host ports for forwarded ports",
        },
        "provisioners" => Tooling {
            note: "Vagrant's own list behind `provision`",
        },
        "clone" => Tooling {
            note: "starts from another Vagrant machine; machines start from their image",
        },
        "allow_fstab_modification" | "allowed_synced_folder_types" => NO_SHARED_FOLDERS,
        "allow_hosts_modification" => Tooling {
            note: "Isoloom writes /etc/hosts itself (names of the other machines)",
        },
        "post_up_message" | "graceful_halt_timeout" | "ignore_box_vagrantfile" => TOOLING,
        s if s.starts_with("box_") => USER_SETUP,
        _ => return None,
    })
}
