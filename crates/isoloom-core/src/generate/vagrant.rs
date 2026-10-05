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

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, address, address_for, common_unsupported, header, netmask, router, start_order};
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
    if router::needed(spec) {
        router_vm(spec, &mut out);
    }

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
        // Published services: forwarded from the host's loopback only.
        for svc in &m.services {
            if let Some(host) = svc.publish {
                let _ = writeln!(
                    out,
                    "    m.vm.network \"forwarded_port\", guest: {}, host: {host}, host_ip: \"127.0.0.1\"",
                    svc.port
                );
            }
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
        // Apple Silicon Macs: UTM and QEMU (besides VMware Fusion and Parallels above).
        let _ = writeln!(
            out,
            "    m.vm.provider \"utm\" do |v|\n      v.name = {}\n      v.cpus = {cpus}\n      v.memory = {mem}\n    end",
            rb(&label)
        );
        let _ = writeln!(
            out,
            "    m.vm.provider \"qemu\" do |v|\n      v.smp = \"cpus={cpus}\"\n      v.memory = \"{mem}M\"\n    end"
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

        // Routes to the other networks through the router and gateways, as a boot-time
        // service. The default route through a gateway comes after provisioning (below).
        let cmds = router::route_commands(spec, name, m, false);
        if !cmds.is_empty() {
            let _ = writeln!(
                out,
                "    m.vm.provision \"shell\", name: \"routes\", inline: <<~'SH'\n{}    SH",
                indent(&routes_unit(&cmds), 6)
            );
        }

        // A gateway forwards between its networks; its own provisioning sets the rules.
        if router::is_gateway(spec, name) {
            let _ = writeln!(
                out,
                "    m.vm.provision \"shell\", name: \"forwarding\", inline: {}",
                rb("echo 'net.ipv4.ip_forward=1' > /etc/sysctl.d/90-isoloom.conf && sysctl -q -p /etc/sysctl.d/90-isoloom.conf")
            );
        }

        // volumes: the VM's own disk already keeps data across restarts; the paths exist
        // before provisioning, as a Docker volume's mount point would.
        if !m.volumes.is_empty() {
            let dirs: Vec<&str> = m.volumes.values().map(String::as_str).collect();
            let _ = writeln!(
                out,
                "    m.vm.provision \"shell\", name: \"volumes\", inline: {}",
                rb(&format!("mkdir -p {}", dirs.join(" ")))
            );
        }

        // depends_on: wait until each dependency answers on its service ports.
        for dep in &m.depends_on {
            let ports: Vec<u16> = spec.machines[dep].services.iter().map(|svc| svc.port).collect();
            let _ = writeln!(
                out,
                "    m.vm.provision \"shell\", name: {}, inline: {}",
                rb(&format!("wait for {dep}")),
                rb(&router::wait_for(dep, &ports, 300))
            );
        }

        if !vm.provision.is_empty() {
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
        }

        // Once provisioned, no new connections out through the NAT interface when the machine
        // is offline, or when a gateway decides for it (Vagrant's SSH keeps working: it comes
        // in, and replies are allowed). Then the default route through the gateway.
        let online = m.networks.keys().any(|n| spec.networks[n].internet);
        let gateway = router::default_gateway(spec, name, m);
        if !online || gateway.is_some() {
            let _ = writeln!(
                out,
                "    m.vm.provision \"shell\", name: {}, inline: <<~'SH'\n{}    SH",
                rb(if gateway.is_some() { "through the gateway" } else { "no internet" }),
                indent(&egress(gateway.is_some()), 6)
            );
        }
        if gateway.is_some() {
            let _ = writeln!(
                out,
                "    m.vm.provision \"shell\", name: \"default route\", inline: <<~'SH'\n{}    SH",
                indent(&routes_unit(&router::route_commands(spec, name, m, true)), 6)
            );
        }
        out.push_str("  end\n");
    }
    out.push_str("end\n");

    Ok(vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/Vagrantfile"),
        contents: out,
    }])
}

/// Blocks new outgoing connections on the NAT interface (the default route's), in its own
/// nftables table loaded at boot, leaving the machine's other rules alone. Behind a gateway,
/// name lookups to the NAT side's resolver stay allowed (the traffic goes through the gateway).
fn egress(allow_dns: bool) -> String {
    let (dns_var, dns_rule) = if allow_dns {
        (
            "DNS=$(awk '/^nameserver/ {print $2; exit}' /etc/resolv.conf)\n",
            "    ip daddr $DNS meta l4proto { tcp, udp } th dport 53 accept\n",
        )
    } else {
        ("", "")
    };
    format!(
        "export DEBIAN_FRONTEND=noninteractive\ncommand -v nft >/dev/null || apt-get install -y -qq nftables >/dev/null\nIF=$(ip route show default | awk '{{print $5; exit}}')\n{dns_var}mkdir -p /etc/isoloom\ncat > /etc/isoloom/egress.nft <<NFT\ntable inet isoloom-egress\ndelete table inet isoloom-egress\ntable inet isoloom-egress {{\n  chain output {{\n    type filter hook output priority 0; policy accept;\n{dns_rule}    oifname \"$IF\" ct state new drop\n  }}\n}}\nNFT\ncat > /etc/systemd/system/isoloom-egress.service <<'UNIT'\n[Unit]\nDescription=No new connections out through the NAT interface\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=/usr/sbin/nft -f /etc/isoloom/egress.nft\n\n[Install]\nWantedBy=multi-user.target\nUNIT\nsystemctl daemon-reload\nsystemctl enable isoloom-egress.service\nsystemctl restart isoloom-egress.service\n"
    )
}

/// A script that installs routes as a boot-time service (and applies them now). The routes
/// live in /etc/isoloom/routes.sh, also run whenever an interface comes up (ifupdown,
/// networkd-dispatcher): bringing an interface down drops its routes, and Vagrant
/// reconfigures interfaces after every boot.
fn routes_unit(cmds: &[String]) -> String {
    let lines: String = cmds.iter().map(|c| format!("{c} 2>/dev/null || true\n")).collect();
    format!(
        "mkdir -p /etc/isoloom\ncat > /etc/isoloom/routes.sh <<'ROUTES'\n#!/bin/sh\n# Routes to the other isoloom networks.\n{lines}ROUTES\nchmod +x /etc/isoloom/routes.sh\nfor d in /etc/network/if-up.d /etc/networkd-dispatcher/routable.d; do\n  if [ -d \"$d\" ]; then ln -sf /etc/isoloom/routes.sh \"$d/zz-isoloom-routes\"; fi\ndone\ncat > /etc/systemd/system/isoloom-routes.service <<'UNIT'\n[Unit]\nDescription=Routes to the other isoloom networks\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=/etc/isoloom/routes.sh\n\n[Install]\nWantedBy=multi-user.target\nUNIT\nsystemctl daemon-reload\nsystemctl enable isoloom-routes.service\nsystemctl restart isoloom-routes.service\n"
    )
}

/// Indents every line of a script for a Ruby squiggly heredoc.
fn indent(script: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    script
        .lines()
        .map(|l| if l.is_empty() { "\n".to_string() } else { format!("{pad}{l}\n") })
        .collect()
}

/// The router VM: on every network at its last address, forwarding with the `reach` rules.
fn router_vm(spec: &Spec, out: &mut String) {
    let _ = writeln!(out, "\n  config.vm.define {} do |m|", rb(router::NAME));
    let _ = writeln!(out, "    m.vm.box = \"bento/debian-12\"");
    let _ = writeln!(out, "    m.vm.hostname = {}", rb(router::NAME));
    for net in router::networks(spec) {
        let netname = format!("isoloom-{}-{net}", spec.name);
        let _ = writeln!(
            out,
            "    m.vm.network \"private_network\", ip: {}, netmask: {}, virtualbox__intnet: {}, libvirt__network_name: {}, libvirt__dhcp_enabled: false",
            rb(&router::address(spec, net).to_string()),
            rb(&netmask(spec, net).to_string()),
            rb(&netname),
            rb(&netname),
        );
    }
    let label = format!("{} · router", spec.name);
    let _ = writeln!(
        out,
        "    m.vm.provider \"virtualbox\" do |v|\n      v.name = {}\n      v.cpus = 1\n      v.memory = 512\n    end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"vmware_desktop\" do |v|\n      v.vmx[\"displayName\"] = {}\n      v.vmx[\"numvcpus\"] = \"1\"\n      v.vmx[\"memsize\"] = \"512\"\n    end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"parallels\" do |v|\n      v.name = {}\n      v.cpus = 1\n      v.memory = 512\n    end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"libvirt\" do |v, o|\n      o.vm.box = \"generic/debian12\"\n      v.cpus = 1\n      v.memory = 512\n    end"
    );
    let script = format!(
        "set -e\nexport DEBIAN_FRONTEND=noninteractive\napt-get update -qq\napt-get install -y -qq nftables >/dev/null\necho 'net.ipv4.ip_forward=1' > /etc/sysctl.d/90-isoloom.conf\nsysctl -q -p /etc/sysctl.d/90-isoloom.conf\ncat > /etc/nftables.conf <<'NFT'\nflush ruleset\n{}NFT\nsystemctl enable nftables\nnft -f /etc/nftables.conf\n",
        router::nftables(spec)
    );
    let _ = writeln!(
        out,
        "    m.vm.provision \"shell\", name: \"router\", inline: <<~'SH'\n{}    SH",
        indent(&script, 6)
    );
    out.push_str("  end\n");
}
