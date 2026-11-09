//! The `docker-vm` target: the environment's Compose file, run by Docker on one VM, in
//! `.isoloom/docker-vm/Vagrantfile` (a VM on this machine, or on an ESXi host).
//!
//! - One Debian VM, sized for every container together (and Docker itself).
//! - Docker from Docker's own repository; the project copied to /opt/isoloom; then the lab started
//!   with `.isoloom/docker/compose.yml`, the `docker` target's Compose file, rendered with it by
//!   the Compose backend: the commands Compose's backend gives ([`GuestCompose`]).
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

use isoloom_manifest::Manifest;
use isoloom_plan::checks::{Position, Probe};
use isoloom_plan::{Environment, MachineShape};
use isoloom_render::container::{self as docker, DOCKER_HOST as VM};
use isoloom_render::run_manifest::DERIVED;
use isoloom_render::run_manifest::guest::GuestCompose;
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header, steps};
use isoloom_spec::images;
use isoloom_spec::model::Target;

use crate::host_folders::Share;
use crate::ruby::{BootStatements, Call, Provision, Ruby, Value};
use crate::vagrantfile::{self, shell};

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
/// The Compose file the VM runs, from the project's copy.
const COMPOSE: &str = ".isoloom/docker/compose.yml";

/// The CPUs and memory (MB) of the VM: the containers' host's.
fn host_size(env: &Environment) -> (u32, u32) {
    let size = docker::HostSize::of(env);
    (size.cpus, size.memory_mb)
}

/// `.isoloom/docker-vm/Vagrantfile`: the VM, Docker, then the Compose file up as `guest` starts it,
/// and its checks.
#[must_use]
pub fn vagrantfile(env: &Environment, guest: &GuestCompose) -> GeneratedFile {
    GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/Vagrantfile"),
        contents: written(env, guest).finish(),
    }
}

/// What the Vagrantfile of `env` applies to its VM only when it boots ([`BootStatements`]), as
/// its builder records it while writing it.
pub(crate) fn boot(env: &Environment, guest: &GuestCompose) -> BootStatements {
    written(env, guest).boot().clone()
}

/// The Vagrantfile of `env`, written.
fn written(env: &Environment, guest: &GuestCompose) -> Ruby {
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
    r.configure(|r| configure(r, env, guest));
    r
}

/// The VM: its box and name, the published ports, its size on every provider, then Docker, SSH
/// accepting the values given at launch, the project and the environment.
fn configure(r: &mut Ruby, env: &Environment, guest: &GuestCompose) {
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
    let label = format!("{} · docker", env.name);
    desktop_providers(r, &label, size, docker::HostSize::of(env).disk_gb);
    esxi(r, &format!("{}-docker", env.name), size);
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
    // The project, its generated Compose file included: first, so the certificate authorities
    // it holds are trusted before Docker downloads.
    r.provision(named(PROJECT, "isoloom_project").kw("generated", true));
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
    environment(r, env, guest);
    checks(r, env);
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
        r.assign("o.vm.box", Value::str(images::QEMU_DEBIAN_12));
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
            Value::raw(format!("IsoloomQemu.disk({}, {disk_gb})", Value::str(images::QEMU_DEBIAN_12))),
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
/// values given at launch, then the ready mark.
fn environment(r: &mut Ruby, env: &Environment, guest: &GuestCompose) {
    let start: Vec<String> = guest.start.iter().map(|c| c.command.to_string()).collect();
    let secrets = isoloom_render::secrets::host_environment(env.spec(), &isoloom_render::secrets::of_containers(env));
    let script = format!(
        "{secrets}{} && {} && echo ready > {}",
        start.join(" && "),
        steps::MAKE_STATE_DIR,
        steps::READY_FILE
    );
    let inputs = (!env.spec().inputs.is_empty()).then(|| Value::raw("INPUTS"));
    r.provision(shell("config", "environment").kw("inline", Value::str(&script)).kw_opt("env", inputs));
}

/// Checks on demand: every runner of the Compose `check` profile (one per position), then the
/// `exec` checks piped into their machines; the derived checks switched off with
/// `ISOLOOM_DERIVED=0` on the host.
fn checks(r: &mut Ruby, env: &Environment) {
    if env.checks.is_empty() {
        return;
    }
    let profile = format!("docker compose -f {COMPOSE} --profile check");
    let (stand_in, routes) = (docker::STAND_IN, docker::routes_service(docker::STAND_IN));
    let access = docker::as_arguments(&format!("{profile} config --services | grep -x -e {stand_in} -e {routes}"));
    // The stand-ins go once the checks ran, whatever they found, so the access machine they
    // stand in for can take its addresses; failing to take them down fails no check.
    let script = format!(
        "cd /opt/isoloom && failed=0; {access}; [ $# -eq 0 ] || {profile} {up} \"$@\"; for s in $({profile} config --services | grep '^{runners}'); do {profile} run --rm --no-deps -e {DERIVED} \"$s\" || failed=1; done; {}[ $# -eq 0 ] || {profile} {down} \"$@\" || true; exit \"$failed\"",
        exec_runs(env),
        up = docker::STAND_INS_UP.verb.join(" "),
        down = docker::STAND_INS_DOWN.verb.join(" "),
        runners = docker::CHECK_RUNNER_PREFIX,
    );
    let derived = crate::vagrantfile::derived_from_host();
    let vars = if env.spec().inputs.is_empty() {
        derived
    } else {
        Value::raw(format!("INPUTS.merge({derived})"))
    };
    let call = shell("config", "checks").kw("run", Value::str("never"));
    r.provision(call.kw("inline", Value::str(&script)).kw("env", vars));
}

/// The `exec` checks' runners, piped into their machines (see `docker::exec_runner`), one per
/// machine in the order of its first check.
fn exec_runs(env: &Environment) -> String {
    let mut machines: Vec<&str> = Vec::new();
    for c in &env.checks {
        if let (Probe::Exec { .. }, Position::Machine(m)) = (&c.probe, &c.position)
            && !machines.contains(&m.as_str())
        {
            machines.push(m);
        }
    }
    let runs: Vec<String> = machines
        .iter()
        .map(|m| {
            format!(
                "docker compose -f {COMPOSE} exec -T {m} sh -s < .isoloom/docker/checks/{} || failed=1; ",
                docker::exec_runner(m)
            )
        })
        .collect();
    runs.concat()
}
