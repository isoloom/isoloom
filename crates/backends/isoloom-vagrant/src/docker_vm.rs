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
//! - The run manifest (`docker-vm/run.json`, [`manifest`]): the Isothrum runner brings the VM up
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
use isoloom_render::container as docker;
use isoloom_render::run_manifest::guest::GuestCompose;
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header, steps};

use crate::ruby::{Call, Ruby, Value};
use crate::vagrantfile::{self, shell};

const DIR: &str = "docker-vm";
/// Vagrant's name for the file's one machine.
pub(crate) const DEFINE: &str = "default";
/// The VM, as the run manifest names it.
pub(crate) const VM: &str = "docker";
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
    crate::manifest::docker_vm::manifest(env, guest)
}

/// The VM's own provisioners, in order: everything `vagrant up` runs but the lab's start (the
/// constraints, when the spec has some; Docker; SSH accepting the forwarded values, when there
/// are some; the project).
pub(crate) fn prepares(env: &Environment, guest: &GuestCompose) -> Vec<&'static str> {
    let constraints = steps::linux(env, "", &isoloom_plan::Step::Prelude).map(|_| vagrantfile::CONSTRAINTS);
    let accept = (!guest.forwarded().is_empty()).then_some(ACCEPT_ENV);
    constraints.into_iter().chain([vagrantfile::DOCKER]).chain(accept).chain([PROJECT]).collect()
}

/// The project provisioner.
const PROJECT: &str = "project";

/// `config.vm.provision "<name>", type: "<kind>"`: a provisioner named at the top level, which
/// `vagrant up --provision-with <name>` takes (it refuses a name given only as the shell
/// provisioner's own `name:` option, and the project provisioner has no such option).
fn named(name: &str, kind: &str) -> Call {
    Call::new("config.vm.provision").arg(Value::str(name)).kw("type", Value::str(kind))
}
/// The Compose file the VM runs, from the project's copy.
const COMPOSE: &str = ".isoloom/docker/compose.yml";
/// The derived-checks switch, read from the host's environment when Vagrant runs.
const DERIVED: &str = "ENV.fetch(\"ISOLOOM_DERIVED\", \"1\")";

/// The CPUs and memory (MB) of the VM: the containers' host's, at least 2 GB.
pub(crate) fn host_size(env: &Environment) -> (u32, u32) {
    let size = docker::HostSize::of(env);
    (size.cpus, size.memory_mb.max(2048))
}

/// `.isoloom/docker-vm/Vagrantfile`: the VM, Docker, then the Compose file up as `guest` starts it,
/// and its checks.
#[must_use]
pub fn vagrantfile(env: &Environment, guest: &GuestCompose) -> GeneratedFile {
    let mut r = Ruby::new();
    r.text(&header("#"));
    r.comment(&format!(
        "Start:  cd .isoloom/{DIR} && vagrant up\nChecks: cd .isoloom/{DIR} && vagrant provision --provision-with checks\nStop:   cd .isoloom/{DIR} && vagrant destroy -f"
    ));
    r.blank();
    r.assign("ROOT", Value::raw("File.expand_path(\"../..\", __dir__)"));
    r.text(&crate::vms::project_rb());
    vagrantfile::inputs(&mut r, &env.spec().inputs, false);
    r.blank();
    r.block("Vagrant.configure(\"2\")", "config", |r| configure(r, env, guest));
    GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/Vagrantfile"),
        contents: r.finish(),
    }
}

/// The VM: its box and name, the published ports, its size on every provider, then Docker, SSH
/// accepting the values given at launch, the project and the environment.
fn configure(r: &mut Ruby, env: &Environment, guest: &GuestCompose) {
    r.assign("config.vm.box", Value::str("bento/debian-12"));
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
    let containers = env.machines.values().filter(|m| m.shape == MachineShape::Container);
    for p in containers.flat_map(|m| &m.ports) {
        r.call(vagrantfile::forwarded_port("config", p.publish, p.publish, p.protocol));
    }
    let size = host_size(env);
    let label = format!("{} · docker", env.name);
    desktop_providers(r, &label, size);
    esxi(r, &format!("{}-docker", env.name), size);
    if let Some(sh) = steps::linux(env, "", &isoloom_plan::Step::Prelude) {
        r.call(named(vagrantfile::CONSTRAINTS, "shell").kw("inline", Value::str(&sh)));
    }
    let docker = format!("{}\n{}", steps::INSTALL_DOCKER, steps::PUBLISHED_FROM_OUTSIDE);
    r.call(named(vagrantfile::DOCKER, "shell").kw("inline", Value::str(&docker)));
    if !forwarded.is_empty() {
        let accept = format!(
            "mkdir -p /etc/ssh/sshd_config.d && echo {} > {ACCEPT_ENV_FILE} && (systemctl reload ssh || systemctl reload sshd)",
            isoloom_spec::shell::quote(&format!("AcceptEnv {}", forwarded.join(" ")))
        );
        r.call(named(ACCEPT_ENV, "shell").kw("inline", Value::str(&accept)));
    }
    // The project, its generated Compose file included.
    r.call(named(PROJECT, "isoloom_project").kw("generated", true));
    environment(r, env, guest);
    checks(r, env);
}

/// The size on the providers of this machine (VirtualBox, VMware, Parallels, UTM, QEMU,
/// libvirt), named `label` where the provider shows a name.
fn desktop_providers(r: &mut Ruby, label: &str, (cpus, mem): (u32, u32)) {
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
    // Apple Silicon Macs: UTM and QEMU, as for the VMs of the Vagrant target.
    r.block(provider("utm"), "v", named);
    r.block(provider("qemu"), "v", |r| {
        r.assign("v.smp", Value::str(&format!("cpus={cpus}")));
        r.assign("v.memory", Value::str(&format!("{mem}M")));
    });
    r.block(provider("libvirt"), "v, o", |r| {
        r.assign("o.vm.box", Value::str("generic/debian12"));
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
        "{secrets}{} && mkdir -p /var/lib/isoloom && echo ready > /var/lib/isoloom/ready",
        start.join(" && ")
    );
    let inputs = (!env.spec().inputs.is_empty()).then(|| Value::raw("INPUTS"));
    r.call(shell("config", "environment").kw("inline", Value::str(&script)).kw_opt("env", inputs));
}

/// Checks on demand: every runner of the Compose `check` profile (one per position), then the
/// `exec` checks piped into their machines; the derived checks switched off with
/// `ISOLOOM_DERIVED=0` on the host.
fn checks(r: &mut Ruby, env: &Environment) {
    if env.checks.is_empty() {
        return;
    }
    let profile = format!("docker compose -f {COMPOSE} --profile check");
    let access = docker::as_arguments(&format!("{profile} config --services | grep -x -e isoloom-access -e isoloom-access-routes"));
    let script = format!(
        "cd /opt/isoloom && failed=0; {access}; [ $# -eq 0 ] || {profile} up -d --wait --no-deps \"$@\"; for s in $({profile} config --services | grep '^isoloom-check'); do {profile} run --rm --no-deps -e ISOLOOM_DERIVED \"$s\" || failed=1; done; {}exit \"$failed\"",
        exec_runs(env)
    );
    let derived = Value::hash([(Value::str("ISOLOOM_DERIVED"), Value::raw(DERIVED))]);
    let vars = if env.spec().inputs.is_empty() {
        derived
    } else {
        Value::raw(format!("INPUTS.merge({derived})"))
    };
    let call = shell("config", "checks").kw("run", Value::str("never"));
    r.call(call.kw("inline", Value::str(&script)).kw("env", vars));
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
