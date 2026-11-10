//! The `docker-vm` target: the environment's Compose file, run by Docker on one VM, in
//! `.isoloom/docker-vm/Vagrantfile` (a VM on this machine, or on an ESXi host).
//!
//! - One Debian VM, sized for every container together (and Docker itself).
//! - Docker from Docker's own repository; the project copied to /opt/isoloom, with the `docker`
//!   target's folder and the configuration files of Isoloom's outputs (no other target's, nor the
//!   tools' state); then the lab started with `.isoloom/docker/compose.yml`, the `docker` target's
//!   Compose file, rendered with it by the Compose backend: the commands Compose's backend gives
//!   ([`GuestCompose`]).
//! - When everything answers: /var/lib/isoloom/ready, for runners to poll.
//! - Checks on demand: `vagrant provision --provision-with checks` (every runner of the Compose
//!   `check` profile).
//! - Published ports: the VM forwards them from the host's loopback.
//! - The containers' host folders: each shared into the VM by its provider's own shared folder,
//!   under `/mnt/isoloom`, where the commands run there tell Compose to bind it from.
//! - The run manifest (`docker-vm/run.json`, [`manifest`]): the runner brings the VM up
//!   with every provisioner but the lab's start (`prepares`), then runs the same commands
//!   through `vagrant ssh`, the values given at launch forwarded into the VM (SSH's `SendEnv`,
//!   accepted by the VM's `sshd`), never written in it.
//! - On a Proxmox server: `.isoloom/docker-vm/proxmox/main.tf` (Terraform, bpg/proxmox), one
//!   VM on the uplink bridge, the same set-up over SSH as `cloud-docker`'s Docker host.
//! - ESXi: the vagrant-vmware-esxi provider reads the host from `ESXI_HOSTNAME`,
//!   `ESXI_USERNAME` and `ESXI_PASSWORD` (and the datastore from `ESXI_DATASTORE`).
//!
//! This is the Vagrant side of the target; `isoloom-targets` puts it together with the
//! `docker` target's Compose file (first) and the Proxmox module, and refuses what the Compose
//! file can't run: its capabilities are Compose's.

use crate::boxes;
use crate::error::Result;
use crate::settings;
use isoloom_manifest::contract::OUTPUT_DIR;
use isoloom_manifest::{Error as ManifestError, Failure, Manifest, Part, Placeholder};
use isoloom_plan::{Environment, MachineShape};
use isoloom_render::container::{self as docker, DOCKER_HOST as VM};
use isoloom_render::run_manifest::guest::{COMPOSE_OUTPUTS, GuestCommand, GuestCompose};
use isoloom_render::{GeneratedFile, header, steps};
use isoloom_spec::model::Target;
use isoloom_spec::shell::quote;

use crate::docker_host;
use crate::host_folders::Share;
use crate::ruby::{BootStatements, Call, Provision, Ruby, Value};
use crate::vagrantfile::{self, shell};
use crate::vms::values;

const DIR: &str = "docker-vm";
/// Vagrant's name for the file's one machine.
pub(crate) const DEFINE: &str = "default";
/// The provisioner letting SSH carry the values given at launch into the VM.
const ACCEPT_ENV: &str = "accept-env";

/// The run manifest of `env` on a VM Vagrant makes, Compose running in it as `guest` says (the
/// Compose backend's, [`isoloom_compose::guest`](https://docs.rs/isoloom-compose)); its message is
/// left to the caller.
///
/// # Errors
///
/// [`crate::Error::Manifest`] for a value the manifest's types refuse (a bug of this backend).
pub fn manifest(env: &Environment, guest: &GuestCompose) -> crate::Result<Manifest> {
    crate::manifest::docker_vm::manifest(env, guest).map_err(|e| e.on(Target::DockerVm))
}

/// The VM's own provisioners: everything `vagrant up` runs but the environment's start and its
/// checks (UTM's default route, a no-op elsewhere; the Docker host's set-up; SSH accepting the
/// forwarded values, when there are some), as the Vagrantfile names them.
///
/// # Errors
///
/// As [`vagrantfile`].
pub(crate) fn prepares(env: &Environment, guest: &GuestCompose) -> Result<Vec<String>> {
    Ok(written(env, guest)?.named_provisioners().to_vec())
}

/// The CPUs and memory (MB) of the VM: the containers' host's.
fn host_size(env: &Environment) -> (u32, u32) {
    let size = docker::HostSize::of(env);
    (size.cpus, size.memory_mb)
}

/// `.isoloom/docker-vm/Vagrantfile`: the VM, Docker, then the Compose file up as `guest` starts it,
/// and its checks.
///
/// # Errors
///
/// [`crate::Error::Manifest`] when a command of `guest` holds a placeholder only a runner fills in
/// (a bug of the Compose backend, which builds them).
pub fn vagrantfile(env: &Environment, guest: &GuestCompose) -> Result<GeneratedFile> {
    Ok(GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/Vagrantfile"),
        contents: written(env, guest).map_err(|e| e.on(Target::DockerVm))?.finish(),
    })
}

/// What the Vagrantfile of `env` applies to its VM only when it boots ([`BootStatements`]), as
/// its builder records it while writing it.
pub(crate) fn boot(env: &Environment, guest: &GuestCompose) -> Result<BootStatements> {
    Ok(written(env, guest)?.boot().clone())
}

/// The scripts of the provisioners that run `guest`'s commands in the VM.
struct Scripts {
    /// The environment's start.
    environment: String,
    /// The checks, when there are some.
    checks: Option<String>,
}

/// The Vagrantfile of `env`, written.
fn written(env: &Environment, guest: &GuestCompose) -> Result<Ruby> {
    let scripts = Scripts {
        environment: environment_script(guest)?,
        checks: checks_script(env, guest)?,
    };
    let mut r = Ruby::new();
    r.text(&header("#"));
    r.comment(&format!(
        "Start:  cd {OUTPUT_DIR}/{DIR} && vagrant up\nChecks: cd {OUTPUT_DIR}/{DIR} && vagrant provision --provision-with checks\nStop:   cd {OUTPUT_DIR}/{DIR} && vagrant destroy -f"
    ));
    r.blank();
    r.assign("ROOT", Value::raw("File.expand_path(\"../..\", __dir__)"));
    r.text(&crate::vms::project_rb());
    r.text(vagrantfile::PORT_CHECK_RB);
    // `IsoloomQemu.disk`, for the helper VM's disk on QEMU.
    r.text(crate::vms::qemu::QEMU_RB);
    crate::vms::utm::constant(&mut r);
    vagrantfile::inputs(&mut r, &env.input_names(), false);
    // Always read the runner's chosen host ports: the QEMU provider block below reads the helper
    // VM's own SSH forward port from it, so the hash must be defined even when no container port is.
    vagrantfile::chosen_ports(&mut r);
    r.blank();
    r.configure(|r| configure(r, env, guest, &scripts));
    Ok(r)
}

/// The VM: its box and name, the published ports, its size on every provider, its host folders,
/// then its provisioners in order: the system constraints, the project, the certificate
/// authorities, Docker, SSH accepting the values given at launch, the environment and the checks.
fn configure(r: &mut Ruby, env: &Environment, guest: &GuestCompose, scripts: &Scripts) {
    // generic/debian12 has a build for the providers here but UTM and QEMU (VirtualBox, VMware,
    // Parallels, libvirt); those two are given their own box below. Hyper-V isn't offered here.
    r.assign("config.vm.box", Value::str("generic/debian12"));
    r.assign("config.vm.hostname", Value::str(&env.name));
    r.call(
        Call::new("config.vm.synced_folder")
            .arg(Value::str("."))
            .arg(Value::str("/vagrant"))
            .kw("disabled", true),
    );
    r.assign("config.vm.boot_timeout", 600u32);
    vagrantfile::ssh_proxy(r, "config");
    let forwarded = guest.forwarded();
    if !forwarded.is_empty() {
        r.comment("`vagrant ssh` carries the values given at launch into the VM (SSH's SendEnv).");
        r.assign("config.ssh.forward_env", Value::array(forwarded.iter().map(|v| Value::str(v))));
    }
    vagrantfile::libvirt_prefix(r, "config", &env.name);
    // Each container's published port is served inside the VM at its own number; the VM forwards
    // it from a host port the runner picks (looked up by `<machine>/<port>`, defaulting to publish).
    for (name, m) in env.machines.iter().filter(|(_, m)| m.shape == MachineShape::Container) {
        for p in &m.ports {
            let host = vagrantfile::chosen_host(&format!("{name}/{}", p.name), p.publish);
            r.call(vagrantfile::forwarded_port("config", p.publish, host, p.protocol));
        }
    }
    let size = host_size(env);
    desktop_providers(r, &crate::vms::label(env, VM), size, docker::HostSize::of(env).disk_gb);
    esxi(r, &crate::vms::vm_name(env, VM), size);
    // The containers' host folders, shared read-write where Compose finds them (a read-only one
    // is bound read-only into its container).
    let shares: Vec<Share> = docker::host_folders::of_containers(env)
        .into_iter()
        .map(|f| Share {
            from: f.folder.from.clone(),
            guest: f.in_vm,
            read_only: false,
        })
        .collect();
    crate::host_folders::render(r, "config", &shares);
    crate::vms::utm::placeholder(r, "config");
    // The project, with the outputs Compose reads.
    docker_host::set_up(r, env, "config", &COMPOSE_OUTPUTS);
    if !forwarded.is_empty() {
        let accept = steps::accept_env(&forwarded);
        r.provision(Provision::named("config", ACCEPT_ENV, "shell").kw("inline", Value::str(&accept)));
    }
    docker_host::start(r, env, "config", "environment", &scripts.environment);
    if let Some(script) = &scripts.checks {
        checks(r, env, script);
    }
}

/// The size on the providers of this machine (VirtualBox, VMware, Parallels, UTM, QEMU,
/// libvirt), named `label` where the provider shows a name.
fn desktop_providers(r: &mut Ruby, label: &str, (cpus, mem): (u32, u32), disk_gb: u32) {
    let provider = |name: &str| Call::new("config.vm.provider").arg(Value::str(name));
    let named = |v: &mut Ruby| {
        v.assign("v.name", Value::str(label));
        v.assign("v.cpus", cpus);
        v.assign("v.memory", mem);
    };
    r.block(provider("virtualbox"), "v", |r| {
        // VirtualBox 7.2 can't restore its own VMSVGA saved state (`Failed to load unit 'vga'`),
        // so a suspended VM would come back cold, its containers' memory lost: this headless VM
        // has the older VBoxVGA, whose saved state it restores.
        r.call(Call::new("v.customize").arg(Value::array(
            ["modifyvm", ":id", "--graphicscontroller", "vboxvga"].map(|w| if w == ":id" { Value::raw(w) } else { Value::str(w) }),
        )));
        named(r);
    });
    r.block(provider("vmware_desktop"), "v", |r| {
        r.assign(&format!("v.vmx[{}]", Value::str("displayName")), Value::str(label));
        r.assign(&format!("v.vmx[{}]", Value::str("numvcpus")), Value::str(&cpus.to_string()));
        r.assign(&format!("v.vmx[{}]", Value::str("memsize")), Value::str(&mem.to_string()));
    });
    r.block(provider("parallels"), "v", named);
    // Apple Silicon Macs: UTM and QEMU, as for the VMs of the Vagrant target. UTM has no
    // generic/debian12 build, so it takes a UTM box of the same Debian 12.
    r.block(provider("utm"), "v, o", |r| {
        r.assign("o.vm.box", Value::str("utm/bookworm"));
        named(r);
        crate::vms::utm::override_in_block(r);
    });
    r.block(provider("qemu"), "v, o", |r| {
        // vagrant-qemu boots the Debian cloud image, as the VMs of the Vagrant target do on QEMU
        // (generic/debian12's libvirt build never comes up under it: the box is found, the VM
        // starts, SSH never answers). The main box's version pin isn't one of this box's.
        r.assign("o.vm.box", Value::str(boxes::QEMU_DEBIAN_12));
        r.assign("o.vm.box_version", Value::str(">= 0"));
        r.assign("v.smp", Value::str(&format!("cpus={cpus}")));
        r.assign("v.memory", Value::str(&format!("{mem}M")));
        // The cloud image's disk is 10 GB, little for Docker's images and the lab's volumes: an
        // overlay of the containers' host disk (`HostSize`, as cloud-docker's and Proxmox's;
        // sparse, so it costs what it holds), which the image's cloud-init
        // grows the root file system into. `IsoloomQemu.disk` keeps the box's own size when
        // that is already larger.
        r.assign(
            "v.disk_resize",
            Value::raw(format!("IsoloomQemu.disk({}, {disk_gb})", Value::str(boxes::QEMU_DEBIAN_12))),
        );
        // vagrant-qemu forwards the VM's SSH (guest 22) to one fixed host port (50022) with weak
        // auto-correct, so two local docker-vm labs — or a half-created VM left by a failed start —
        // collide on it. Take the free host port the runner chose for this VM (the way published
        // ports do), defaulting to 50022 when it set none; auto-correct stays on as a last resort.
        r.assign("v.ssh_port", vagrantfile::chosen_host(&format!("{VM}/ssh"), 50022));
        r.assign("v.ssh_auto_correct", true);
    });
    r.block(provider("libvirt"), "v", |r| {
        r.assign("v.cpus", cpus);
        r.assign("v.memory", mem);
    });
}

/// The size and placement on an ESXi host (vagrant-vmware-esxi), the host from the environment.
fn esxi(r: &mut Ruby, guest: &str, (cpus, mem): (u32, u32)) {
    r.block(Call::new("config.vm.provider").arg(Value::str("vmware_esxi")), "v", |r| {
        vagrantfile::esxi_host(r);
        r.assign(
            "v.esxi_virtual_network",
            Value::array([Value::raw(format!("{}.split(\",\").first.strip", settings::ESXI_VIRTUAL_NETWORK.fetch()))]),
        );
        r.assign("v.guest_name", Value::str(guest));
        r.assign("v.guest_numvcpus", cpus);
        r.assign("v.guest_memsize", mem);
    });
}

/// The environment's start in the VM, after the Docker host's own ([`docker_host::start`]):
/// `guest`'s start steps (published on every address of the VM, at the fixed host ports Vagrant
/// forwards), then the ready mark.
fn environment_script(guest: &GuestCompose) -> Result<String> {
    Ok(format!(
        "{}{} && echo ready > {}",
        sequence(&guest.start)?,
        steps::MAKE_STATE_DIR,
        steps::READY_FILE
    ))
}

/// `commands` as script lines, in order, each as its failure policy says: a failed one ends the
/// script, or, when tolerated (a clean-up), is said and passed.
fn sequence(commands: &[GuestCommand]) -> Result<String> {
    commands
        .iter()
        .map(|c| {
            let line = in_vm_shell(c)?;
            Ok(match c.failure {
                Failure::Fail => format!("{line} || exit 1\n"),
                Failure::Tolerate => format!("{line} || echo {} >&2\n", quote(&format!("{}: failed, tolerated", c.id))),
            })
        })
        .collect()
}

/// The command as a shell line in the VM itself, which no runner fills in (a Vagrantfile's
/// own provisioner): each `${params.<NAME>}` the environment variable `NAME`, which that
/// provisioner sets.
///
/// # Errors
///
/// [`ManifestError::PlaceholderOutOfPlace`] for any other placeholder: only a runner fills
/// those in.
fn in_vm_shell(c: &GuestCommand) -> std::result::Result<String, ManifestError> {
    c.command
        .parts()
        .iter()
        .map(|part| match part {
            Part::Text(text) => Ok(text.clone()),
            Part::Placeholder(Placeholder::Param(name)) => Ok(format!("\"${name}\"")),
            Part::Placeholder(other) => Err(ManifestError::PlaceholderOutOfPlace {
                at: format!("guest command `{}`", c.id),
                root: other.root(),
                only: "a runner's steps",
            }),
        })
        .collect()
}

/// Checks on demand (`script`), with the values given at launch (in base64, which the script
/// decodes); the derived checks switched off with `ISOLOOM_DERIVED=0` on the host.
fn checks(r: &mut Ruby, env: &Environment, script: &str) {
    let derived = crate::vagrantfile::derived_from_host();
    let inputs = &env.input_names();
    let vars = if inputs.is_empty() {
        derived
    } else {
        Value::raw(format!("{}.merge({derived})", values::encoded(inputs)))
    };
    let call = shell("config", "checks").kw("run", Value::str("never"));
    r.provision(call.kw("inline", Value::str(script)).kw("env", vars));
}

/// The checks as the run manifest runs them (`guest`'s), after the values given at launch are
/// decoded: the stand-ins set up, every runner, then the stand-ins taken down whatever the
/// runners found. None without runners.
fn checks_script(env: &Environment, guest: &GuestCompose) -> Result<Option<String>> {
    if guest.runners.is_empty() {
        return Ok(None);
    }
    let runners: String = guest
        .runners
        .iter()
        .map(|g| Ok(format!("  {} || failed=1\n", in_vm_shell(&g.command)?)))
        .collect::<Result<_>>()?;
    let setup = sequence(&guest.check_setup)?;
    let checked = if setup.is_empty() {
        runners
    } else {
        format!("if (\n{setup}); then\n{runners}else\n  failed=1\nfi\n")
    };
    Ok(Some(format!(
        "{}failed=0\n{checked}{}exit \"$failed\"",
        values::linux_decode(&env.input_names()),
        sequence(&guest.check_teardown)?
    )))
}
