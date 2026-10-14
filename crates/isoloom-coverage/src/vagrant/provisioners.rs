//! The options of `config.vm.provision`: the ones every provisioner takes (`*`), then each
//! provisioner's own, by its real name (`chef_solo`, `puppet_server`...).
//!
//! Chef, Puppet, Salt and CFEngine options configure that tool, not the machine: the machine is
//! the same whichever tool set it up, and a `.sh` step runs the tool with its own configuration.
//! So they are tooling, while the provisioners themselves are rows of the "Vagrant" table.

use super::{PROVISIONERS, pairs, table};
use crate::{
    Format, Support,
    Support::{Emitted, Equivalent, Planned, Tooling},
};

/// The "Vagrant: provisioners" table.
pub(super) fn format() -> Format {
    let mut keys: Vec<(String, Option<Support>)> = pairs(PROVISIONERS)
        .into_iter()
        .map(|(kind, o)| (format!("config.vm.provision {kind}: {o}"), classify(kind, o)))
        .collect();
    // Isoloom's own provisioner: which generated files go with the project.
    for o in ["generated", "extra"] {
        keys.push((
            format!("config.vm.provision isoloom_project: {o}"),
            Some(Emitted {
                from: "(the generated files a step needs, such as the checks, copied with the project)",
            }),
        ));
    }
    table(
        "Vagrant: provisioners",
        "Vagrant 2.4.9 (each provisioner's config class, with the classes it inherits from)",
        keys,
    )
}

const RUNS_IT: Support = Tooling {
    note: "how Vagrant runs the step: not the machine's state",
};
const OTHER_TOOL: Support = Tooling {
    note: "an option of that tool: a `.sh` step runs it with its own configuration",
};

fn classify(kind: &str, o: &str) -> Option<Support> {
    match kind {
        "*" => common(o),
        "shell" => shell(o),
        "file" => Some(Equivalent {
            via: "`isoloom_project` copies the project into each VM",
        }),
        "ansible" | "ansible_local" => ansible(o),
        "docker" | "podman" | "container" => containers(o),
        "chef_solo" | "chef_zero" | "chef_client" | "chef_apply" | "puppet" | "puppet_server" | "salt" | "cfengine" => Some(OTHER_TOOL),
        _ => None,
    }
}

fn common(o: &str) -> Option<Support> {
    Some(match o {
        "name" => Emitted {
            from: "(each step's name, shown as it runs)",
        },
        "run" => Emitted {
            from: "(`never` for the checks, run only when asked)",
        },
        "before" | "after" => Equivalent {
            via: "the order of machines.*.vm.provision (steps run as listed)",
        },
        "type" | "preserve_order" | "communicator_required" | "id" | "config" => RUNS_IT,
        _ => return None,
    })
}

fn shell(o: &str) -> Option<Support> {
    Some(match o {
        "inline" => Emitted {
            from: "machines.*.vm.provision, and Isoloom's own steps",
        },
        "path" => Emitted {
            from: "(the checks script, run from the project)",
        },
        "env" => Emitted {
            from: "machines.*.inputs (from the host's environment)",
        },
        "reboot" => Emitted {
            from: "machines.*.vm.provision (`reboot`)",
        },
        "name" => Equivalent {
            via: "`*: name`, the same label",
        },
        "args" => Equivalent {
            via: "machines.*.inputs (values reach a script as environment variables)",
        },
        "privileged" => Tooling {
            note: "steps run as root, Vagrant's default",
        },
        "binary"
        | "keep_color"
        | "upload_path"
        | "reset"
        | "sensitive"
        | "powershell_args"
        | "powershell_elevated_interactive"
        | "md5"
        | "sha1"
        | "sha256"
        | "sha384"
        | "sha512" => RUNS_IT,
        _ => return None,
    })
}

fn ansible(o: &str) -> Option<Support> {
    let via = match o {
        "playbook" => "provision[].ansible",
        "inventory_path" => "provision[].inventory",
        "groups" => "provision[].groups",
        "host_vars" => "provision[].host_vars",
        "extra_vars" => "provision[].vars",
        "galaxy_role_file" | "galaxy_roles_path" | "galaxy_command" => "provision[].requirements",
        "become" | "become_user" | "sudo" | "sudo_user" => "the playbook's own `become`",
        "limit" => "the playbook's own `hosts`",
        "config_file" => "an `ansible.cfg` beside the playbook, in the project",
        _ => return ansible_rest(o),
    };
    Some(Equivalent { via })
}

fn ansible_rest(o: &str) -> Option<Support> {
    Some(match o {
        "vault_password_file" | "ask_vault_pass" => Planned {
            note: "an Ansible vault's password as a secret, with secrets on the VM targets",
        },
        "tags" | "skip_tags" | "start_at_task" => Tooling {
            note: "running part of a playbook: a debugging aid",
        },
        "verbose"
        | "compatibility_mode"
        | "version"
        | "install"
        | "install_mode"
        | "pip_args"
        | "pip_install_cmd"
        | "playbook_command"
        | "provisioning_path"
        | "tmp_path"
        | "show_deprecation_info"
        | "raw_arguments"
        | "raw_ssh_args"
        | "host_key_checking"
        | "force_remote_user"
        | "ask_become_pass"
        | "ask_sudo_pass" => Tooling {
            note: "how Vagrant installs and runs Ansible",
        },
        _ => return None,
    })
}

fn containers(o: &str) -> Option<Support> {
    Some(match o {
        "images" | "pull_images" | "build_image" | "run" => Equivalent {
            via: "the docker-vm output (containers on one VM, from the Compose file)",
        },
        "post_install_provision" | "post_install_provisioner" | "kubic" => Tooling {
            note: "how Vagrant installs the container engine",
        },
        _ => return None,
    })
}
