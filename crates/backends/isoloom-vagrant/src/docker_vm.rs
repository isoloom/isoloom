//! The `docker-vm` target: the environment's Compose file, run by Docker on one VM, in
//! `.isoloom/docker-vm/Vagrantfile` (a VM on this machine, or on an ESXi host).
//!
//! - One Debian VM, sized for every container together (and Docker itself).
//! - Docker from Docker's own repository; the project copied to /opt/isoloom; then
//!   `docker compose up --wait` with `.isoloom/docker/compose.yml`, the `docker` target's Compose
//!   file, rendered with it by the Compose backend.
//! - When everything answers: /var/lib/isoloom/ready, for runners to poll.
//! - Checks on demand: `vagrant provision --provision-with checks` (every runner of the Compose
//!   `check` profile), or `isoloom test docker-vm`.
//! - Published ports: the VM forwards them from the host's loopback.
//! - On a Proxmox server: `.isoloom/docker-vm/proxmox/main.tf` (Terraform, bpg/proxmox), one
//!   VM on the uplink bridge, the same set-up over SSH as `cloud-docker`'s Docker host.
//! - ESXi: the vagrant-vmware-esxi provider reads the host from `ESXI_HOSTNAME`,
//!   `ESXI_USERNAME` and `ESXI_PASSWORD` (and the datastore from `ESXI_DATASTORE`).
//!
//! This is the Vagrant side of the target; `isoloom-targets` puts it together with the
//! `docker` target's Compose file (first) and the Proxmox module, and refuses what the Compose
//! file can't run: its capabilities are Compose's.

use isoloom_plan::checks::{Position, Probe};
use isoloom_plan::{Environment, MachineShape};
use isoloom_render::container as docker;
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header};

use crate::ruby::{Call, Ruby, Value};
use crate::vagrantfile::{self, shell};

const DIR: &str = "docker-vm";
/// The Compose file the VM runs, from the project's copy.
const COMPOSE: &str = ".isoloom/docker/compose.yml";
/// The derived-checks switch, read from the host's environment when Vagrant runs.
const DERIVED: &str = "ENV.fetch(\"ISOLOOM_DERIVED\", \"1\")";

/// The CPUs and memory (MB) of the VM: the containers' host's, at least 2 GB.
fn host_size(env: &Environment) -> (u32, u32) {
    let size = docker::HostSize::of(env);
    (size.cpus, size.memory_mb.max(2048))
}

/// `.isoloom/docker-vm/Vagrantfile`: the VM, Docker, then the Compose file up and its checks.
#[must_use]
pub fn vagrantfile(env: &Environment) -> GeneratedFile {
    let mut r = Ruby::new();
    r.text(&header("#"));
    r.comment(&format!(
        "Start:  cd .isoloom/{DIR} && vagrant up\nChecks: cd .isoloom/{DIR} && vagrant provision --provision-with checks\nStop:   cd .isoloom/{DIR} && vagrant destroy -f"
    ));
    r.blank();
    r.assign("ROOT", Value::raw("File.expand_path(\"../..\", __dir__)"));
    r.text(crate::vms::PROJECT_RB);
    vagrantfile::inputs(&mut r, &env.spec().inputs);
    r.blank();
    r.block("Vagrant.configure(\"2\")", "config", |r| configure(r, env));
    GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/Vagrantfile"),
        contents: r.finish(),
    }
}

/// The VM: its box and name, the published ports, its size on every provider, then Docker and
/// the environment.
fn configure(r: &mut Ruby, env: &Environment) {
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
    vagrantfile::libvirt_prefix(r, "config", &env.name);
    let containers = env.machines.values().filter(|m| m.shape == MachineShape::Container);
    for p in containers.flat_map(|m| &m.ports) {
        r.call(vagrantfile::forwarded_port("config", p.publish, p.publish, p.protocol));
    }
    let size = host_size(env);
    let label = format!("{} · docker", env.name);
    desktop_providers(r, &label, size);
    esxi(r, &format!("{}-docker", env.name), size);
    vagrantfile::prelude(r, env, "config");
    vagrantfile::install_docker(r, "config");
    // The project, its generated Compose file included.
    r.call(
        Call::new("config.vm.provision")
            .arg(Value::str("isoloom_project"))
            .kw("name", Value::str("project"))
            .kw("generated", true),
    );
    environment(r, env);
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
    r.block(provider("virtualbox"), "v", named);
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

/// The Compose file up (published on every address of the VM, at the fixed host ports Vagrant
/// forwards), with the containers' secrets once `isoloom run` delivered them and the project
/// files Compose reads from its environment, then the ready mark.
fn environment(r: &mut Ruby, env: &Environment) {
    let compose = format!("ISOLOOM_PUBLISH_ADDRESS=0.0.0.0 ISOLOOM_PUBLISH_FIXED=1 docker compose -f {COMPOSE}");
    let start = docker::start_commands(&compose, &docker::leaf_jobs(env.spec()), Some(900));
    let secrets = isoloom_render::secrets::host_environment(env.spec(), &isoloom_render::secrets::of_containers(env));
    let files = docker::host_environment(env);
    let script = format!("{secrets}cd /opt/isoloom && {files}{start} && mkdir -p /var/lib/isoloom && echo ready > /var/lib/isoloom/ready");
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
    let script = format!(
        "cd /opt/isoloom && failed=0; sa=$({profile} config --services | grep -x -e isoloom-access -e isoloom-access-routes); [ -z \"$sa\" ] || {profile} up -d --wait --no-deps $sa; for s in $({profile} config --services | grep '^isoloom-check'); do {profile} run --rm --no-deps -e ISOLOOM_DERIVED \"$s\" || failed=1; done; {}exit $failed",
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
