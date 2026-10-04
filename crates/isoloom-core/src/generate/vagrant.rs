//! The `vagrant` target: one VM per machine, in `.isoloom/vagrant/Vagrantfile`.
//!
//! - Each network is a private network: a VirtualBox internal network (any address, isolated
//!   from the host's LAN) or a libvirt network, with the machine's fixed address on it.
//! - Every VM gets the other machines' names in `/etc/hosts`, so provisioning uses names.
//! - The project is copied into the VM at `/opt/isoloom` (no shared folders, works on every
//!   host OS), then the `vm.provision` steps run there in order: `.sh` with sh, `.yml` /
//!   `.yaml` with Ansible inside the VM (never on the user's machine).
//! - Machines start in dependency order; Vagrant boots and provisions them one after the
//!   other, so a machine's services are installed before the machines depending on it start.
//! - Inputs are read from the environment (empty when unset), passed only to the machines
//!   that list them.

use std::fmt::Write;

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, address, address_for, common_unsupported, header, netmask, start_order};
use crate::model::{Spec, Target};

const DIR: &str = "vagrant";

/// Ruby double-quoted string.
fn rb(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace("#{", "\\#{"))
}

/// The Vagrant box for an OS name (and the libvirt one when it differs).
fn boxes(os: &str) -> Option<(&'static str, Option<&'static str>)> {
    match os {
        "debian-12" => Some(("bento/debian-12", Some("generic/debian12"))),
        "ubuntu-24.04" => Some(("bento/ubuntu-24.04", None)),
        "kali" => Some(("kalilinux/rolling", None)),
        _ => None,
    }
}

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    common_unsupported(spec, Target::Vagrant)?;
    let unsupported = |what: String| GenerateError::Unsupported { target: Target::Vagrant, what };
    if let Some((net, _)) = spec.networks.iter().find(|(_, n)| !n.internet) {
        return Err(unsupported(format!(
            "network `{net}` has `internet: false`, which needs a router to enforce on VMs; not built yet"
        )));
    }
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        if boxes(&vm.os).is_none() {
            return Err(unsupported(format!(
                "machine `{name}`: no image for `{}` on local VMs yet (Windows images come later)",
                vm.os
            )));
        }
        for step in &vm.provision {
            if !(step.ends_with(".sh") || step.ends_with(".yml") || step.ends_with(".yaml")) {
                return Err(unsupported(format!(
                    "machine `{name}`: provisioning step `{step}` isn't a .sh, .yml or .yaml file"
                )));
            }
        }
    }

    let mut out = header("#");
    out.push_str("# Start:  cd .isoloom/vagrant && vagrant up\n# Stop:   cd .isoloom/vagrant && vagrant destroy -f\n\n");
    out.push_str("ROOT = File.expand_path(\"../..\", __dir__)\n");
    out.push_str("# Copied into each VM: the project, without version control or generated files.\n");
    out.push_str("PROJECT = Dir.children(ROOT).reject { |e| [\".git\", \".isoloom\", \".vagrant\"].include?(e) }.sort\n");
    if !spec.inputs.is_empty() {
        out.push_str("# Values provided at launch (empty when unset).\nINPUTS = {\n");
        for i in &spec.inputs {
            let _ = writeln!(out, "  {} => ENV.fetch({}, \"\"),", rb(i), rb(i));
        }
        out.push_str("}\n");
    }
    out.push_str("\nVagrant.configure(\"2\") do |config|\n");
    out.push_str("  config.vm.synced_folder \".\", \"/vagrant\", disabled: true\n");
    out.push_str("  config.vm.boot_timeout = 600\n");

    for name in start_order(spec) {
        let m = &spec.machines[name];
        let Some(vm) = &m.vm else { continue };
        let (vbox, libvirt_box) = boxes(&vm.os).expect("checked above");
        let cpus = m.resources.and_then(|r| r.cpus).unwrap_or(crate::DEFAULT_CPUS);
        let mem = m.resources.and_then(|r| r.memory_mb).unwrap_or(crate::DEFAULT_MEMORY_MB);
        let label = format!("{} · {name}", spec.name);

        let _ = writeln!(out, "\n  config.vm.define {} do |m|", rb(name));
        let _ = writeln!(out, "    m.vm.box = {}", rb(vbox));
        let _ = writeln!(out, "    m.vm.hostname = {}", rb(name));
        for (net, octet) in &m.networks {
            let netname = format!("isoloom-{}-{net}", spec.name);
            let _ = writeln!(
                out,
                "    m.vm.network \"private_network\", ip: {}, netmask: {}, virtualbox__intnet: {}, libvirt__network_name: {}, libvirt__dhcp_enabled: false",
                rb(&address(spec, net, *octet).to_string()),
                rb(&netmask(spec, net).to_string()),
                rb(&netname),
                rb(&netname),
            );
        }
        let _ = writeln!(
            out,
            "    m.vm.provider \"virtualbox\" do |v|\n      v.name = {}\n      v.cpus = {cpus}\n      v.memory = {mem}\n    end",
            rb(&label)
        );
        let _ = writeln!(
            out,
            "    m.vm.provider \"vmware_desktop\" do |v|\n      v.vmx[\"displayName\"] = {}\n      v.vmx[\"numvcpus\"] = \"{cpus}\"\n      v.vmx[\"memsize\"] = \"{mem}\"\n    end",
            rb(&label)
        );
        let _ = writeln!(
            out,
            "    m.vm.provider \"parallels\" do |v|\n      v.name = {}\n      v.cpus = {cpus}\n      v.memory = {mem}\n    end",
            rb(&label)
        );
        match libvirt_box {
            Some(b) => {
                let _ = writeln!(
                    out,
                    "    m.vm.provider \"libvirt\" do |v, o|\n      o.vm.box = {}\n      v.cpus = {cpus}\n      v.memory = {mem}\n    end",
                    rb(b)
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "    m.vm.provider \"libvirt\" do |v|\n      v.cpus = {cpus}\n      v.memory = {mem}\n    end"
                );
            }
        }

        // The other machines by name.
        let hosts: Vec<String> = spec
            .machines
            .keys()
            .filter(|o| o.as_str() != name)
            .map(|o| format!("{} {o}", address_for(spec, name, o)))
            .collect();
        if !hosts.is_empty() {
            let _ = writeln!(
                out,
                "    m.vm.provision \"shell\", name: \"hosts\", inline: {}",
                rb(&format!(
                    "for l in {}; do grep -qxF \"$l\" /etc/hosts || echo \"$l\" >> /etc/hosts; done",
                    hosts.iter().map(|h| format!("'{h}'")).collect::<Vec<_>>().join(" ")
                ))
            );
        }

        if vm.provision.is_empty() {
            out.push_str("  end\n");
            continue;
        }
        out.push_str("    PROJECT.each do |entry|\n      m.vm.provision \"file\", source: File.join(ROOT, entry), destination: \"/tmp/isoloom-project/#{entry}\"\n    end\n");
        out.push_str("    m.vm.provision \"shell\", name: \"project\", inline: \"rm -rf /opt/isoloom && mv /tmp/isoloom-project /opt/isoloom\"\n");
        let env = if m.inputs.is_empty() {
            String::new()
        } else {
            format!(", env: INPUTS.slice({})", m.inputs.iter().map(|i| rb(i)).collect::<Vec<_>>().join(", "))
        };
        let mut ansible_ready = false;
        for step in &vm.provision {
            if step.ends_with(".sh") {
                let _ = writeln!(
                    out,
                    "    m.vm.provision \"shell\", name: {}, inline: {}{env}",
                    rb(step),
                    rb(&format!("cd /opt/isoloom && sh {step}"))
                );
            } else {
                if !ansible_ready {
                    out.push_str("    m.vm.provision \"shell\", name: \"ansible\", inline: \"command -v ansible-playbook >/dev/null || (apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq ansible-core)\"\n");
                    ansible_ready = true;
                }
                let vars = if m.inputs.is_empty() {
                    String::new()
                } else {
                    "\n      a.extra_vars = INPUTS.slice(".to_string() + &m.inputs.iter().map(|i| rb(i)).collect::<Vec<_>>().join(", ") + ")"
                };
                let _ = writeln!(
                    out,
                    "    m.vm.provision \"ansible_local\" do |a|\n      a.provisioning_path = \"/opt/isoloom\"\n      a.playbook = {}\n      a.install = false{vars}\n    end",
                    rb(step)
                );
            }
        }
        out.push_str("  end\n");
    }
    out.push_str("end\n");

    Ok(vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/Vagrantfile"),
        contents: out,
    }])
}
