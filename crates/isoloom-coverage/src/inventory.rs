//! Every behavioral inventory parameter of Ansible (the host variables that change how Ansible
//! reaches or runs on a host, listed in `coverage/ansible-inventory.txt` by
//! `scripts/refresh-ansible-inventory.py`), and what Isoloom's one inventory does with it. The
//! inventory is the controller's `/etc/isoloom/inventory.ini` on Vagrant, Proxmox and the clouds,
//! and `.isoloom/external/inventory.ini` on `external`; each target reaches its hosts its own way
//! (`isoloom_render::inventory::Access`).
//!
//! The rule: a parameter is a machine feature when it changes what runs on the machine (which
//! Python, which privilege escalation); how the controller talks to the host (SSH and WinRM
//! tuning, another transport) is Ansible's tooling. Any parameter can still be set per host with
//! `provision[].host_vars`, which goes on the host's line.

use super::{
    Format, Support,
    Support::{Emitted, Equivalent, Partial, Tooling},
};

const ALIAS: Support = Tooling {
    note: "another spelling of a parameter in this table (an older or a method-specific one)",
};
const SSH_TUNING: Support = Tooling {
    note: "how the controller's SSH client connects: Isoloom's defaults reach every machine",
};
const WINRM_TUNING: Support = Tooling {
    note: "how the controller's WinRM client connects: Isoloom's defaults reach every Windows machine",
};
const PSRP: Support = Tooling {
    note: "PSRP, another Windows transport: Isoloom reaches Windows over WinRM or SSH",
};
const ANSIBLE: Support = Tooling {
    note: "Ansible's own behavior on the controller, not the machine's",
};
const HOST_VARS: Support = Equivalent {
    via: "provision[].host_vars (any variable, on the host's line)",
};
const SUDO: Support = Equivalent {
    via: "(passwordless sudo as root: Isoloom's user needs no other method, user or password)",
};
const WINDOWS_ACCOUNT: &str = "Proxmox's Windows VMs (no WinRM account yet)";

/// The Ansible inventory table.
pub fn format() -> Format {
    Format {
        name: "Ansible inventory",
        file: "/etc/isoloom/inventory.ini on the controller, .isoloom/external/inventory.ini",
        source: "ansible-core v2.21.5's behavioral parameters, and pywinrm v0.5.0's for `ansible_winrm_*` (crates/isoloom-coverage/coverage/ansible-inventory.txt)",
        rows: ROWS.iter().map(|(k, s)| ((*k).to_string(), *s)).collect(),
        collapse_not_portable: false,
    }
}

/// Every parameter, in the list's order. Data, not code: one row a parameter.
const ROWS: &[(&str, Support)] = &[
    ("ansible_admin_users", HOST_VARS),
    ("ansible_async_dir", HOST_VARS),
    (
        "ansible_become",
        Emitted {
            from: "(every Linux host: the steps and playbooks run as root)",
        },
    ),
    ("ansible_become_exe", SUDO),
    ("ansible_become_flags", SUDO),
    ("ansible_become_method", SUDO),
    ("ansible_become_pass", ALIAS),
    ("ansible_become_password", SUDO),
    ("ansible_become_user", SUDO),
    ("ansible_common_remote_group", HOST_VARS),
    (
        "ansible_connection",
        Partial {
            from: "machines.*.vm.os (WinRM for Windows; SSH with PowerShell on external)",
            gap: WINDOWS_ACCOUNT,
        },
    ),
    ("ansible_connection_user", ANSIBLE),
    ("ansible_control_path", SSH_TUNING),
    ("ansible_control_path_dir", SSH_TUNING),
    ("ansible_deprecation_warnings", ANSIBLE),
    ("ansible_docker_extra_args", ANSIBLE),
    ("ansible_facts_modules", ANSIBLE),
    (
        "ansible_host",
        Emitted {
            from: "machines (the first address; `external.address` on external)",
        },
    ),
    (
        "ansible_host_key_checking",
        Equivalent {
            via: "(the controller's ANSIBLE_HOST_KEY_CHECKING=False: the machines are new)",
        },
    ),
    ("ansible_inject_invocation", ANSIBLE),
    ("ansible_interpreter_python_fallback", HOST_VARS),
    ("ansible_local_become_strip_preamble", ANSIBLE),
    ("ansible_local_become_success_timeout", ANSIBLE),
    ("ansible_module_compression", ANSIBLE),
    ("ansible_network_os", ANSIBLE),
    ("ansible_no_target_syslog", HOST_VARS),
    (
        "ansible_password",
        Partial {
            from: "(the Vagrant boxes' account; the clouds' Windows password)",
            gap: WINDOWS_ACCOUNT,
        },
    ),
    (
        "ansible_pipelining",
        Equivalent {
            via: "(the controller's ANSIBLE_PIPELINING=True)",
        },
    ),
    (
        "ansible_port",
        Emitted {
            from: "machines.*.external.port and machines.*.vm.image.winrm",
        },
    ),
    ("ansible_private_key", SSH_TUNING),
    ("ansible_private_key_file", ALIAS),
    ("ansible_private_key_passphrase", SSH_TUNING),
    ("ansible_psrp_auth", PSRP),
    ("ansible_psrp_ca_cert", PSRP),
    ("ansible_psrp_cert_trust_path", PSRP),
    ("ansible_psrp_cert_validation", PSRP),
    ("ansible_psrp_certificate_key_password", PSRP),
    ("ansible_psrp_certificate_key_pem", PSRP),
    ("ansible_psrp_certificate_pem", PSRP),
    ("ansible_psrp_configuration_name", PSRP),
    ("ansible_psrp_connection_backoff", PSRP),
    ("ansible_psrp_connection_timeout", PSRP),
    ("ansible_psrp_credssp_auth_mechanism", PSRP),
    ("ansible_psrp_credssp_disable_tlsv1_2", PSRP),
    ("ansible_psrp_credssp_minimum_version", PSRP),
    ("ansible_psrp_host", PSRP),
    ("ansible_psrp_ignore_proxy", PSRP),
    ("ansible_psrp_max_envelope_size", PSRP),
    ("ansible_psrp_message_encryption", PSRP),
    ("ansible_psrp_negotiate_delegate", PSRP),
    ("ansible_psrp_negotiate_hostname_override", PSRP),
    ("ansible_psrp_negotiate_send_cbt", PSRP),
    ("ansible_psrp_negotiate_service", PSRP),
    ("ansible_psrp_no_profile", PSRP),
    ("ansible_psrp_operation_timeout", PSRP),
    ("ansible_psrp_path", PSRP),
    ("ansible_psrp_port", PSRP),
    ("ansible_psrp_protocol", PSRP),
    ("ansible_psrp_proxy", PSRP),
    ("ansible_psrp_read_timeout", PSRP),
    ("ansible_psrp_reconnection_backoff", PSRP),
    ("ansible_psrp_reconnection_retries", PSRP),
    ("ansible_psrp_user", PSRP),
    ("ansible_python_interpreter", HOST_VARS),
    ("ansible_python_module_rlimit_nofile", HOST_VARS),
    ("ansible_remote_tmp", HOST_VARS),
    ("ansible_runas_flags", ALIAS),
    ("ansible_runas_pass", ALIAS),
    ("ansible_runas_user", ALIAS),
    ("ansible_scp_executable", SSH_TUNING),
    ("ansible_scp_extra_args", SSH_TUNING),
    ("ansible_sftp_batch_mode", SSH_TUNING),
    ("ansible_sftp_executable", SSH_TUNING),
    ("ansible_sftp_extra_args", SSH_TUNING),
    ("ansible_shell_allow_world_readable_temp", HOST_VARS),
    ("ansible_shell_executable", HOST_VARS),
    (
        "ansible_shell_type",
        Emitted {
            from: "(PowerShell, for Windows hosts over SSH on external)",
        },
    ),
    ("ansible_ssh_args", SSH_TUNING),
    ("ansible_ssh_common_args", SSH_TUNING),
    ("ansible_ssh_executable", SSH_TUNING),
    ("ansible_ssh_extra_args", SSH_TUNING),
    ("ansible_ssh_host", ALIAS),
    ("ansible_ssh_host_key_checking", ALIAS),
    ("ansible_ssh_pass", ALIAS),
    ("ansible_ssh_password", ALIAS),
    ("ansible_ssh_password_mechanism", SSH_TUNING),
    ("ansible_ssh_pipelining", ALIAS),
    ("ansible_ssh_pkcs11_provider", SSH_TUNING),
    ("ansible_ssh_port", ALIAS),
    ("ansible_ssh_private_key", ALIAS),
    (
        "ansible_ssh_private_key_file",
        Emitted {
            from: "machines.*.external.key (the controller's own key elsewhere)",
        },
    ),
    ("ansible_ssh_private_key_passphrase", ALIAS),
    ("ansible_ssh_retries", SSH_TUNING),
    ("ansible_ssh_timeout", ALIAS),
    ("ansible_ssh_transfer_method", SSH_TUNING),
    ("ansible_ssh_use_tty", SSH_TUNING),
    ("ansible_ssh_user", ALIAS),
    ("ansible_ssh_verbosity", SSH_TUNING),
    ("ansible_sshpass_prompt", SSH_TUNING),
    ("ansible_su_exe", ALIAS),
    ("ansible_su_flags", ALIAS),
    ("ansible_su_pass", ALIAS),
    ("ansible_su_prompt_l10n", ALIAS),
    ("ansible_su_user", ALIAS),
    ("ansible_sudo_chdir", ALIAS),
    ("ansible_sudo_exe", ALIAS),
    ("ansible_sudo_flags", ALIAS),
    ("ansible_sudo_pass", ALIAS),
    ("ansible_sudo_user", ALIAS),
    ("ansible_system_tmpdirs", HOST_VARS),
    ("ansible_target_log_info", ANSIBLE),
    ("ansible_timeout", SSH_TUNING),
    (
        "ansible_user",
        Partial {
            from: "machines.*.external.user and machines.*.vm.os (each image's own user; `isoloom` on Proxmox)",
            gap: WINDOWS_ACCOUNT,
        },
    ),
    ("ansible_win_async_startup_timeout", WINRM_TUNING),
    ("ansible_winrm_ca_trust_path", WINRM_TUNING),
    ("ansible_winrm_cert_key_pem", WINRM_TUNING),
    ("ansible_winrm_cert_pem", WINRM_TUNING),
    ("ansible_winrm_connection_timeout", WINRM_TUNING),
    ("ansible_winrm_credssp_disable_tlsv1_2", WINRM_TUNING),
    ("ansible_winrm_host", ALIAS),
    ("ansible_winrm_kerberos_delegation", WINRM_TUNING),
    ("ansible_winrm_kerberos_hostname_override", WINRM_TUNING),
    ("ansible_winrm_keytab", WINRM_TUNING),
    ("ansible_winrm_kinit_args", WINRM_TUNING),
    ("ansible_winrm_kinit_cmd", WINRM_TUNING),
    ("ansible_winrm_kinit_env_vars", WINRM_TUNING),
    ("ansible_winrm_kinit_mode", WINRM_TUNING),
    ("ansible_winrm_message_encryption", WINRM_TUNING),
    (
        "ansible_winrm_operation_timeout_sec",
        Emitted {
            from: "(400 s: a domain controller's promotion keeps a call waiting)",
        },
    ),
    ("ansible_winrm_pass", ALIAS),
    ("ansible_winrm_password", ALIAS),
    ("ansible_winrm_path", WINRM_TUNING),
    ("ansible_winrm_port", ALIAS),
    ("ansible_winrm_proxy", WINRM_TUNING),
    (
        "ansible_winrm_read_timeout_sec",
        Emitted {
            from: "(500 s, longer than the operation timeout)",
        },
    ),
    ("ansible_winrm_realm", WINRM_TUNING),
    (
        "ansible_winrm_scheme",
        Emitted {
            from: "machines.*.vm.image.winrm (plain HTTP on the clouds)",
        },
    ),
    ("ansible_winrm_send_cbt", WINRM_TUNING),
    (
        "ansible_winrm_server_cert_validation",
        Emitted {
            from: "(off: the machines' certificates are self-signed)",
        },
    ),
    ("ansible_winrm_service", WINRM_TUNING),
    (
        "ansible_winrm_transport",
        Emitted {
            from: "machines.*.vm.image.winrm (NTLM over HTTPS, basic over HTTP)",
        },
    ),
    ("ansible_winrm_user", ALIAS),
    ("ansible_worker_session_isolation", ANSIBLE),
];
