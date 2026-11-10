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
use isoloom_manifest::{Failure, Manifest};
use isoloom_plan::{Environment, MachineShape};
use isoloom_render::container::{self as docker, DOCKER_HOST as VM};
use isoloom_render::run_manifest::guest::{GuestCommand, GuestCompose};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header, steps};
use isoloom_spec::model::Target;
use isoloom_spec::shell::quote;

use crate::host_folders::Share;
use crate::ruby::{BootStatements, Call, Provision, Ruby, Value};
use crate::vagrantfile::{self, shell};
use crate::vms::values;

const DIR: &str = "docker-vm";
/// Vagrant's name for the file's one machine.
pub(crate) const DEFINE: &str = "default";
/// The provisioner letting SSH carry the values given at launch into the VM.
const ACCEPT_ENV: &str = "accept-env";
/// Where that provisioner tells `sshd` which variables to accept.
const ACCEPT_ENV_FILE: &str = "/etc/ssh/sshd_config.d/isoloom-env.conf";

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

/// The VM's own provisioners, in order: everything `vagrant up` runs but the lab's start (UTM's
/// default route, a no-op elsewhere; the constraints, when the spec has some; the project; its
/// certificate authorities trusted, when the spec names some; Docker; SSH accepting the
/// forwarded values, when there are some).
pub(crate) fn prepares(env: &Environment, guest: &GuestCompose) -> Vec<&'static str> {
    let constraints = steps::linux(env, "", &isoloom_plan::Step::Prelude).map(|_| vagrantfile::CONSTRAINTS);
    let authorities = steps::trust_authorities(env).map(|_| CA_CERTIFICATES);
    let accept = (!guest.forwarded().is_empty()).then_some(ACCEPT_ENV);
    [crate::vms::utm::NAT_ROUTE]
        .into_iter()
        .chain(constraints)
        .chain([PROJECT])
        .chain(authorities)
        .chain([vagrantfile::DOCKER])
        .chain(accept)
        .collect()
}

/// The project provisioner.
const PROJECT: &str = "project";

/// The provisioner trusting the spec's certificate authorities, from the project's copy.
const CA_CERTIFICATES: &str = "ca-certificates";

/// `config.vm.provision "<name>", type: "<kind>"`: a provisioner named at the top level, which
/// `vagrant up --provision-with <name>` takes (it refuses a name given only as the shell
/// provisioner's own `name:` option, and the project provisioner has no such option).
fn named(name: &str, kind: &str) -> Provision {
    Provision::new("config", name).kw("type", Value::str(kind))
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
        environment: environment_script(env, guest)?,
        checks: checks_script(env, guest)?,
    };
    let mut r = Ruby::new();
    r.text(&header("#"));
    r.comment(&format!(
        "Start:  cd .isoloom/{DIR} && vagrant up\nChecks: cd .isoloom/{DIR} && vagrant provision --provision-with checks\nStop:   cd .isoloom/{DIR} && vagrant destroy -f"
    ));
    r.blank();
    r.assign("ROOT", Value::raw("File.expand_path(\"../..\", __dir__)"));
    r.text(&crate::vms::project_rb());
    r.text(vagrantfile::PORT_CHECK_RB);
    // `IsoloomQemu.disk`, for the helper VM's disk on QEMU.
    r.text(crate::vms::qemu::QEMU_RB);
    crate::vms::utm::constant(&mut r);
    vagrantfile::inputs(&mut r, &env.spec().inputs, false);
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
            r.call(vagrantfile::forwarded_port_chosen(
                "config",
                p.publish,
                &format!("{name}/{}", p.name),
                p.publish,
                p.protocol,
            ));
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
    if let Some(sh) = steps::linux(env, "", &isoloom_plan::Step::Prelude) {
        r.provision(named(vagrantfile::CONSTRAINTS, "shell").kw("inline", Value::str(&sh)));
    }
    // The project, with the outputs Compose reads: first, so the certificate authorities it holds
    // are trusted before Docker downloads.
    r.provision(named(PROJECT, "isoloom_project").kw("extra", compose_outputs()));
    if let Some(sh) = steps::trust_authorities(env) {
        r.provision(named(CA_CERTIFICATES, "shell").kw("inline", Value::str(&sh)));
    }
    let docker = format!("{}\n{}", steps::install_docker(), steps::PUBLISHED_FROM_OUTSIDE);
    r.provision(named(vagrantfile::DOCKER, "shell").kw("inline", Value::str(&docker)));
    if !forwarded.is_empty() {
        let accept = format!(
            "mkdir -p /etc/ssh/sshd_config.d && echo {} > {ACCEPT_ENV_FILE} && (systemctl reload ssh || systemctl reload sshd)",
            isoloom_spec::shell::quote(&format!("AcceptEnv {}", forwarded.join(" ")))
        );
        r.provision(named(ACCEPT_ENV, "shell").kw("inline", Value::str(&accept)));
    }
    environment(r, env, &scripts.environment);
    if let Some(script) = &scripts.checks {
        checks(r, env, script);
    }
}

/// What of Isoloom's outputs the VM gets with the project: the `docker` target's folder (its
/// Compose file, check scripts and builds) and the configuration files its images copy. Never
/// another target's folder, nor what the tools write beside them (a Terraform state, a cloud
/// machine's login).
fn compose_outputs() -> Value {
    let docker = format!("{OUTPUT_DIR}/{}", Target::Docker.id());
    Value::array([Value::str(&docker), Value::str(isoloom_render::configure::DIR)])
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
        r.assign("v.ssh_port", Value::raw(format!("{}.fetch(\"{VM}/ssh\", 50022)", vagrantfile::HOSTPORTS)));
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
            Value::array([Value::raw("ENV.fetch(\"ESXI_VIRTUAL_NETWORK\", \"VM Network\").split(\",\").first.strip")]),
        );
        r.assign("v.guest_name", Value::str(guest));
        r.assign("v.guest_numvcpus", cpus);
        r.assign("v.guest_memsize", mem);
    });
}

/// The Compose file up as `guest` starts it (published on every address of the VM, at the fixed
/// host ports Vagrant forwards), with the containers' secrets once they were delivered and the
/// values given at launch (in base64, which the script decodes), then the ready mark.
fn environment(r: &mut Ruby, env: &Environment, script: &str) {
    let inputs = &env.spec().inputs;
    let encoded = (!inputs.is_empty()).then(|| values::encoded(inputs));
    r.provision(shell("config", "environment").kw("inline", Value::str(script)).kw_opt("env", encoded));
}

/// The environment's start in the VM: the values given at launch decoded, the containers'
/// secrets, `guest`'s start steps, the ready mark.
fn environment_script(env: &Environment, guest: &GuestCompose) -> Result<String> {
    let secrets = isoloom_render::secrets::host_environment(env.spec(), &isoloom_render::secrets::of_containers(env));
    Ok(format!(
        "{}{secrets}{}{} && echo ready > {}",
        values::linux_decode(&env.spec().inputs),
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
            let line = c.in_vm_shell()?;
            Ok(match c.failure {
                Failure::Fail => format!("{line} || exit 1\n"),
                Failure::Tolerate => format!("{line} || echo {} >&2\n", quote(&format!("{}: failed, tolerated", c.id))),
            })
        })
        .collect()
}

/// Checks on demand (`script`), with the values given at launch (in base64, which the script
/// decodes); the derived checks switched off with `ISOLOOM_DERIVED=0` on the host.
fn checks(r: &mut Ruby, env: &Environment, script: &str) {
    let derived = crate::vagrantfile::derived_from_host();
    let inputs = &env.spec().inputs;
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
        .map(|g| Ok(format!("  {} || failed=1\n", g.command.in_vm_shell()?)))
        .collect::<Result<_>>()?;
    let setup = sequence(&guest.check_setup)?;
    let checked = if setup.is_empty() {
        runners
    } else {
        format!("if (\n{setup}); then\n{runners}else\n  failed=1\nfi\n")
    };
    Ok(Some(format!(
        "{}failed=0\n{checked}{}exit \"$failed\"",
        values::linux_decode(&env.spec().inputs),
        sequence(&guest.check_teardown)?
    )))
}
