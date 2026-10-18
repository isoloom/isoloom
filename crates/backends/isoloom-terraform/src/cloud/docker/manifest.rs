//! The `cloud-docker` run manifest (`.isoloom/cloud-docker/<cloud>/run.json`): Terraform makes
//! one VM, and the Isothrum runner runs the environment's containers inside it, as `docker-vm`
//! does on a server. The containers and the commands that start, check and read them are the
//! shared Compose-in-a-VM pieces (`isoloom_render::run_manifest::guest`); this module adds the
//! Terraform lifecycle, the cloud's credentials and the VM.
//!
//! Terraform keeps only the infrastructure: no SSH from it, no project in the module. Once the
//! apply is done, the runner installs Docker and copies the project into the VM over SSH
//! (`into_vm`), then starts the Compose file there, the launch's values and the containers'
//! secrets carried in over SSH (`SendEnv`). The published ports answer at the VM's address, and
//! the runner reads the containers inside it (`Manifest.guest`).

use std::collections::BTreeSet;

use isoloom_manifest::{
    Checks, Condition, Driver, Error as ManifestError, Guest, HostOs, Identity, Machine, Manifest, ManifestVersion, Name, Output, OutputRole, Placeholder,
    RelPath, Resources, Role, Run, SCHEMA_URL, SnakeName, StateOwner, Step, Template, TerraformIdentity,
};
use isoloom_plan::Environment;
use isoloom_render::container::HostSize;
use isoloom_render::run_manifest::guest::{GuestCompose, into_vm, ssh_run};
use isoloom_render::run_manifest::{environment, generator, outputs_on_their_hosts};

use super::super::Cloud;
use super::super::manifest::{self, connection, steps};

/// The host VM, as the manifest names it: a machine of its own (the containers run in it).
const HOST: &str = "isoloom-docker-host";

/// The host's address (`${outputs.ip}`): where SSH reaches it and its containers' ports answer.
const IP: &str = "ip";

/// How long the runner retries the first SSH while the fresh VM is still booting, in seconds.
const SSH_BOOT_WAIT_S: u32 = 300;

/// The run manifest of `env`'s containers on `cloud`, run in one VM whose module `driver` writes;
/// `guest` is the Compose-in-a-VM plan (`isoloom_compose::guest`). Its message is left to the
/// caller.
pub(crate) fn manifest(env: &Environment, cloud: Cloud, driver: &super::Driver, guest: &GuestCompose) -> Result<Manifest, ManifestError> {
    let ip = Template::from(Placeholder::Output(SnakeName::parse(IP)?));
    let way_in = ssh_run(ip.clone(), driver.user);
    let up = up_steps(&way_in, guest)?;
    let guest_machines = guest.machines_at(&ip);
    let mut machines = vec![host(env)?];
    machines.extend(guest_machines.iter().cloned());
    let runners = guest.runners.iter().map(|r| r.runner(&way_in)).collect::<Result<Vec<_>, _>>()?;
    let setup = guest.check_setup.iter().map(|c| c.step(&way_in)).collect::<Result<Vec<_>, _>>()?;
    let mut params = connection::params(&driver_variables(env, driver), cloud)?;
    params.extend(guest.params.iter().cloned());
    let mut secrets = connection::slots(cloud, connection::terraform_steps(false))?;
    secrets.extend(guest.slots.iter().cloned());
    let mut ready = vec![Condition::StepOk {
        step: isoloom_manifest::StepId::parse("apply")?,
        means: Some("the Docker host exists and booted".into()),
    }];
    ready.extend(guest.ready.iter().cloned());
    let outputs = docker_outputs(&guest_machines)?;
    Ok(Manifest {
        schema: Some(SCHEMA_URL.to_owned()),
        manifest_version: ManifestVersion,
        generator: generator()?,
        environment: environment(env)?,
        target: isoloom_manifest::Target::CloudDocker,
        cloud: Some(manifest::cloud_of(cloud)),
        driver: Driver::Terraform,
        hosts: BTreeSet::from([HostOs::Linux, HostOs::Macos, HostOs::Windows]),
        tools: manifest::tools()?,
        identity: Identity::Terraform(TerraformIdentity {
            module: RelPath::parse(".")?,
            state: StateOwner::Runner,
            var_files: Vec::new(),
        }),
        params,
        secrets,
        networks: Vec::new(),
        machines,
        outputs,
        lifecycle: steps::terraform_lifecycle(up)?,
        ready,
        checks: Checks {
            protocol: isoloom_manifest::CheckProtocol::V1,
            setup,
            runners,
            teardown: Vec::new(),
        },
        recovery: manifest::recovery()?,
        expiry: manifest::expiry()?,
        guest: Some(Guest {
            machine: Name::parse(HOST)?,
            host: Some(Template::from(Placeholder::Output(SnakeName::parse(IP)?))),
            status: guest.status.step(&way_in)?,
        }),
    })
}

/// The `up` steps: Terraform's lock, init and apply, then the runner installs Docker and copies
/// the project into the VM over SSH (`into_vm`) and starts the Compose file there.
fn up_steps(way_in: &dyn Fn(&Template) -> Result<Run, ManifestError>, guest: &GuestCompose) -> Result<Vec<Step>, ManifestError> {
    let mut up = vec![
        steps::clear_lock()?,
        steps::init_step()?,
        steps::apply_step("apply", "Creating the Docker host in the cloud…")?,
    ];
    let mut into = into_vm(way_in, &RelPath::parse("setup.sh")?, "../../..")?;
    boot_wait(&mut into)?;
    up.extend(into);
    for command in &guest.start {
        up.push(command.step(way_in)?);
    }
    Ok(up)
}

/// The outputs: the host's address and SSH user, then each container's published ports on the
/// host (`outputs_on_their_hosts`).
fn docker_outputs(guest_machines: &[Machine]) -> Result<Vec<Output>, ManifestError> {
    let mut outputs = vec![
        state_output(
            IP,
            OutputRole::Address,
            "The Docker host's address: SSH reaches it there, and its containers' published ports answer there.",
        )?,
        state_output("ssh_user", OutputRole::Info, "The user the runner's key lets in on the Docker host.")?,
    ];
    outputs.extend(outputs_on_their_hosts(guest_machines)?);
    Ok(outputs)
}

/// The first SSH step into the fresh VM retries while ssh can't connect yet (the VM is still
/// booting): `with.wait_s`. Later steps run once it answered.
fn boot_wait(steps: &mut [isoloom_manifest::Step]) -> Result<(), ManifestError> {
    if let Some(first) = steps.first_mut()
        && let isoloom_manifest::Run::Action { with, .. } = &mut first.run
    {
        with.insert(SnakeName::parse("wait_s")?, Template::text(&SSH_BOOT_WAIT_S.to_string()));
    }
    Ok(())
}

/// The host VM, sized for every container at once: Isoloom's own machine (`infra`), where the
/// containers run.
fn host(env: &Environment) -> Result<Machine, ManifestError> {
    let size = HostSize::of(env);
    Ok(Machine {
        name: Name::parse(HOST)?,
        role: Role::Infra,
        of: None,
        os: None,
        windows: false,
        arch: None,
        resources: Some(Resources {
            cpus: Some(size.cpus),
            memory_mb: Some(size.memory_mb),
        }),
        addresses: std::collections::BTreeMap::new(),
        keep_running: false,
        on_demand: false,
        native: None,
        volumes: Vec::new(),
        ports: Vec::new(),
    })
}

/// A Terraform output of the module's state.
fn state_output(name: &str, role: OutputRole, what: &str) -> Result<Output, ManifestError> {
    crate::module::manifest::state_output(name, role, what)
}

/// The module's variables, for the params: the cloud's own and its size, then the operator's
/// (who may reach the host, the runner's key, the expiry). The launch's values reach the
/// containers over SSH, not a module variable, so `inputs` is left out; a cloud-docker host has
/// internet, so there is no seal.
fn driver_variables(env: &Environment, driver: &super::Driver) -> Vec<crate::module::variable::Variable> {
    let mut v = (driver.variables)();
    if let Some(super::SizeVariable { name, default }) = driver.size {
        v.push(crate::module::variable::Variable::new(name, "string").default(isoloom_render::hcl_builder::string(default(HostSize::of(env)))));
    }
    v.extend(super::super::operator::host_reaching());
    v
}
