//! The `docker-vm` target: the environment's Compose file, run by Docker on one VM, in
//! `.isoloom/docker-vm/Vagrantfile` (a VM on this machine, or on an ESXi host).
//!
//! - One Debian VM, sized for every machine together (and Docker itself).
//! - Docker from Docker's own repository; the project copied to /opt/isoloom; then
//!   `docker compose up --wait` with `.isoloom/docker/compose.yml` (generated with it).
//! - When everything answers: /var/lib/isoloom/ready, for runners to poll.
//! - Checks on demand: `vagrant provision --provision-with checks` (every runner of the Compose
//!   `check` profile), or `isoloom test docker-vm`.
//! - Published ports: the VM forwards them from the host's loopback.
//! - On a Proxmox server: `.isoloom/docker-vm/proxmox/main.tf` (Terraform, bpg/proxmox), one
//!   VM on the uplink bridge, the same steps over SSH.
//! - ESXi: the vagrant-vmware-esxi provider reads the host from ESXI_HOSTNAME, ESXI_USERNAME
//!   and ESXI_PASSWORD (and the datastore from ESXI_DATASTORE).

use std::fmt::Write;

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, header};
use crate::model::Spec;

const DIR: &str = "docker-vm";

/// Ruby double-quoted string.
fn rb(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace("#{", "\\#{"))
}

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    // Sized for every machine at once, plus Docker: at least 2 CPUs and 2 GB.
    let machines = spec.machines.values().filter(|m| m.docker.is_some());
    let (cpus, mem) = machines.fold((0u32, 1024u32), |(c, m), x| {
        let r = x.resources;
        (c + r.and_then(|r| r.cpus).unwrap_or(1), m + r.and_then(|r| r.memory_mb).unwrap_or(512))
    });
    let (cpus, mem) = (cpus.clamp(2, 16), mem.max(2048));
    let label = format!("{} · docker", spec.name);

    let mut out = header("#");
    out.push_str("# Start:  cd .isoloom/docker-vm && vagrant up\n# Checks: cd .isoloom/docker-vm && vagrant provision --provision-with checks\n# Stop:   cd .isoloom/docker-vm && vagrant destroy -f\n\n");
    out.push_str("ROOT = File.expand_path(\"../..\", __dir__)\n");
    out.push_str("# Copied into the VM: the project, its generated Compose file included.\n");
    out.push_str("PROJECT = Dir.children(ROOT).reject { |e| [\".git\", \".vagrant\"].include?(e) }.sort\n");
    if !spec.inputs.is_empty() {
        out.push_str("# Values provided at launch (empty when unset).\nINPUTS = {\n");
        for i in &spec.inputs {
            let _ = writeln!(out, "  {} => ENV.fetch({}, \"\"),", rb(i), rb(i));
        }
        out.push_str("}\n");
    }
    out.push_str("\nVagrant.configure(\"2\") do |config|\n");
    out.push_str("  config.vm.box = \"bento/debian-12\"\n");
    let _ = writeln!(out, "  config.vm.hostname = {}", rb(&spec.name));
    out.push_str("  config.vm.synced_folder \".\", \"/vagrant\", disabled: true\n  config.vm.boot_timeout = 600\n");
    // libvirt's domain name: the environment's, not this folder's ("docker-vm_default").
    let _ = writeln!(
        out,
        "  config.vm.provider \"libvirt\" do |v|\n    v.default_prefix = {}\n  end",
        rb(&format!("{}_", spec.name))
    );
    for m in spec.machines.values().filter(|m| m.docker.is_some()) {
        for svc in &m.services {
            if let Some(host) = svc.publish {
                let _ = writeln!(
                    out,
                    "  config.vm.network \"forwarded_port\", guest: {host}, host: {host}, host_ip: \"127.0.0.1\""
                );
            }
        }
    }
    let _ = writeln!(
        out,
        "  config.vm.provider \"virtualbox\" do |v|\n    v.name = {}\n    v.cpus = {cpus}\n    v.memory = {mem}\n  end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "  config.vm.provider \"vmware_desktop\" do |v|\n    v.vmx[\"displayName\"] = {}\n    v.vmx[\"numvcpus\"] = \"{cpus}\"\n    v.vmx[\"memsize\"] = \"{mem}\"\n  end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "  config.vm.provider \"parallels\" do |v|\n    v.name = {}\n    v.cpus = {cpus}\n    v.memory = {mem}\n  end",
        rb(&label)
    );
    // Apple Silicon Macs: UTM and QEMU, as for the VMs of the Vagrant target.
    let _ = writeln!(
        out,
        "  config.vm.provider \"utm\" do |v|\n    v.name = {}\n    v.cpus = {cpus}\n    v.memory = {mem}\n  end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "  config.vm.provider \"qemu\" do |v|\n    v.smp = \"cpus={cpus}\"\n    v.memory = \"{mem}M\"\n  end"
    );
    let _ = writeln!(
        out,
        "  config.vm.provider \"libvirt\" do |v, o|\n    o.vm.box = \"generic/debian12\"\n    v.cpus = {cpus}\n    v.memory = {mem}\n  end"
    );
    let _ = writeln!(
        out,
        "  config.vm.provider \"vmware_esxi\" do |v|\n    v.esxi_hostname = ENV.fetch(\"ESXI_HOSTNAME\", \"\")\n    v.esxi_hostport = ENV.fetch(\"ESXI_HOSTPORT\", \"22\").to_i\n    v.esxi_username = ENV.fetch(\"ESXI_USERNAME\", \"root\")\n    v.esxi_password = \"env:ESXI_PASSWORD\"\n    v.esxi_disk_store = ENV[\"ESXI_DATASTORE\"] if ENV[\"ESXI_DATASTORE\"]\n    v.esxi_virtual_network = [ENV.fetch(\"ESXI_VIRTUAL_NETWORK\", \"VM Network\").split(\",\").first.strip]\n    v.guest_name = {}\n    v.guest_numvcpus = {cpus}\n    v.guest_memsize = {mem}\n  end",
        rb(&format!("{}-docker", spec.name))
    );
    out.push_str("  config.vm.provision \"shell\", name: \"docker\", inline: \"command -v docker >/dev/null || curl -fsSL https://get.docker.com | sh\"\n");
    out.push_str(
        "  PROJECT.each do |entry|\n    config.vm.provision \"file\", source: File.join(ROOT, entry), destination: \"/tmp/isoloom-project/#{entry}\"\n  end\n",
    );
    out.push_str("  config.vm.provision \"shell\", name: \"project\", inline: \"rm -rf /opt/isoloom && mv /tmp/isoloom-project /opt/isoloom\"\n");
    let env = if spec.inputs.is_empty() { String::new() } else { ", env: INPUTS".to_string() };
    let _ = writeln!(
        out,
        "  config.vm.provision \"shell\", name: \"environment\", inline: {}{env}",
        rb(&format!(
            "cd /opt/isoloom && {} && mkdir -p /var/lib/isoloom && echo ready > /var/lib/isoloom/ready",
            super::docker::start_commands(
                "ISOLOOM_PUBLISH_ADDRESS=0.0.0.0 ISOLOOM_PUBLISH_FIXED=1 docker compose -f .isoloom/docker/compose.yml",
                &super::docker::leaf_jobs(spec),
                Some(900)
            )
        ))
    );
    // Checks on demand: every runner of the Compose `check` profile (one per position), the
    // derived checks switched off with ISOLOOM_DERIVED=0 on the host.
    if !crate::checks::plan(spec).is_empty() {
        let env = if spec.inputs.is_empty() {
            ", env: { \"ISOLOOM_DERIVED\" => ENV.fetch(\"ISOLOOM_DERIVED\", \"1\") }".to_string()
        } else {
            ", env: INPUTS.merge({ \"ISOLOOM_DERIVED\" => ENV.fetch(\"ISOLOOM_DERIVED\", \"1\") })".to_string()
        };
        let _ = writeln!(
            out,
            "  config.vm.provision \"shell\", name: \"checks\", run: \"never\", inline: {}{env}",
            rb(
                "cd /opt/isoloom && failed=0; sa=$(docker compose -f .isoloom/docker/compose.yml --profile check config --services | grep -x -e isoloom-access -e isoloom-access-routes); [ -z \"$sa\" ] || docker compose -f .isoloom/docker/compose.yml --profile check up -d --wait --no-deps $sa; for s in $(docker compose -f .isoloom/docker/compose.yml --profile check config --services | grep '^isoloom-check'); do docker compose -f .isoloom/docker/compose.yml --profile check run --rm --no-deps -e ISOLOOM_DERIVED \"$s\" || failed=1; done; exit $failed"
            )
        );
    }
    out.push_str("end\n");
    Ok(vec![
        GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/Vagrantfile"),
            contents: out,
        },
        super::cloud_docker::other_in(spec, DIR, "proxmox", super::cloud_docker::PROXMOX),
    ])
}
