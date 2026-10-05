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
use indexmap::IndexMap;

use crate::images;
use crate::model::{Machine, Spec, Target, VmImpl};

const DIR: &str = "vagrant";

/// Ruby double-quoted string.
fn rb(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace("#{", "\\#{"))
}

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    common_unsupported(spec, Target::Vagrant)?;
    let unsupported = |what: String| GenerateError::Unsupported { target: Target::Vagrant, what };
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        if images::vagrant(vm).is_none() {
            return Err(unsupported(format!(
                "machine `{name}`: no Vagrant box for `{}` yet; give one with `vm.image.vagrant`",
                vm.os
            )));
        }
        if images::is_windows(&vm.os) {
            windows_supported(spec, name, m).map_err(unsupported)?;
            continue;
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
    let lab_nets = spec.networks.keys().map(|n| rb(n)).collect::<Vec<_>>().join(", ");
    let _ = writeln!(
        out,
        "# ESXi (vagrant-vmware-esxi): ESXI_VIRTUAL_NETWORK lists port groups, comma-separated: the\n# management network first, then one per network, in order ({}); a missing one reuses the last.\nESXI_NETWORKS = ENV.fetch(\"ESXI_VIRTUAL_NETWORK\", \"VM Network\").split(\",\").map(&:strip)\nLAB_NETWORKS = [{lab_nets}]\ndef esxi_networks(nets)\n  [ESXI_NETWORKS[0]] + nets.map {{ |n| ESXI_NETWORKS[1 + LAB_NETWORKS.index(n)] || ESXI_NETWORKS[-1] }}\nend",
        spec.networks.keys().cloned().collect::<Vec<_>>().join(", ")
    );
    out.push_str("\nVagrant.configure(\"2\") do |config|\n");
    out.push_str("  config.vm.synced_folder \".\", \"/vagrant\", disabled: true\n");
    out.push_str("  config.vm.boot_timeout = 600\n");
    if router::needed(spec) {
        router_vm(spec, &mut out);
    }

    for name in start_order(spec) {
        let m = &spec.machines[name];
        let Some(vm) = &m.vm else { continue };
        let image = images::vagrant(vm).expect("checked above");
        let (vbox, libvirt_box) = (image.name.as_str(), image.libvirt.as_deref());
        let windows = images::is_windows(&vm.os);
        let cpus = m.resources.and_then(|r| r.cpus).unwrap_or(crate::DEFAULT_CPUS);
        let mem = m.resources.and_then(|r| r.memory_mb).unwrap_or(crate::DEFAULT_MEMORY_MB);
        let label = format!("{} · {name}", spec.name);

        let _ = writeln!(out, "\n  config.vm.define {} do |m|", rb(name));
        let _ = writeln!(out, "    m.vm.box = {}", rb(vbox));
        if let Some(v) = &image.version {
            let _ = writeln!(out, "    m.vm.box_version = {}", rb(v));
        }
        let _ = writeln!(out, "    m.vm.hostname = {}", rb(name));
        if windows {
            // The box's own account, over WinRM (Windows has no SSH by default).
            // The box forwards RDP to every interface of the host: off (publish a service to
            // reach one, on the loopback).
            out.push_str("    m.vm.network \"forwarded_port\", guest: 3389, host: 3389, id: \"rdp\", disabled: true\n");
            out.push_str("    m.vm.guest = :windows\n    m.vm.communicator = \"winrm\"\n    m.winrm.username = \"vagrant\"\n    m.winrm.password = \"vagrant\"\n    m.winrm.transport = :plaintext\n    m.winrm.basic_auth_only = true\n    m.winrm.retry_limit = 30\n    m.winrm.retry_delay = 10\n");
        }
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
        let nets: Vec<&str> = m.networks.keys().map(String::as_str).collect();
        esxi(&mut out, &format!("{}-{name}", spec.name), cpus, mem, &nets);
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

        if windows {
            windows_steps(spec, name, m, vm, &mut out);
            out.push_str("  end\n");
            continue;
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
        // The access machine runs the script checks on demand, from where a user stands.
        if access_vm(spec) == Some(name) {
            let scripts: Vec<String> = spec.checks.iter().filter(|c| c.ends_with(".sh")).map(|c| rb(c)).collect();
            if !scripts.is_empty() {
                let _ = writeln!(
                    out,
                    "    m.vm.provision \"shell\", name: \"checks\", run: \"never\", inline: \"failed=0\\n\" + [{}].map {{ |c| \"echo '== #{{c}}'\\n(\\n\" + File.read(File.join(ROOT, c)) + \"\\n) || failed=1\\n\" }}.join + \"exit $failed\\n\"",
                    scripts.join(", ")
                );
            }
        }
        out.push_str("  end\n");
    }
    if needs_controller(spec) {
        controller_vm(spec, &mut out);
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

/// The vagrant-vmware-esxi provider block: the host from ESXI_* variables, a port group per NIC.
fn esxi(out: &mut String, guest: &str, cpus: u32, mem: u32, nets: &[&str]) {
    let nets = nets.iter().map(|n| rb(n)).collect::<Vec<_>>().join(", ");
    let _ = writeln!(
        out,
        "    m.vm.provider \"vmware_esxi\" do |v|\n      v.esxi_hostname = ENV.fetch(\"ESXI_HOSTNAME\", \"\")\n      v.esxi_hostport = ENV.fetch(\"ESXI_HOSTPORT\", \"22\").to_i\n      v.esxi_username = ENV.fetch(\"ESXI_USERNAME\", \"root\")\n      v.esxi_password = \"env:ESXI_PASSWORD\"\n      v.esxi_disk_store = ENV[\"ESXI_DATASTORE\"] if ENV[\"ESXI_DATASTORE\"]\n      v.esxi_virtual_network = esxi_networks([{nets}])\n      v.guest_name = {}\n      v.guest_numvcpus = {cpus}\n      v.guest_memsize = {mem}\n    end",
        rb(guest)
    );
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
    let router_nets: Vec<&str> = router::networks(spec).map(String::as_str).collect();
    esxi(out, &format!("{}-router", spec.name), 1, 512, &router_nets);
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

/// What a Windows machine can't have on Vagrant yet, as the reason.
fn windows_supported(spec: &Spec, name: &str, m: &Machine) -> Result<(), String> {
    let vm = m.vm.as_ref().expect("a vm");
    if name.len() > 15 {
        return Err(format!("machine `{name}`: Windows computer names are at most 15 characters"));
    }
    if let Some(step) = vm.provision.iter().find(|s| !s.ends_with(".ps1")) {
        return Err(format!(
            "machine `{name}`: Windows provisioning steps are PowerShell scripts (.ps1); `{step}` isn't (Ansible runs from outside Windows: environment-level provisioning comes next)"
        ));
    }
    if !router::route_commands(spec, name, m, true).is_empty() || router::is_gateway(spec, name) {
        return Err(format!("machine `{name}`: routes between networks on Windows come later"));
    }
    if m.networks.keys().any(|n| !spec.networks[n].internet) {
        return Err(format!("machine `{name}`: `internet: false` on Windows comes later"));
    }
    if !m.volumes.is_empty() {
        return Err(format!("machine `{name}`: volumes on Windows come later"));
    }
    Ok(())
}

/// A Windows machine's steps, in PowerShell: the other machines' names, waiting for its
/// dependencies, the project in C:\\isoloom, then its own `.ps1` steps.
fn windows_steps(spec: &Spec, name: &str, m: &Machine, vm: &VmImpl, out: &mut String) {
    let hosts: Vec<String> = spec
        .machines
        .keys()
        .filter(|o| o.as_str() != name)
        .map(|o| format!("'{} {o}'", address_for(spec, name, o)))
        .collect();
    if !hosts.is_empty() {
        let script = format!(
            "$h = \"$env:SystemRoot\\System32\\drivers\\etc\\hosts\"; foreach ($l in @({})) {{ if (-not (Select-String -Path $h -SimpleMatch $l -Quiet)) {{ Add-Content -Path $h -Value $l }} }}",
            hosts.join(", ")
        );
        let _ = writeln!(out, "    m.vm.provision \"shell\", name: \"hosts\", inline: {}", rb(&script));
    }
    for dep in &m.depends_on {
        let ports: Vec<String> = spec.machines[dep].services.iter().map(|svc| svc.port.to_string()).collect();
        let script = format!(
            "$t = (Get-Date).AddSeconds(300); foreach ($p in @({ports})) {{ while (-not (Test-NetConnection {dep} -Port $p -WarningAction SilentlyContinue).TcpTestSucceeded) {{ if ((Get-Date) -gt $t) {{ throw \"{dep} didn't answer on $p\" }}; Start-Sleep 5 }} }}; \"{dep} answers\"",
            ports = ports.join(", ")
        );
        let _ = writeln!(
            out,
            "    m.vm.provision \"shell\", name: {}, inline: {}",
            rb(&format!("wait for {dep}")),
            rb(&script)
        );
    }
    // Each step is uploaded and run on its own: copying the project over WinRM is slow.
    let env = if m.inputs.is_empty() {
        String::new()
    } else {
        format!(", env: INPUTS.slice({})", m.inputs.iter().map(|i| rb(i)).collect::<Vec<_>>().join(", "))
    };
    for step in &vm.provision {
        let _ = writeln!(
            out,
            "    m.vm.provision \"shell\", name: {}, path: File.join(ROOT, {}){env}",
            rb(step),
            rb(step)
        );
    }
}

/// The controller: a Debian VM on every network at its controller address, started after every
/// machine. It writes the inventory and runs the environment-level Ansible playbooks.
fn controller_vm(spec: &Spec, out: &mut String) {
    let cidr = |net: &str| crate::validate::Cidr::parse(&spec.networks[net].cidr).expect("validated cidr");
    let _ = writeln!(out, "\n  config.vm.define \"isoloom-controller\" do |m|");
    let _ = writeln!(out, "    m.vm.box = \"bento/debian-12\"");
    let _ = writeln!(out, "    m.vm.hostname = \"isoloom-controller\"");
    for net in spec.networks.keys() {
        let netname = format!("isoloom-{}-{net}", spec.name);
        let _ = writeln!(
            out,
            "    m.vm.network \"private_network\", ip: {}, netmask: {}, virtualbox__intnet: {}, libvirt__network_name: {}, libvirt__dhcp_enabled: false",
            rb(&cidr(net).controller().to_string()),
            rb(&netmask(spec, net).to_string()),
            rb(&netname),
            rb(&netname),
        );
    }
    let label = format!("{} · controller", spec.name);
    let _ = writeln!(
        out,
        "    m.vm.provider \"virtualbox\" do |v|\n      v.name = {}\n      v.cpus = 1\n      v.memory = 1024\n    end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"vmware_desktop\" do |v|\n      v.vmx[\"displayName\"] = {}\n      v.vmx[\"numvcpus\"] = \"1\"\n      v.vmx[\"memsize\"] = \"1024\"\n    end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"parallels\" do |v|\n      v.name = {}\n      v.cpus = 1\n      v.memory = 1024\n    end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"libvirt\" do |v, o|\n      o.vm.box = \"generic/debian12\"\n      v.cpus = 1\n      v.memory = 1024\n    end"
    );
    let all: Vec<&str> = spec.networks.keys().map(String::as_str).collect();
    esxi(out, &format!("{}-controller", spec.name), 1, 1024, &all);
    // Every machine by name, at its address on its first network (the controller is on all).
    let hosts: Vec<String> = spec
        .machines
        .iter()
        .filter_map(|(n, m)| m.networks.first().map(|(net, o)| format!("'{} {n}'", address(spec, net, *o))))
        .collect();
    if !hosts.is_empty() {
        let _ = writeln!(
            out,
            "    m.vm.provision \"shell\", name: \"hosts\", inline: {}",
            rb(&format!(
                "for l in {}; do grep -qxF \"$l\" /etc/hosts || echo \"$l\" >> /etc/hosts; done",
                hosts.join(" ")
            ))
        );
    }
    out.push_str(
        "    PROJECT.each do |entry|\n      m.vm.provision \"file\", source: File.join(ROOT, entry), destination: \"/tmp/isoloom-project/#{entry}\"\n    end\n",
    );
    out.push_str("    m.vm.provision \"shell\", name: \"project\", inline: \"rm -rf /opt/isoloom && mv /tmp/isoloom-project /opt/isoloom\"\n");
    let script = format!(
        "set -e\nexport DEBIAN_FRONTEND=noninteractive\napt-get update -qq\napt-get install -y -qq python3-venv sshpass curl netcat-openbsd >/dev/null\n[ -x /opt/ansible/bin/ansible-playbook ] || {{ python3 -m venv /opt/ansible && /opt/ansible/bin/pip install -q 'ansible-core>=2.15,<2.17' pywinrm; }}\nmkdir -p /etc/isoloom\ncat > /etc/isoloom/inventory.ini <<'INV'\n{}INV\n",
        inventory(spec)
    );
    let _ = writeln!(
        out,
        "    m.vm.provision \"shell\", name: \"controller\", inline: <<~'SH'\n{}    SH",
        indent(&script, 6)
    );
    let mut script = String::from("set -e\nexport PATH=/opt/ansible/bin:$PATH ANSIBLE_HOST_KEY_CHECKING=False\n");
    for step in &spec.provision {
        let dir = step.ansible.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
        let file = step.ansible.rsplit('/').next().unwrap_or(&step.ansible);
        let extra: String = step.inventory.iter().map(|i| format!(" -i /opt/isoloom/{i}")).collect();
        // As JSON: `-e k=v` splits values on spaces.
        let vars = if step.vars.is_empty() {
            String::new()
        } else {
            format!(" -e {}", shell_quote(&serde_json::to_string(&step.vars).expect("strings serialize")))
        };
        let requirements = match &step.requirements {
            Some(r) => format!("ansible-galaxy install -r /opt/isoloom/{r}\n"),
            None => "[ ! -f requirements.yml ] || ansible-galaxy install -r requirements.yml\n".into(),
        };
        script.push_str(&format!(
            "cd /opt/isoloom/{dir}\n{requirements}ansible-playbook -i /etc/isoloom/inventory.ini{extra}{vars} {file}\n"
        ));
    }
    if !spec.provision.is_empty() {
        let _ = writeln!(
            out,
            "    m.vm.provision \"shell\", name: \"ansible\", inline: <<~'SH'\n{}    SH",
            indent(&script, 6)
        );
    }
    // Standing in for a user on offline networks, the controller goes offline too once it's
    // done installing (its checks would otherwise see its own NAT internet).
    if access_vm(spec).is_none() && !spec.checks.is_empty() && !spec.networks.values().any(|n| n.internet) {
        let _ = writeln!(
            out,
            "    m.vm.provision \"shell\", name: \"no internet\", inline: <<~'SH'\n{}    SH",
            indent(&egress(false), 6)
        );
    }
    // Checks, run on demand (`vagrant provision --provision-with checks`): Ansible ones here,
    // and the scripts too when no access machine stands where a user would.
    let mut checks = String::from("export PATH=/opt/ansible/bin:$PATH ANSIBLE_HOST_KEY_CHECKING=False\nfailed=0\n");
    let mut any = false;
    for c in &spec.checks {
        if c.ends_with(".yml") || c.ends_with(".yaml") {
            let dir = c.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
            let file = c.rsplit('/').next().unwrap_or(c);
            checks.push_str(&format!(
                "echo '== {c}'\n(cd /opt/isoloom/{dir} && ansible-playbook -i /etc/isoloom/inventory.ini {file}) || failed=1\n"
            ));
            any = true;
        } else if access_vm(spec).is_none() {
            checks.push_str(&format!("echo '== {c}'\n(cd /opt/isoloom && sh {c}) || failed=1\n"));
            any = true;
        }
    }
    if any {
        checks.push_str("exit $failed\n");
        let _ = writeln!(
            out,
            "    m.vm.provision \"shell\", name: \"checks\", run: \"never\", inline: <<~'SH'\n{}    SH",
            indent(&checks, 6)
        );
    }
    out.push_str("  end\n");
}

/// The access machine, when it's a Linux VM: script checks run there, where a user stands.
fn access_vm(spec: &Spec) -> Option<&str> {
    spec.machines
        .iter()
        .find(|(_, m)| m.access && m.vm.as_ref().is_some_and(|v| !images::is_windows(&v.os)))
        .map(|(n, _)| n.as_str())
}

/// Whether the environment gets a controller: for `provision:`, Ansible checks, or script checks
/// when no access machine can run them.
fn needs_controller(spec: &Spec) -> bool {
    !spec.provision.is_empty()
        || spec.checks.iter().any(|c| c.ends_with(".yml") || c.ends_with(".yaml"))
        || (!spec.checks.is_empty() && access_vm(spec).is_none())
}

/// The inventory Isoloom writes: every VM machine at its address on its first network, with its
/// connection (SSH on Linux, WinRM on Windows, the boxes' own account), and the spec's groups.
fn inventory(spec: &Spec) -> String {
    let mut linux = Vec::new();
    let mut windows = Vec::new();
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        let Some((net, octet)) = m.networks.first() else { continue };
        let line = format!("{name} ansible_host={}", address(spec, net, *octet));
        if images::is_windows(&vm.os) { windows.push(line) } else { linux.push(line) }
    }
    let mut inv = String::new();
    inv.push_str(&format!("[linux]\n{}\n\n[windows]\n{}\n\n", linux.join("\n"), windows.join("\n")));
    inv.push_str("[linux:vars]\nansible_user=vagrant\nansible_password=vagrant\nansible_become=true\n\n");
    inv.push_str("[windows:vars]\nansible_user=vagrant\nansible_password=vagrant\nansible_connection=winrm\nansible_port=5985\nansible_winrm_scheme=http\nansible_winrm_transport=basic\nansible_winrm_server_cert_validation=ignore\nansible_winrm_operation_timeout_sec=400\nansible_winrm_read_timeout_sec=500\n");
    let mut groups: IndexMap<&str, Vec<&str>> = IndexMap::new();
    for step in &spec.provision {
        for (g, members) in &step.groups {
            let e = groups.entry(g.as_str()).or_default();
            for mbr in members {
                if !e.contains(&mbr.as_str()) {
                    e.push(mbr);
                }
            }
        }
    }
    for (g, members) in groups {
        inv.push_str(&format!("\n[{g}]\n{}\n", members.join("\n")));
    }
    inv
}

/// A single-quoted shell word.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
