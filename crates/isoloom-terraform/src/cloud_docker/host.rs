//! The Docker host every `cloud-docker` module (and `docker-vm`'s Proxmox module) shares: its
//! size, the ports it opens, its variables, its set-up over SSH and its outputs. A cloud only
//! brings its network, firewall and VM.

use isoloom_plan::{Environment, MachineShape, Step};
use isoloom_render::hcl_builder::{Block, Expression, Module, Template, list, reference, string};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header, steps};
use isoloom_spec::model::Protocol;

use crate::vm::linux::CLOUD_INIT_WAIT;
use crate::vm::module::{inputs_file, output, project_lists};
use crate::vm::provisioner::{Connection, Provisioner, Upload};
use crate::vm::{as_root, step_command};

/// Where the host writes that the environment is up (the plan's `Ready` step).
pub const READY_FILE: &str = "/var/lib/isoloom/ready";
/// The Compose command on the host: the `docker` target's file, ports published on every
/// address of the VM (its firewall decides who reaches them).
const COMPOSE: &str = "sudo -E env ISOLOOM_PUBLISH_ADDRESS=0.0.0.0 ISOLOOM_PUBLISH_FIXED=1 docker compose -f .isoloom/docker/compose.yml";
/// Seconds `docker compose up --wait` waits for every container to be healthy.
const WAIT_TIMEOUT: u32 = 900;

/// What the host needs: every container's at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sizing {
    /// CPUs: one per container unless it says, at least 2, at most 16.
    pub cpus: u32,
    /// Memory in MB: 512 per container unless it says, plus 1 GB for Docker itself.
    pub memory_mb: u32,
    /// Disk in GB: 5 per container unless it says, at least 20.
    pub disk_gb: u32,
}

impl Sizing {
    /// The host's size for the environment's containers.
    pub fn of(env: &Environment) -> Sizing {
        let containers = env.machines.values().filter(|m| m.shape == MachineShape::Container);
        let (mut cpus, mut memory_mb, mut disk_gb) = (0u32, 1024u32, 0u32);
        for m in containers {
            cpus += m.size.cpus.unwrap_or(1);
            memory_mb += m.size.memory_mb.unwrap_or(512);
            disk_gb += m.size.disk_gb.unwrap_or(5);
        }
        Sizing {
            cpus: cpus.clamp(2, 16),
            memory_mb,
            disk_gb: disk_gb.max(20),
        }
    }

    /// One of a cloud's three VM sizes: up to 4 GB of memory, up to 8 GB, more.
    pub fn pick<T: Copy>(&self, sizes: [T; 3]) -> T {
        match self.memory_mb {
            0..=4096 => sizes[0],
            4097..=8192 => sizes[1],
            _ => sizes[2],
        }
    }
}

/// The ports the containers publish over `protocol`, sorted, each once.
pub fn published(env: &Environment, protocol: Protocol) -> Vec<u16> {
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

/// The ports as strings (`["22", "8080"]`), for the firewalls that take them so.
pub fn port_strings(ports: &[u16]) -> Expression {
    list(ports.iter().map(|p| string(p.to_string())))
}

/// How Terraform reaches the host.
#[derive(Debug, Clone)]
pub struct Reach {
    /// Its address.
    pub host: Expression,
    /// The SSH user (it has `sudo`).
    pub user: &'static str,
    /// What replaces its set-up when it changes (the VM's id).
    pub replace_on: Expression,
    /// Whether the set-up schedules the auto-stop (else the VM's user data does).
    pub auto_stop: bool,
}

/// The module's first lines: the header, then how to start and stop it.
pub fn preamble(dir: &str, cloud: &str, settings: &str) -> String {
    let chdir = format!("terraform -chdir=.isoloom/{dir}/{cloud}");
    format!(
        "{}# Start:  {chdir} init && {chdir} apply \\\n#           {settings}-var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   {chdir} destroy (same variables)\n",
        header("#")
    )
}

/// The variables every host has: vm::cloud's (who may reach it, the SSH key, the auto-stop,
/// the end date, the inputs), without `allowed_cidr` off a cloud (Proxmox: its firewall is the
/// operator's network).
pub fn variables(env: &Environment, cloud: bool) -> Vec<Block> {
    crate::vm::cloud::variables(env)
        .into_iter()
        .filter(|b| cloud || !b.labels().iter().any(|l| l.as_str() == "allowed_cidr"))
        .collect()
}

/// The host's set-up (`terraform_data.environment`), through the VM core's `remote-exec`
/// transport: the project's archive and the inputs file uploaded, then the commands.
pub fn setup(env: &Environment, reach: &Reach) -> Block {
    let mut uploads = vec![Upload::Project {
        id: "environment".into(),
        destination: steps::PROJECT_ARCHIVE,
    }];
    let inputs = &env.spec().inputs;
    if !inputs.is_empty() {
        uploads.push(Upload::Content {
            content: inputs_file(inputs),
            destination: steps::INPUTS_FILE.into(),
        });
    }
    Provisioner {
        name: "environment".into(),
        replace_on: reach.replace_on.clone(),
        connection: Connection::Ssh {
            host: reach.host.clone(),
            user: reach.user.into(),
        },
        uploads,
        parts: vec![commands(env, reach.auto_stop)],
        depends_on: Vec::new(),
    }
    .block()
}

/// Once cloud-init is done: the auto-stop, the plan's `Prelude` (the constraints) and
/// `Project` steps, Docker, the Compose file started, then the `Ready` step.
fn commands(env: &Environment, auto_stop_here: bool) -> Vec<Expression> {
    let mut cmds: Vec<Expression> = vec![string("set -e"), string(CLOUD_INIT_WAIT)];
    if auto_stop_here {
        let shutdown = Template::text("sudo shutdown -h +")
            .with_expr(reference("var.auto_stop_minutes"))
            .with_text(" >/dev/null 2>&1");
        cmds.push(crate::vm::cloud::auto_stop(shutdown.into_expression(), string("true")));
    }
    let root = |step: &Step| step_command(env, "", step, true).map(string);
    cmds.extend(root(&Step::Prelude));
    cmds.extend(root(&Step::Project));
    cmds.push(string(as_root(steps::INSTALL_DOCKER, true)));
    let inputs = if env.spec().inputs.is_empty() {
        String::new()
    } else {
        format!("set -a; . {}; set +a; ", steps::INPUTS_FILE)
    };
    cmds.push(string(format!("cd {} && {inputs}{}", steps::PROJECT_DIR, start(env))));
    cmds.extend(root(&Step::Ready));
    cmds
}

/// Starts the Compose file: everything up and healthy, then the one-shot jobs nothing waits for
/// (see `isoloom_compose::docker::start_commands`).
fn start(env: &Environment) -> String {
    let jobs = isoloom_compose::docker::leaf_jobs(env.spec());
    isoloom_compose::docker::start_commands(COMPOSE, &jobs, Some(WAIT_TIMEOUT))
}

/// The host's set-up, then its outputs: the end of every module.
pub fn environment(env: &Environment, reach: &Reach, m: &mut Module) {
    m.commented("The environment, over SSH: the project, Docker, then the Compose file.", setup(env, reach));
    m.blocks(outputs(reach));
}

/// `ip`, `ssh_user` and `ready_file`: what `isoloom test` and `connect` read.
pub fn outputs(reach: &Reach) -> Vec<Block> {
    vec![
        output("ip", reach.host.clone()),
        output("ssh_user", string(reach.user)),
        output("ready_file", string(READY_FILE)),
    ]
}

/// The module's file at `.isoloom/<dir>/<cloud>/main.tf`, the project's lists (`.isoloomignore`)
/// at its end.
pub fn file(m: &Module, dir: &str, cloud: &str) -> Result<GeneratedFile, isoloom_render::hcl_builder::Error> {
    let mut m = m.clone();
    m.fragment(&project_lists(true));
    Ok(GeneratedFile {
        path: format!("{OUTPUT_DIR}/{dir}/{cloud}/main.tf"),
        contents: m.render()?,
    })
}
