//! The Docker host every `cloud-docker` module (and `docker-vm`'s Proxmox module) shares: its
//! size, the ports it opens, its variables, its set-up over SSH and its outputs. A cloud only
//! brings its network, firewall and VM.

use isoloom_plan::{Environment, MachineShape, Step};
use isoloom_render::hcl_builder::{BinaryOperator, Block, Expression, Module, Template, binary, call, conditional, list, number, object, reference, string};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header, steps};
use isoloom_spec::model::Protocol;

use crate::vm::linux::CLOUD_INIT_WAIT;
use crate::vm::module::{Variable, inputs_file, inputs_variable, output, project_lists};
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

/// The variables every host has: who may reach it (on a cloud), the SSH key, the auto-stop, the
/// end date, and the inputs when the environment takes any.
pub fn variables(env: &Environment, cloud: bool) -> Vec<Block> {
    let mut v = Vec::new();
    if cloud {
        v.push(
            Variable::new("allowed_cidr", "string")
                .describe("Who may reach the VM (SSH and the published ports), e.g. your IP/32")
                .block(),
        );
    }
    v.extend([
        Variable::new("ssh_public_key", "string").block(),
        Variable::new("ssh_private_key_file", "string")
            .describe("The private key of ssh_public_key: Terraform copies the project over SSH")
            .block(),
        Variable::new("auto_stop_minutes", "number")
            .default(number(0))
            .describe("Shut the VM down after this many minutes (0: never). Destroy still ends the billing of disks and addresses")
            .block(),
        Variable::new("expires_at", "string")
            .default(string(""))
            .describe("When the environment should end, in Unix seconds (empty: no end), as a tag on every resource so a reaper can find what to destroy")
            .block(),
    ]);
    if !env.spec().inputs.is_empty() {
        v.push(inputs_variable());
    }
    v
}

/// The environment's random id (`terraform_data.id`): two copies of it in one account don't
/// collide.
pub fn id() -> Block {
    let id = call("substr", [call("replace", [call("uuid", []), string("-"), string("")]), number(0), number(8)]);
    Block::builder("resource")
        .add_label("terraform_data")
        .add_label("id")
        .add_attribute(("input", id))
        .add_block(
            Block::builder("lifecycle")
                .add_attribute(("ignore_changes", list([reference("input")])))
                .build(),
        )
        .build()
}

/// `locals`: the resources' name, the project's root, then a cloud's own.
pub fn locals(env: &Environment, extra: Vec<(&str, Expression)>) -> Block {
    let name = Template::text(format!("isoloom-{}-", env.name)).with_expr(reference("terraform_data.id.output"));
    let root = call(
        "abspath",
        [Template::new().with_expr(reference("path.module")).with_text("/../../..").into_expression()],
    );
    let mut b = Block::builder("locals")
        .add_attribute(("name", name.into_expression()))
        .add_attribute(("root", root));
    for (k, v) in extra {
        b = b.add_attribute((k, v));
    }
    b.build()
}

/// The tags (labels) a reaper finds the environment's resources by.
pub fn tags(env: &Environment) -> Expression {
    object([
        ("isoloom-environment", string(env.name.as_str())),
        ("managed-by", string("isoloom")),
        ("isoloom-instance", reference("local.name")),
        ("isoloom-expires-at", reference("var.expires_at")),
    ])
}

/// `var.auto_stop_minutes > 0 ? <yes> : <no>`.
pub fn auto_stop(yes: Expression, no: Expression) -> Expression {
    conditional(binary(reference("var.auto_stop_minutes"), BinaryOperator::Greater, number(0)), yes, no)
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
        cmds.push(auto_stop(shutdown.into_expression(), string("true")));
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
