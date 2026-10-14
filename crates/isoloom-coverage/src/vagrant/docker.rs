//! The settings of Vagrant's Docker provider. Isoloom runs containers with Compose instead (the
//! docker, docker-vm and hybrid outputs), so a container feature here is produced there.

use super::{PROVIDER_SPECIFIC, TOOLING, USER_SETUP};
use crate::{
    Support,
    Support::{Equivalent, Planned},
};

pub(super) fn classify(s: &str) -> Option<Support> {
    let via = match s {
        "image" => "machines.*.docker.image, in the Compose output",
        "build_dir" | "git_repo" => "machines.*.docker.build, in the Compose output",
        "dockerfile" => "machines.*.docker.dockerfile, in the Compose output",
        "build_args" => "machines.*.docker.args, in the Compose output",
        "env" => "machines.*.environment, in the Compose output",
        "ports" => "machines.*.services[].publish, in the Compose output",
        "expose" => "machines.*.services[].port, in the Compose output",
        "volumes" => "machines.*.volumes, in the Compose output",
        "privileged" => "machines.*.privileged, in the Compose output",
        "link" => "networks (machines reach each other by name), in the Compose output",
        "name" => "the environment and machine names, in the Compose output",
        _ => return rest(s),
    };
    Some(Equivalent { via })
}

fn rest(s: &str) -> Option<Support> {
    Some(match s {
        "cmd" => Planned {
            note: "a machine's own command, instead of its image's",
        },
        "create_args" => PROVIDER_SPECIFIC,
        "auth_server" | "email" | "username" | "password" => USER_SETUP,
        "compose"
        | "compose_configuration"
        | "force_host_vm"
        | "host_vm_build_dir_options"
        | "vagrant_machine"
        | "vagrant_vagrantfile"
        | "has_ssh"
        | "pull"
        | "remains_running"
        | "stop_timeout" => TOOLING,
        _ => return None,
    })
}
