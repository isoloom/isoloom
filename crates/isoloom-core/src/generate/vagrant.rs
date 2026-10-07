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

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, address, address_for, common_unsupported, header, netmask, router, start_order, trunks};
use indexmap::IndexMap;

use crate::checks;
use crate::images;
use crate::model::{Arch, Machine, Spec, Target, VmImpl};

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
    out.push_str("PROJECT = Dir.children(ROOT).reject { |e| [\".git\", \".vagrant\"].include?(e) || e.start_with?(\".isoloom\") }.sort\n");
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
    out.push_str("  config.vm.boot_timeout = 900\n");
    if router::needed(spec) {
        router_vm(spec, &mut out);
    }

    // The checks, resolved: a runner script per machine that has some (uploaded by its
    // provisioner), and one for the controller.
    let plan = checks::plan(spec);
    let groups = checks::by_position(spec, &plan);
    let mut check_files: Vec<GeneratedFile> = Vec::new();
    let host = |h: &checks::Host, _: &checks::Position| -> String {
        match h {
            checks::Host::Literal(l) => l.clone(),
            checks::Host::Machine { name, network } => address(spec, network, spec.machines[name].networks[network]).to_string(),
        }
    };
    let run_script = |path: &str| format!("cd /opt/isoloom && sh {path}");
    let run_playbook = |path: &str| {
        let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
        let file = path.rsplit('/').next().unwrap_or(path);
        format!(
            "export PATH=/opt/ansible/bin:$PATH ANSIBLE_HOST_KEY_CHECKING=False && cd /opt/isoloom/{dir} && ansible-playbook -i /etc/isoloom/inventory.ini {file}"
        )
    };
    let render = checks::Render {
        host: &host,
        script: &run_script,
        playbook: Some(&run_playbook),
    };

    let vm_trunks = trunks::vm_trunks(spec);
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
        // Only pin the architecture for non-default (arm64) boxes. amd64 is Vagrant's default;
        // setting it explicitly makes Vagrant strict-match the box's Vagrant Cloud metadata, which
        // older boxes (e.g. mayfly/windows_server2019) don't advertise, so they'd be rejected.
        if m.arch != Arch::Amd64 {
            let _ = writeln!(out, "    m.vm.box_architecture = {}", rb(m.arch.id()));
        }
        // Windows: don't set the hostname here. Vagrant renames the Windows guest from it over
        // WinRM, which is unreliable (it fails on some boxes with a misleading "not a valid name"
        // error even for a valid name, e.g. mayfly/windows10); the machine's name is set by its
        // provisioning (the lab's Ansible, or the controller) instead. Linux keeps it.
        if !windows {
            let _ = writeln!(out, "    m.vm.hostname = {}", rb(name));
        }
        if windows {
            // The box's own account, over WinRM (Windows has no SSH by default).
            // The box forwards RDP to every interface of the host: off (publish a service to
            // reach one, on the loopback).
            out.push_str("    m.vm.network \"forwarded_port\", guest: 3389, host: 3389, id: \"rdp\", disabled: true\n");
            out.push_str(
                "    m.vm.guest = :windows\n    m.vm.communicator = \"winrm\"\n    m.winrm.username = \"vagrant\"\n    m.winrm.password = \"vagrant\"\n",
            );
            match images::winrm(vm) {
                crate::model::Winrm::Ssl => out.push_str("    m.winrm.transport = :ssl\n    m.winrm.ssl_peer_verification = false\n"),
                crate::model::Winrm::Plaintext => out.push_str("    m.winrm.transport = :plaintext\n    m.winrm.basic_auth_only = true\n"),
            }
            out.push_str("    m.winrm.retry_limit = 30\n    m.winrm.retry_delay = 10\n");
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
            "    m.vm.provider \"virtualbox\" do |v|\n      v.name = {}\n      v.linked_clone = true\n      v.cpus = {cpus}\n      v.memory = {mem}\n    end",
            rb(&label)
        );
        let _ = writeln!(
            out,
            "    m.vm.provider \"vmware_desktop\" do |v|\n      v.vmx[\"displayName\"] = {}\n      v.vmx[\"numvcpus\"] = \"{cpus}\"\n      v.vmx[\"memsize\"] = \"{mem}\"\n    end",
            rb(&label)
        );
        let _ = writeln!(
            out,
            "    m.vm.provider \"parallels\" do |v|\n      v.name = {}\n      v.linked_clone = true\n      v.cpus = {cpus}\n      v.memory = {mem}\n    end",
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

        // The other machines by name. Windows machines are left out: in a domain they're
        // reached through AD DNS, and a short-name /etc/hosts entry shadows it, so a Linux
        // member's realm join fails the Kerberos SPN lookup ("Server not found").
        let hosts: Vec<String> = spec
            .machines
            .keys()
            .filter(|o| o.as_str() != name && !is_windows_machine(spec, o))
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

        // Link impairment on its own interfaces, so traffic within a network with `tc` is
        // impaired too (the router covers what it forwards into it).
        if let Some(script) = router::machine_tc_script(spec, m, |n, o| address(spec, n, o)) {
            let _ = writeln!(
                out,
                "    m.vm.provision \"shell\", name: \"tc\", inline: <<~'SH'\n{}    SH",
                indent(&tc_unit(&script, "machine"), 6)
            );
        }

        // Routes to the other networks through the router and gateways, as a boot-time
        // service, after its 802.1Q trunks (built in the VM, see `trunks`). The default route
        // through a gateway comes after provisioning (below).
        let trunk_cmds: Vec<String> = trunks::of(&vm_trunks, name)
            .flat_map(|t| trunks::vm_commands(spec, t, |n, o| address(spec, n, o)))
            .collect();
        let cmds: Vec<String> = trunk_cmds.iter().cloned().chain(router::route_commands(spec, name, m, false)).collect();
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

        // depends_on: wait until each dependency answers on its service ports. A Windows
        // dependency is waited on by address, not name: it's left out of this Linux machine's
        // /etc/hosts (so it doesn't shadow AD DNS), so only its address resolves.
        for dep in &m.depends_on {
            let ports: Vec<u16> = spec.machines[dep].services.iter().map(|svc| svc.port).collect();
            let target = if is_windows_machine(spec, dep) {
                address_for(spec, name, dep).to_string()
            } else {
                dep.clone()
            };
            let _ = writeln!(
                out,
                "    m.vm.provision \"shell\", name: {}, inline: {}",
                rb(&format!("wait for {dep}")),
                rb(&router::wait_for(&target, &ports, 300))
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
                        out.push_str(&format!("    m.vm.provision \"shell\", name: \"ansible\", inline: <<~'SH'\n      {PKG}\n      command -v ansible-playbook >/dev/null || pkg ansible-core\n    SH\n"));
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
                indent(
                    &routes_unit(
                        &trunk_cmds
                            .iter()
                            .cloned()
                            .chain(router::route_commands(spec, name, m, true))
                            .collect::<Vec<_>>()
                    ),
                    6
                )
            );
        }
        // This machine's checks, on demand (`vagrant provision --provision-with checks`): the
        // runner script next to the Vagrantfile, uploaded and run here, where the machine stands.
        if let Some(group) = groups.iter().find(|(p, _)| *p == checks::Position::Machine(name.to_string())).map(|(_, g)| g) {
            check_files.push(GeneratedFile {
                path: format!("{OUTPUT_DIR}/{DIR}/checks/{name}.sh"),
                contents: checks::script(&checks::Position::Machine(name.to_string()), group, &render),
            });
            let _ = writeln!(
                out,
                "    m.vm.provision \"shell\", name: \"checks\", run: \"never\", path: {}{CHECK_ENV}",
                rb(&format!("checks/{name}.sh"))
            );
        }
        out.push_str("  end\n");
    }
    for (i, (name, tool)) in spec.tools.iter().enumerate() {
        if name == "shell" {
            tool_vm(spec, i, &mut out);
        } else {
            let _ = writeln!(
                out,
                "\n  # Tool `{name}` ({}) runs on the container targets; no VM form.",
                tool.image.as_deref().unwrap_or("image")
            );
        }
    }
    let on_controller = controller_checks(spec, &plan);
    if !spec.provision.is_empty() || !on_controller.is_empty() {
        if !on_controller.is_empty() {
            check_files.push(GeneratedFile {
                path: format!("{OUTPUT_DIR}/{DIR}/checks/controller.sh"),
                contents: checks::script(&checks::Position::Networks, &on_controller, &render),
            });
        }
        controller_vm(spec, &mut out, !on_controller.is_empty());
    }
    out.push_str("end\n");

    let mut files = vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/Vagrantfile"),
        contents: out,
    }];
    files.extend(check_files);
    Ok(files)
}

/// The derived-checks switch, read from the host's environment when Vagrant runs.
const CHECK_ENV: &str = ", env: { \"ISOLOOM_DERIVED\" => ENV.fetch(\"ISOLOOM_DERIVED\", \"1\") }";

/// Whether a position's checks run on that machine itself: a Linux VM of the environment.
fn runs_on_vm(spec: &Spec, pos: &checks::Position) -> bool {
    match pos {
        checks::Position::Machine(m) => spec.machines[m].vm.as_ref().is_some_and(|v| !images::is_windows(&v.os)),
        checks::Position::Networks => false,
    }
}

/// The checks the controller runs: those standing on every network, Ansible playbooks, and the
/// author's checks of a machine that isn't a VM here (supplied by the runner, or a container in
/// a hybrid environment). Derived checks of such a machine are left out: the controller sees
/// every network, not that machine's view.
fn controller_checks<'a>(spec: &Spec, plan: &'a [checks::Resolved]) -> Vec<&'a checks::Resolved> {
    plan.iter()
        .filter(|c| {
            matches!(c.probe, checks::Probe::Playbook { .. }) || (!runs_on_vm(spec, &c.position) && !(c.derived && c.position != checks::Position::Networks))
        })
        .collect()
}

/// Blocks new outgoing connections on the NAT interface (the default route's), in its own
/// nftables table loaded at boot, leaving the machine's other rules alone. Behind a gateway,
/// name lookups to the NAT side's resolver stay allowed (the traffic goes through the gateway).
fn egress(allow_dns: bool) -> String {
    let pkg = PKG;
    let (dns_var, dns_rule) = if allow_dns {
        (
            "DNS=$(awk '/^nameserver/ {print $2; exit}' /etc/resolv.conf)\n",
            "    ip daddr $DNS meta l4proto { tcp, udp } th dport 53 accept\n",
        )
    } else {
        ("", "")
    };
    format!(
        "{pkg}\ncommand -v nft >/dev/null || pkg nftables >/dev/null\nIF=$(ip route show default | awk '{{print $5; exit}}')\n{dns_var}mkdir -p /etc/isoloom\ncat > /etc/isoloom/egress.nft <<NFT\ntable inet isoloom-egress\ndelete table inet isoloom-egress\ntable inet isoloom-egress {{\n  chain output {{\n    type filter hook output priority 0; policy accept;\n{dns_rule}    oifname \"$IF\" ct state new drop\n  }}\n}}\nNFT\ncat > /etc/systemd/system/isoloom-egress.service <<'UNIT'\n[Unit]\nDescription=No new connections out through the NAT interface\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=/usr/sbin/nft -f /etc/isoloom/egress.nft\n\n[Install]\nWantedBy=multi-user.target\nUNIT\nsystemctl daemon-reload\nsystemctl enable isoloom-egress.service\nsystemctl restart isoloom-egress.service\n"
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

/// The `shell` tool: a Debian VM on every network at the tool's address, with the usual
/// observation tools, outside the environment's contract.
fn tool_vm(spec: &Spec, index: usize, out: &mut String) {
    let cidr = |net: &str| crate::validate::Cidr::parse(&spec.networks[net].cidr).expect("validated cidr");
    let _ = writeln!(out, "\n  # Tool `shell`: a toolbox on every network (tcpdump, nmap, curl, dig, netcat).");
    let _ = writeln!(out, "  config.vm.define \"isoloom-tool-shell\" do |m|");
    let _ = writeln!(out, "    m.vm.box = \"bento/debian-12\"");
    let _ = writeln!(out, "    m.vm.hostname = \"shell\"");
    for net in spec.networks.keys() {
        let netname = format!("isoloom-{}-{net}", spec.name);
        let _ = writeln!(
            out,
            "    m.vm.network \"private_network\", ip: {}, netmask: {}, virtualbox__intnet: {}, libvirt__network_name: {}, libvirt__dhcp_enabled: false",
            rb(&cidr(net).tool(index).to_string()),
            rb(&netmask(spec, net).to_string()),
            rb(&netname),
            rb(&netname),
        );
    }
    let label = format!("{} · tool shell", spec.name);
    let _ = writeln!(
        out,
        "    m.vm.provider \"virtualbox\" do |v|\n      v.name = {}\n      v.linked_clone = true\n      v.cpus = 1\n      v.memory = 512\n    end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"libvirt\" do |v, o|\n      o.vm.box = \"generic/debian12\"\n      v.cpus = 1\n      v.memory = 512\n    end"
    );
    let all: Vec<&str> = spec.networks.keys().map(String::as_str).collect();
    esxi(out, &format!("{}-tool-shell", spec.name), 1, 512, &all);
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
    let _ = writeln!(
        out,
        "    m.vm.provision \"shell\", name: \"toolbox\", inline: {}",
        rb("export DEBIAN_FRONTEND=noninteractive; apt-get update -qq && apt-get install -y -qq tcpdump nmap curl dnsutils netcat-openbsd iproute2 >/dev/null")
    );
    out.push_str("  end\n");
}

/// Link impairment on a VM's interfaces (the router's, or a machine's own), re-applied at every
/// boot (netem doesn't survive one).
fn tc_unit(script: &str, whose: &str) -> String {
    format!(
        "mkdir -p /etc/isoloom\ncat > /etc/isoloom/tc.sh <<'TC'\n#!/bin/sh\n# Link impairment (networks.*.tc) on this {whose}'s interfaces.\n{script}TC\nchmod +x /etc/isoloom/tc.sh\ncat > /etc/systemd/system/isoloom-tc.service <<'UNIT'\n[Unit]\nDescription=Link impairment on the isoloom {whose}\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=/etc/isoloom/tc.sh\n\n[Install]\nWantedBy=multi-user.target\nUNIT\nsystemctl daemon-reload\nsystemctl enable isoloom-tc.service\nsystemctl restart isoloom-tc.service\n"
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

/// Installs packages with the machine's own package manager (Debian and Ubuntu, or the
/// RHEL family: Rocky, AlmaLinux, CentOS, Fedora).
const PKG: &str = "pkg() { if command -v apt-get >/dev/null; then apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \"$@\"; elif command -v dnf >/dev/null; then dnf install -y -q \"$@\"; else yum install -y -q \"$@\"; fi; }";

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
        "    m.vm.provider \"virtualbox\" do |v|\n      v.name = {}\n      v.linked_clone = true\n      v.cpus = 1\n      v.memory = 512\n    end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"vmware_desktop\" do |v|\n      v.vmx[\"displayName\"] = {}\n      v.vmx[\"numvcpus\"] = \"1\"\n      v.vmx[\"memsize\"] = \"512\"\n    end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"parallels\" do |v|\n      v.name = {}\n      v.linked_clone = true\n      v.cpus = 1\n      v.memory = 512\n    end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"libvirt\" do |v, o|\n      o.vm.box = \"generic/debian12\"\n      v.cpus = 1\n      v.memory = 512\n    end"
    );
    let router_nets: Vec<&str> = router::networks(spec).map(String::as_str).collect();
    esxi(out, &format!("{}-router", spec.name), 1, 512, &router_nets);
    let script = format!(
        "set -e\nexport DEBIAN_FRONTEND=noninteractive\napt-get update -qq\napt-get install -y -qq nftables >/dev/null\necho 'net.ipv4.ip_forward=1' > /etc/sysctl.d/90-isoloom.conf\nsysctl -q -p /etc/sysctl.d/90-isoloom.conf\ncat > /etc/nftables.conf <<'NFT'\nflush ruleset\n{}NFT\nsystemctl enable nftables\nnft -f /etc/nftables.conf\n{}",
        router::nftables(spec),
        router::tc_script(spec).map(|t| tc_unit(&t, "router")).unwrap_or_default()
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

/// Whether a machine runs Windows (used to leave Windows hosts out of `/etc/hosts`).
fn is_windows_machine(spec: &Spec, name: &str) -> bool {
    spec.machines.get(name).and_then(|m| m.vm.as_ref()).is_some_and(|vm| images::is_windows(&vm.os))
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
/// What the controller installs before Ansible: Python's venv, sshpass (password SSH), curl and
/// netcat (checks). Alpine's packages (the default box), or Debian's (a `controller.image`).
const CONTROLLER_PACKAGES: &str = "if command -v apk >/dev/null; then\n  apk add -q --no-cache python3 sshpass curl netcat-openbsd\nelse\n  export DEBIAN_FRONTEND=noninteractive\n  apt-get update -qq\n  apt-get install -y -qq python3-venv sshpass curl netcat-openbsd >/dev/null\nfi\n";

fn controller_vm(spec: &Spec, out: &mut String, with_checks: bool) {
    let cidr = |net: &str| crate::validate::Cidr::parse(&spec.networks[net].cidr).expect("validated cidr");
    let _ = writeln!(out, "\n  config.vm.define \"isoloom-controller\" do |m|");
    let (bx, version) = super::controller_box(spec);
    let (cpus, mem, _) = super::controller_size(spec);
    let _ = writeln!(out, "    m.vm.box = {}", rb(bx));
    if let Some(v) = version {
        let _ = writeln!(out, "    m.vm.box_version = {}", rb(v));
    }
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
        "    m.vm.provider \"virtualbox\" do |v|\n      v.name = {}\n      v.linked_clone = true\n      v.cpus = {cpus}\n      v.memory = {mem}\n    end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"vmware_desktop\" do |v|\n      v.vmx[\"displayName\"] = {}\n      v.vmx[\"numvcpus\"] = \"{cpus}\"\n      v.vmx[\"memsize\"] = \"{mem}\"\n    end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"parallels\" do |v|\n      v.name = {}\n      v.linked_clone = true\n      v.cpus = {cpus}\n      v.memory = {mem}\n    end",
        rb(&label)
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"libvirt\" do |v|\n      v.cpus = {cpus}\n      v.memory = {mem}\n    end"
    );
    let all: Vec<&str> = spec.networks.keys().map(String::as_str).collect();
    esxi(out, &format!("{}-controller", spec.name), cpus, mem, &all);
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
        "set -e\n{CONTROLLER_PACKAGES}[ -x /opt/ansible/bin/ansible-playbook ] || {{ python3 -m venv /opt/ansible && /opt/ansible/bin/pip install -q 'ansible-core>=2.15,<2.17' pywinrm; }}\nmkdir -p /etc/isoloom\ncat > /etc/isoloom/inventory.ini <<'INV'\n{}INV\n",
        inventory(spec)
    );
    let _ = writeln!(
        out,
        "    m.vm.provision \"shell\", name: \"controller\", inline: <<~'SH'\n{}    SH",
        indent(&script, 6)
    );
    let script = ansible_runs(spec, &[]);
    if !spec.provision.is_empty() {
        let _ = writeln!(
            out,
            "    m.vm.provision \"shell\", name: \"ansible\", inline: <<~'SH'\n{}    SH",
            indent(&script, 6)
        );
    }
    // Standing in for a user on offline networks, the controller goes offline too once it's
    // done installing (its checks would otherwise see its own NAT internet).
    if with_checks && !spec.networks.values().any(|n| n.internet) {
        let _ = writeln!(
            out,
            "    m.vm.provision \"shell\", name: \"no internet\", inline: <<~'SH'\n{}    SH",
            indent(&egress(false), 6)
        );
    }
    // Done: it halts (its disk kept) unless the spec keeps it running. Detached, so that the
    // provisioner returns before the VM goes; `isoloom test` boots it again for the checks.
    if !spec.controller.as_ref().is_some_and(|c| c.keep_running) {
        let _ = writeln!(
            out,
            "    m.vm.provision \"shell\", name: \"halt\", inline: \"touch /run/isoloom-halting; (sleep 5; poweroff) >/dev/null 2>&1 &\""
        );
    }
    // Its checks, on demand (`vagrant provision --provision-with checks`).
    if with_checks {
        let _ = writeln!(
            out,
            "    m.vm.provision \"shell\", name: \"checks\", run: \"never\", path: \"checks/controller.sh\"{CHECK_ENV}"
        );
    }
    out.push_str("  end\n");
}

/// The inventory Isoloom writes: every VM machine at its address on its first network, with its
/// connection (SSH on Linux, WinRM on Windows, the boxes' own account), and the spec's groups.
fn inventory(spec: &Spec) -> String {
    let mut linux = Vec::new();
    let mut windows = Vec::new();
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        let Some((net, octet)) = m.networks.first() else { continue };
        let mut line = format!("{name} ansible_host={}", address(spec, net, *octet));
        if images::is_windows(&vm.os) {
            // WinRM transport per host, so a lab can mix plain-HTTP and HTTPS Windows boxes.
            line.push_str(match images::winrm(vm) {
                crate::model::Winrm::Ssl => " ansible_port=5986 ansible_winrm_scheme=https ansible_winrm_transport=ntlm",
                crate::model::Winrm::Plaintext => " ansible_port=5985 ansible_winrm_scheme=http ansible_winrm_transport=basic",
            });
            windows.push(line)
        } else {
            linux.push(line)
        }
    }
    let mut inv = String::new();
    inv.push_str(&format!("[linux]\n{}\n\n[windows]\n{}\n\n", linux.join("\n"), windows.join("\n")));
    inv.push_str("[linux:vars]\nansible_user=vagrant\nansible_password=vagrant\nansible_become=true\n\n");
    inv.push_str("[windows:vars]\nansible_user=vagrant\nansible_password=vagrant\nansible_connection=winrm\nansible_winrm_server_cert_validation=ignore\nansible_winrm_operation_timeout_sec=400\nansible_winrm_read_timeout_sec=500\n");
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
    // The spec's own groups, with the machines that are VMs here.
    let resolved: Vec<(&str, Vec<String>)> = spec
        .groups
        .keys()
        .map(|g| {
            (
                g.as_str(),
                crate::groups::members(spec, g).into_iter().filter(|m| spec.machines[m].vm.is_some()).collect(),
            )
        })
        .collect();
    for (g, members) in &resolved {
        let e = groups.entry(g).or_default();
        for mbr in members {
            if !e.contains(&mbr.as_str()) {
                e.push(mbr.as_str());
            }
        }
    }
    for (g, members) in groups {
        inv.push_str(&format!("\n[{g}]\n{}\n", members.join("\n")));
    }
    inv
}

/// The controller's script running the environment's playbooks (`provision:`), in order, with
/// the inventory at /etc/isoloom/inventory.ini plus each step's own files. Shared with the
/// cloud output.
///
/// `limit`: only these machines (`isoloom provision web`); every machine when empty.
pub(super) fn ansible_runs(spec: &Spec, limit: &[String]) -> String {
    let limit = if limit.is_empty() {
        String::new()
    } else {
        format!(" --limit {}", shell_quote(&limit.join(",")))
    };
    // Performance: gather facts once and cache them (the WinRM `setup` module is slow and the
    // environment playbooks re-run across many imported plays), fan out across hosts (default
    // forks is 5, too few for a multi-DC range), and pipeline SSH steps (a no-op over WinRM).
    let mut script = String::from(
        "set -e\nmkdir -p /tmp/isoloom-facts\nexport PATH=/opt/ansible/bin:$PATH ANSIBLE_HOST_KEY_CHECKING=False \
         ANSIBLE_GATHERING=smart ANSIBLE_FORKS=20 ANSIBLE_PIPELINING=True ANSIBLE_CACHE_PLUGIN=jsonfile \
         ANSIBLE_CACHE_PLUGIN_CONNECTION=/tmp/isoloom-facts ANSIBLE_CACHE_PLUGIN_TIMEOUT=7200\n",
    );
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
            "cd /opt/isoloom/{dir}\n{requirements}ansible-playbook -i /etc/isoloom/inventory.ini{extra}{vars}{limit} {file}\n"
        ));
    }
    script
}

/// A single-quoted shell word.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
