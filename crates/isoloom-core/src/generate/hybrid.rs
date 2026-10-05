//! The `hybrid` target: containers and VMs together on the same networks, on this machine, in
//! `.isoloom/hybrid/` (a Vagrantfile and a Compose file).
//!
//! - A machine with `docker:` is a container (the light option); a machine with only `vm:` is a
//!   VM. The VMs, the router and the controller are what the `vagrant` output makes of them.
//! - The containers run on one more VM, `isoloom-docker`, plugged into the networks they're on
//!   with promiscuous network interfaces: Docker attaches each container to its network directly
//!   (`macvlan`), at its own address, so VMs and containers reach each other as on one network.
//! - Published ports: macvlan can't publish, so a container with published ports also joins
//!   Docker's bridge network (its default route stays on its lab network), and the Docker host
//!   VM forwards each port to this machine's loopback.
//! - VirtualBox only for now (promiscuous network interfaces are set per provider).
//! - Not yet: gateways; a VM that depends on a container (VMs start first); containers reaching
//!   the internet while running (the Docker host pulls their images).

use std::fmt::Write;

use serde_yaml_ng::{Mapping, Value};

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, address, header, router};
use crate::model::{Spec, Target};
use crate::validate::Cidr;

const DIR: &str = "hybrid";
const HOST: &str = "isoloom-docker";
/// The bridge network of the containers with published ports.
const PUBLISH_NET: &str = "isoloom-publish";

fn rb(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace("#{", "\\#{"))
}

/// The Docker network a container network is on the host: named after the environment.
fn docker_net(spec: &Spec, net: &str) -> String {
    format!("isoloom-{}-{net}", spec.name)
}

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    let unsupported = |what: String| GenerateError::Unsupported { target: Target::Hybrid, what };
    if spec.networks.values().any(|n| n.gateway.is_some()) {
        return Err(unsupported("networks with a `gateway` machine in hybrid environments come later".into()));
    }
    let container = |name: &str| spec.machines[name].docker.is_some();
    for (name, m) in &spec.machines {
        if !container(name)
            && let Some(dep) = m.depends_on.iter().find(|d| container(d))
        {
            return Err(unsupported(format!(
                "VM `{name}` depends on container `{dep}`: VMs start first in a hybrid environment, for now"
            )));
        }
    }

    // The VMs: the machines that can't be containers, as the vagrant output makes them.
    let mut vms = spec.clone();
    for m in vms.machines.values_mut() {
        if m.docker.is_some() {
            m.vm = None;
        }
    }
    let vfiles = super::vagrant::generate(&vms).map_err(|e| match e {
        GenerateError::Unsupported { what, .. } => unsupported(what),
        other => other,
    })?;
    let mut vagrantfile = vfiles.into_iter().find(|f| f.path.ends_with("Vagrantfile")).expect("a Vagrantfile").contents;
    vagrantfile = vagrantfile.replace(".isoloom/vagrant", ".isoloom/hybrid");

    // The containers: the Compose file of the container machines alone, plugged into the host's
    // networks; the router is the router VM; the checks run from the VM side.
    let mut ctr = spec.clone();
    ctr.machines.retain(|_, m| m.docker.is_some());
    let kept: Vec<String> = ctr.machines.keys().cloned().collect();
    for m in ctr.machines.values_mut() {
        m.depends_on.retain(|d| kept.contains(d));
    }
    ctr.checks.clear();
    ctr.provision.clear();
    let cfiles = super::docker::generate(&ctr, &ctr).map_err(|e| match e {
        GenerateError::Unsupported { what, .. } => unsupported(what),
        other => other,
    })?;
    let compose = cfiles.into_iter().find(|f| f.path.ends_with("compose.yml")).expect("a Compose file").contents;
    let mut doc: Value = serde_yaml_ng::from_str(&compose).expect("the Compose file Isoloom wrote");
    let nets: Vec<String> = {
        let mut n: Vec<String> = ctr.machines.values().flat_map(|m| m.networks.keys().cloned()).collect();
        n.sort();
        n.dedup();
        // In the spec's order.
        spec.networks.keys().filter(|k| n.contains(k)).cloned().collect()
    };
    let mut published: Vec<String> = Vec::new();
    if let Some(Value::Mapping(services)) = doc.get_mut("services") {
        services.remove(Value::String(router::NAME.into()));
        let names: Vec<String> = services.keys().filter_map(|k| k.as_str().map(str::to_string)).collect();
        // The VM machines, by name, for the containers (Docker's DNS only knows containers).
        let vm_hosts: Vec<Value> = spec
            .machines
            .iter()
            .filter(|(n, _)| !container(n))
            .filter_map(|(n, m)| m.networks.first().map(|(net, o)| Value::String(format!("{n}:{}", address(spec, net, *o)))))
            .collect();
        for (_, svc) in services.iter_mut() {
            let Value::Mapping(svc) = svc else { continue };
            if let Some(Value::Sequence(ports)) = svc.get_mut("ports") {
                for p in ports.iter_mut() {
                    let Some(text) = p.as_str() else { continue };
                    // `${ISOLOOM_PUBLISH_ADDRESS:-127.0.0.1}:<host>:<port>`: on every address
                    // of the Docker host, which Vagrant forwards to this machine's loopback.
                    let mut parts = text.rsplitn(3, ':');
                    let (Some(port), Some(host)) = (parts.next(), parts.next()) else { continue };
                    published.push(host.to_string());
                    *p = Value::String(format!("{host}:{port}"));
                }
                if let Some(Value::Mapping(nets)) = svc.get_mut("networks") {
                    for (_, n) in nets.iter_mut() {
                        if let Value::Mapping(n) = n {
                            n.insert(Value::String("gw_priority".into()), Value::Number(1.into()));
                        }
                    }
                    nets.insert(Value::String(PUBLISH_NET.into()), Value::Null);
                }
            }
            if let Some(Value::Mapping(deps)) = svc.get_mut("depends_on") {
                deps.retain(|k, _| k.as_str().is_some_and(|k| names.iter().any(|n| n == k)));
            }
            if svc.contains_key("networks") && !vm_hosts.is_empty() {
                let entry = svc.entry(Value::String("extra_hosts".into())).or_insert(Value::Sequence(Vec::new()));
                if let Value::Sequence(list) = entry {
                    list.extend(vm_hosts.iter().cloned());
                }
            }
        }
    }
    let mut networks = Mapping::new();
    if !published.is_empty() {
        networks.insert(Value::String(PUBLISH_NET.into()), Value::Null);
    }
    for net in &nets {
        let mut n = Mapping::new();
        n.insert(Value::String("external".into()), Value::Bool(true));
        n.insert(Value::String("name".into()), Value::String(docker_net(spec, net)));
        networks.insert(Value::String(net.clone()), Value::Mapping(n));
    }
    if let Value::Mapping(top) = &mut doc {
        top.insert(Value::String("networks".into()), Value::Mapping(networks));
    }
    let mut compose_out = header("#");
    compose_out.push_str("# The containers of a hybrid environment: run on the isoloom-docker VM, on the networks it\n# creates there (macvlan, shared with the VMs). Started by the Vagrantfile next to it.\n\n");
    compose_out.push_str(&serde_yaml_ng::to_string(&doc).expect("YAML"));

    // The Docker host: a VM on the containers' networks, interfaces in promiscuous mode.
    let mem: u32 = 1024 + ctr.machines.values().map(|m| m.resources.and_then(|r| r.memory_mb).unwrap_or(512)).sum::<u32>();
    let cpus: u32 = ctr
        .machines
        .values()
        .map(|m| m.resources.and_then(|r| r.cpus).unwrap_or(1))
        .sum::<u32>()
        .clamp(2, 16);
    let mut host = format!(
        "\n  # The containers' host: on their networks, letting Docker give each its own address there.\n  config.vm.define {} do |m|\n    m.vm.box = \"bento/debian-12\"\n    m.vm.hostname = {}\n",
        rb(HOST),
        rb(HOST)
    );
    for host_port in &published {
        let _ = writeln!(
            host,
            "    m.vm.network \"forwarded_port\", guest: {host_port}, host: {host_port}, host_ip: \"127.0.0.1\""
        );
    }
    let mut nics = String::new();
    let mut create = String::from("set -e\n");
    for (i, net) in nets.iter().enumerate() {
        let c = Cidr::parse(&spec.networks[net].cidr).expect("validated cidr");
        let mac = format!("0A15{:06X}{:02X}", name_hash(&spec.name) & 0xFF_FFFF, i as u8);
        let netname = format!("isoloom-{}-{net}", spec.name);
        // auto_config off: the host itself takes no address there (`ip` is only what Vagrant
        // requires); its containers do.
        let _ = writeln!(
            host,
            "    m.vm.network \"private_network\", ip: {}, auto_config: false, mac: {}, virtualbox__intnet: {}",
            rb(&c.controller().to_string()),
            rb(&mac),
            rb(&netname),
        );
        let _ = writeln!(nics, "      v.customize [\"modifyvm\", :id, \"--nicpromisc{}\", \"allow-all\"]", i + 2);
        let gw = if router::needed(spec) { router::address(spec, net) } else { c.gateway() };
        let _ = writeln!(
            create,
            "IF=$(ip -o link | grep -i '{mac_l}' | awk -F': ' '{{print $2}}')\nip link set \"$IF\" up promisc on\ndocker network inspect {dn} >/dev/null 2>&1 || docker network create -d macvlan --subnet {cidr} --gateway {gw} -o parent=\"$IF\" {dn}",
            mac_l = mac
                .to_lowercase()
                .as_bytes()
                .chunks(2)
                .map(|p| std::str::from_utf8(p).unwrap_or_default())
                .collect::<Vec<_>>()
                .join(":"),
            dn = docker_net(spec, net),
            cidr = spec.networks[net].cidr,
        );
    }
    host.push_str(&format!(
        "    m.vm.provider \"virtualbox\" do |v|\n      v.name = {}\n      v.cpus = {cpus}\n      v.memory = {mem}\n{nics}    end\n",
        rb(&format!("{} · containers", spec.name))
    ));
    host.push_str("    m.vm.provision \"shell\", name: \"docker\", inline: \"command -v docker >/dev/null || curl -fsSL https://get.docker.com | sh\"\n");
    host.push_str(
        "    PROJECT.each do |entry|\n      m.vm.provision \"file\", source: File.join(ROOT, entry), destination: \"/tmp/isoloom-project/#{entry}\"\n    end\n",
    );
    host.push_str(
        "    m.vm.provision \"file\", source: File.join(__dir__, \"compose.yml\"), destination: \"/tmp/isoloom-project/.isoloom/hybrid/compose.yml\"\n",
    );
    host.push_str("    m.vm.provision \"shell\", name: \"project\", inline: \"rm -rf /opt/isoloom && mv /tmp/isoloom-project /opt/isoloom\"\n");
    let _ = writeln!(
        host,
        "    m.vm.provision \"shell\", name: \"networks\", inline: <<~'SH'\n{}    SH",
        indent(&create, 6)
    );
    let env = if spec.inputs.is_empty() { "" } else { ", env: INPUTS" };
    let _ = writeln!(
        host,
        "    m.vm.provision \"shell\", name: \"containers\", inline: \"cd /opt/isoloom && docker compose -f .isoloom/hybrid/compose.yml up -d --build --wait --wait-timeout 900\"{env}\n  end"
    );
    let at = vagrantfile.rfind("end\n").expect("the Vagrantfile ends its configure block");
    vagrantfile.insert_str(at, &host);

    Ok(vec![
        GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/Vagrantfile"),
            contents: vagrantfile,
        },
        GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/compose.yml"),
            contents: compose_out,
        },
    ])
}

fn indent(s: &str, n: usize) -> String {
    let pad = " ".repeat(n);
    s.lines().map(|l| if l.is_empty() { "\n".to_string() } else { format!("{pad}{l}\n") }).collect()
}

/// A stable hash of the environment's name, for the Docker host's MAC addresses (FNV-1a).
fn name_hash(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193))
}
