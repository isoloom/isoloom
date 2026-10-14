//! The options of the communicators: `config.ssh`, `config.winrm` and `config.winssh`. They say
//! how Vagrant reaches a VM, so most are tooling; the ones Isoloom writes come from the box or a
//! jump host.

use super::{COMMUNICATORS, pairs, table};
use crate::{
    Format, Support,
    Support::{Emitted, Tooling},
};

/// The "Vagrant: communicators" table.
pub(super) fn format() -> Format {
    let keys = pairs(COMMUNICATORS)
        .into_iter()
        .map(|(kind, o)| {
            let s = match kind {
                "ssh" => ssh(o),
                "winrm" => winrm(o),
                "winssh" => winssh(o),
                _ => None,
            };
            (format!("config.{kind}.{o}"), s)
        })
        .collect();
    table(
        "Vagrant: communicators",
        "Vagrant 2.4.9 (the SSH, WinRM and Windows OpenSSH config classes)",
        keys,
    )
}

/// The OpenSSH communicator of a Windows box (`vm.image.winssh`): its account and its shell are
/// Isoloom's; the rest is how Vagrant connects.
fn winssh(o: &str) -> Option<Support> {
    match o {
        "username" | "password" => Some(Emitted {
            from: "machines.*.vm.image.winssh (the Windows box's own account)",
        }),
        "shell" => Some(Emitted {
            from: "machines.*.vm.image.winssh (PowerShell runs the provisioners' scripts)",
        }),
        o if o == "upload_directory" || ssh(o).is_some() => Some(Tooling {
            note: "how Vagrant reaches a Windows VM over OpenSSH: not the environment's behavior",
        }),
        _ => None,
    }
}

/// Every SSH option, named so an upstream addition shows up unclassified.
const SSH_TOOLING: [&str; 30] = [
    "compression",
    "config",
    "connect_retries",
    "connect_retry_delay",
    "connect_timeout",
    "default",
    "disable_deprecated_algorithms",
    "dsa_authentication",
    "export_command_template",
    "extra_args",
    "forward_agent",
    "forward_env",
    "forward_x11",
    "guest_port",
    "host",
    "insert_key",
    "keep_alive",
    "key_type",
    "keys_only",
    "paranoid",
    "password",
    "port",
    "private_key_path",
    "pty",
    "remote_user",
    "shell",
    "ssh_command",
    "sudo_command",
    "username",
    "verify_host_key",
];

fn ssh(o: &str) -> Option<Support> {
    match o {
        "proxy_command" => Some(Emitted {
            from: "(ISOLOOM_SSH_PROXY_COMMAND, when set: the VMs reached through a jump host)",
        }),
        o if SSH_TOOLING.contains(&o) => Some(Tooling {
            note: "how Vagrant reaches the VM over SSH: not the environment's behavior",
        }),
        _ => None,
    }
}

fn winrm(o: &str) -> Option<Support> {
    Some(match o {
        "username" | "password" => Emitted {
            from: "vm.os (the Windows box's own account)",
        },
        "transport" | "basic_auth_only" => Emitted {
            from: "machines.*.vm.image.winrm (plain HTTP with basic authentication, unless the box says `ssl`)",
        },
        "max_tries" | "retry_delay" | "timeout" => Emitted {
            from: "(waits for a Windows VM's first boot, longer under emulation)",
        },
        "host" | "port" | "guest_port" | "codepage" | "execution_time_limit" | "ssl_peer_verification" => Tooling {
            note: "how Vagrant reaches the VM over WinRM: not the environment's behavior",
        },
        _ => return None,
    })
}
