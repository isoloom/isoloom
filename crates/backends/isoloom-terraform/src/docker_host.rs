//! The Docker host: one VM running the environment's containers with the `docker` target's
//! Compose file, on a cloud (`cloud-docker`) or a Proxmox server (`docker-vm`). Every module of
//! it shares its size (`isoloom_render::container::HostSize`), the ports it opens, its set-up over SSH and its outputs; a platform only
//! brings its network, firewall and VM.

use isoloom_plan::{Environment, MachineShape, Step};
use isoloom_render::hcl_builder::{Block, Expression, Module, Template, list, reference, string};
use isoloom_render::steps;
use isoloom_spec::model::Protocol;

use crate::cloud::auto_stop;
use crate::machine::Shell;
use crate::machine::linux::CLOUD_INIT_WAIT;
use crate::machine::provisioner::{Connection, Provisioner, Upload};
use crate::module::{inputs, output};

/// The Compose command on the host: the `docker` target's file, ports published on every
/// address of the VM (its firewall decides who reaches them).
const COMPOSE: &str = "sudo -E env ISOLOOM_PUBLISH_ADDRESS=0.0.0.0 ISOLOOM_PUBLISH_FIXED=1 docker compose -f .isoloom/docker/compose.yml";
/// Seconds `docker compose up --wait` waits for every container to be healthy.
const WAIT_TIMEOUT: u32 = 900;

/// The ports the containers publish over `protocol`, sorted, each once.
pub(crate) fn published(env: &Environment, protocol: Protocol) -> Vec<u16> {
    let mut ports: Vec<u16> = env
        .machines
        .values()
        .filter(|m| m.shape == MachineShape::Container)
        .flat_map(|m| &m.ports)
        .filter(|p| p.protocol == protocol)
        .map(|p| p.publish)
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
}

/// SSH, then the ports the containers publish over TCP: what a firewall taking one list of TCP
/// ports opens.
pub(crate) fn tcp_ports(env: &Environment) -> Vec<u16> {
    let mut tcp = vec![22];
    tcp.extend(published(env, Protocol::Tcp));
    tcp
}

/// The ports as strings (`["22", "8080"]`), for the firewalls that take them so.
pub(crate) fn port_strings(ports: &[u16]) -> Expression {
    list(ports.iter().map(|p| string(p.to_string())))
}

/// Where the host's auto-stop is scheduled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoStop {
    /// In its user data, at first boot (the cloud runs it).
    UserData,
    /// In its set-up, over SSH.
    SetUp,
}

/// How Terraform reaches the host.
#[derive(Debug, Clone)]
pub(crate) struct Access {
    /// Its address.
    pub(crate) host: Expression,
    /// The SSH user (it has `sudo`).
    pub(crate) user: &'static str,
    /// What replaces its set-up when it changes (the VM's id).
    pub(crate) replace_on: Expression,
    /// Where its auto-stop is scheduled.
    pub(crate) auto_stop: AutoStop,
}

/// The host's set-up (`terraform_data.environment`), through the VM core's `remote-exec`
/// transport: the project's archive and the inputs file uploaded, then the commands.
pub(crate) fn setup(env: &Environment, access: &Access) -> Block {
    let mut uploads = vec![Upload::Project {
        id: "environment".to_owned(),
        destination: steps::PROJECT_ARCHIVE,
    }];
    let spec_inputs = &env.spec().inputs;
    if !spec_inputs.is_empty() {
        uploads.push(Upload::Content {
            content: inputs::file(spec_inputs),
            destination: steps::INPUTS_FILE.to_owned(),
        });
    }
    Provisioner {
        name: "environment".to_owned(),
        replace_on: access.replace_on.clone(),
        connection: Connection::Ssh {
            host: access.host.clone(),
            user: access.user.to_owned(),
        },
        uploads,
        parts: vec![commands(env, access.auto_stop)],
        depends_on: Vec::new(),
        announce: (!isoloom_render::secrets::of_containers(env).is_empty()).then(|| "environment".to_owned()),
    }
    .block()
}

/// Once cloud-init is done: the auto-stop, the plan's `Prelude` (the constraints) and
/// `Project` steps, Docker, the Compose file started (once `isoloom run` delivered the
/// containers' secrets), then the `Ready` step.
fn commands(env: &Environment, auto_stop: AutoStop) -> Vec<Expression> {
    let mut cmds: Vec<Expression> = vec![string("set -e"), string(CLOUD_INIT_WAIT)];
    if auto_stop == AutoStop::SetUp {
        let shutdown = Template::text("sudo shutdown -h +")
            .with_expr(reference("var.auto_stop_minutes"))
            .with_text(" >/dev/null 2>&1");
        cmds.push(auto_stop::if_set(shutdown.into_expression(), string("true")));
    }
    let root = |step: &Step| Shell::Sudo.step(env, "", step).map(string);
    cmds.extend(root(&Step::Prelude));
    cmds.extend(root(&Step::Project));
    cmds.push(string(Shell::Sudo.command(steps::INSTALL_DOCKER)));
    let inputs = if env.spec().inputs.is_empty() {
        String::new()
    } else {
        format!("set -a; . {}; set +a; ", steps::INPUTS_FILE)
    };
    let files = isoloom_render::container::host_environment(env);
    let up = format!("cd {} && {inputs}{files}{}", steps::PROJECT_DIR, start(env));
    // The containers' secrets are root's on the host: read, then Compose started, as root.
    let secrets = isoloom_render::secrets::of_containers(env);
    if secrets.is_empty() {
        cmds.push(string(up));
    } else {
        cmds.push(string(
            Shell::Sudo.command(&format!("{}{up}", isoloom_render::secrets::host_environment(env.spec(), &secrets))),
        ));
    }
    cmds.extend(root(&Step::Ready));
    cmds
}

/// Starts the Compose file: everything up and healthy, then the one-shot jobs nothing waits for
/// (see `isoloom_render::container::start_commands`).
fn start(env: &Environment) -> String {
    let jobs = isoloom_render::container::leaf_jobs(env.spec());
    isoloom_render::container::start_commands(COMPOSE, &jobs, Some(WAIT_TIMEOUT))
}

/// The host's set-up, then its outputs: the end of every module.
pub(crate) fn environment(env: &Environment, access: &Access, m: &mut Module) {
    m.commented("The environment, over SSH: the project, Docker, then the Compose file.", setup(env, access));
    m.blocks(outputs(access));
}

/// `ip`, `ssh_user` and `ready_file`: what `isoloom test` and `connect` read.
pub(crate) fn outputs(access: &Access) -> Vec<Block> {
    vec![
        output("ip", access.host.clone()),
        output("ssh_user", string(access.user)),
        output("ready_file", string(crate::machine::cloud_init::READY_FILE)),
    ]
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Protocol;

    use super::*;
    use crate::cloud::docker::tests::{SPEC, env};

    #[test]
    fn published_ports_are_sorted_once_per_protocol() {
        let e = env(SPEC);
        assert_eq!(published(&e, Protocol::Tcp), [8080, 15432]);
        assert_eq!(published(&e, Protocol::Udp), [5353]);
        assert_eq!(tcp_ports(&e), [22, 8080, 15432]);
    }
}
