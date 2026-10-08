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
//! - On QEMU (vagrant-qemu), a machine boots its libvirt-format box with its own CPU, emulated
//!   when the host's is another (x86 Windows on an Apple Silicon Mac: slow, so longer timeouts).
//!   Without root, QEMU links exactly two VMs per network (a socket listen/connect pair), each
//!   on one network; see [`qemu_refusal`].

use std::fmt::Write;
use std::net::Ipv4Addr;

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
    // Machines start one after the other, in dependency order: a machine's services are up
    // before the ones depending on it boot, the controller comes last, and on QEMU the VM that
    // listens on a network's link is up before the one that connects. QEMU and libvirt declare
    // themselves parallel, so Vagrant would otherwise boot them all at once.
    out.push_str("# One machine at a time, in order (QEMU and libvirt would boot them all at once).\nENV[\"VAGRANT_NO_PARALLEL\"] = \"1\"\n");
    out.push_str("ROOT = File.expand_path(\"../..\", __dir__)\n");
    out.push_str("# Copied into each VM: the project, without version control or generated files.\n");
    out.push_str("PROJECT = Dir.children(ROOT).reject { |e| [\".git\", \".vagrant\"].include?(e) || e.start_with?(\".isoloom\") }.sort\n");
    out.push_str("# This host's CPU as QEMU names it: a machine built for another one runs emulated there.\n");
    out.push_str("HOST_ARCH = RbConfig::CONFIG[\"host_cpu\"] =~ /arm|aarch64/ ? \"aarch64\" : \"x86_64\"\n");
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
    // A tool that can't reach the machines itself (macOS keeps third-party tools off the local
    // network) sets ISOLOOM_SSH_PROXY_COMMAND, e.g. "/usr/bin/nc %h %p", and Vagrant's SSH goes
    // through it. Unset: Vagrant connects directly, as before.
    out.push_str("  config.ssh.proxy_command = ENV[\"ISOLOOM_SSH_PROXY_COMMAND\"] if ENV[\"ISOLOOM_SSH_PROXY_COMMAND\"]\n");
    // libvirt names a domain <prefix><machine>, the prefix defaulting to this folder's name
    // ("vagrant_"): every lab's `web` was `vagrant_web`, so two labs collided and a leftover
    // couldn't be told apart. The environment's name keeps them apart (instances included).
    let _ = writeln!(
        out,
        "  config.vm.provider \"libvirt\" do |v|\n    v.default_prefix = {}\n  end",
        rb(&format!("{}_", spec.name))
    );
    // The checks, resolved early: they decide whether there is a controller VM.
    let plan = checks::plan(spec);
    let on_controller = controller_checks(spec, &plan);
    let links = qemu_links(spec, !spec.provision.is_empty() || !on_controller.is_empty());
    if router::needed(spec) {
        router_vm(spec, &mut out, &links);
    }

    // The checks, resolved: a runner script per machine that has some (uploaded by its
    // provisioner), and one for the controller.
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
        let nets: Vec<&str> = m.networks.keys().map(String::as_str).collect();
        let qemu = Qemu {
            boxed: image.qemu.as_deref(),
            arch: Some(m.arch),
            link: &links[name],
            // No cloud-init on Windows: its lab address is set from PowerShell.
            windows_address: m
                .networks
                .first()
                .filter(|_| windows)
                .map(|(net, octet)| (address(spec, net, *octet), cidr_len(spec, net))),
        };
        providers(&mut out, &label, &format!("{}-{name}", spec.name), cpus, mem, &nets, libvirt_box, &qemu);

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
            .map(|o| format!("{} {}", address_for(spec, name, o), super::names_of(spec, o)))
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
            let _ = writeln!(out, "{}", checks_provisioner(&format!("checks/{name}.sh"), group));
        }
        out.push_str("  end\n");
    }
    for (i, (name, tool)) in spec.tools.iter().enumerate() {
        if name == "shell" {
            tool_vm(spec, i, &mut out, &links);
        } else {
            let _ = writeln!(
                out,
                "\n  # Tool `{name}` ({}) runs on the container targets; no VM form.",
                tool.image.as_deref().unwrap_or("image")
            );
        }
    }
    if !spec.provision.is_empty() || !on_controller.is_empty() {
        if !on_controller.is_empty() {
            check_files.push(GeneratedFile {
                path: format!("{OUTPUT_DIR}/{DIR}/checks/controller.sh"),
                contents: checks::script(&checks::Position::Networks, &on_controller, &render),
            });
        }
        controller_vm(spec, &mut out, !on_controller.is_empty(), &links);
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
/// A position's checks, on demand (`vagrant provision --provision-with checks`). The Vagrantfile
/// is read on every `vagrant` command, so the project's check scripts are read then and written
/// over the VM's copy first: an edited script runs at the next `isoloom test`, not the copy made
/// when the VM was provisioned. Then the runner itself.
fn checks_provisioner(runner: &str, group: &[&checks::Resolved]) -> String {
    let mut scripts: Vec<&str> = group
        .iter()
        .filter_map(|c| match &c.probe {
            checks::Probe::Script { path } => Some(path.as_str()),
            _ => None,
        })
        .collect();
    scripts.dedup();
    if scripts.is_empty() {
        return format!(
            "    m.vm.provision \"shell\", name: \"checks\", run: \"never\", path: {}{CHECK_ENV}",
            rb(runner)
        );
    }
    let list = scripts.iter().map(|p| rb(p)).collect::<Vec<_>>().join(", ");
    format!(
        "    m.vm.provision \"shell\", name: \"checks\", run: \"never\", inline: [{list}].map {{ |p| \"mkdir -p /opt/isoloom/#{{File.dirname(p)}} && cat > /opt/isoloom/#{{p}} <<'ISOLOOM_EOF'\\n#{{File.read(File.join(ROOT, p))}}\\nISOLOOM_EOF\\n\" }}.join + File.read(File.join(__dir__, {})){CHECK_ENV}",
        rb(runner)
    )
}

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
            matches!(c.probe, checks::Probe::Playbook { .. }) || (!runs_on_vm(spec, &c.position) && (!c.derived || c.position == checks::Position::Networks))
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
fn tool_vm(spec: &Spec, index: usize, out: &mut String, links: &IndexMap<String, Link>) {
    let cidr = |net: &str| crate::validate::Cidr::parse(&spec.networks[net].cidr).expect("validated cidr");
    let _ = writeln!(out, "\n  # Tool `shell`: a toolbox on every network (tcpdump, nmap, curl, dig, netcat).");
    let _ = writeln!(out, "  config.vm.define {} do |m|", rb(TOOL_SHELL));
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
    let all: Vec<&str> = spec.networks.keys().map(String::as_str).collect();
    providers(
        out,
        &format!("{} · tool shell", spec.name),
        &format!("{}-tool-shell", spec.name),
        1,
        512,
        &all,
        Some(HELPER_LIBVIRT_BOX),
        &Qemu::helper(&links[TOOL_SHELL]),
    );
    let hosts: Vec<String> = spec
        .machines
        .iter()
        .filter_map(|(n, m)| {
            m.networks
                .first()
                .map(|(net, o)| format!("'{} {}'", address(spec, net, *o), super::names_of(spec, n)))
        })
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

/// The libvirt box of Isoloom's own Debian VMs (router, controller, tool shell): libvirt can't
/// run `bento/debian-12`.
const HELPER_LIBVIRT_BOX: &str = "generic/debian12";

/// Every provider's block for one VM, the same set for the lab's machines and Isoloom's own
/// (router, controller, tool shell), so a provider added here reaches all of them: `label` is
/// its display name, `guest` its ESXi guest name, `nets` the networks it is on, `libvirt_box`
/// the box libvirt uses instead of the VirtualBox one (if any), `qemu` how QEMU runs it.
#[allow(clippy::too_many_arguments)]
fn providers(out: &mut String, label: &str, guest: &str, cpus: u32, mem: u32, nets: &[&str], libvirt_box: Option<&str>, qemu: &Qemu) {
    let label = rb(label);
    let _ = writeln!(
        out,
        "    m.vm.provider \"virtualbox\" do |v|\n      v.name = {label}\n      v.linked_clone = true\n      v.cpus = {cpus}\n      v.memory = {mem}\n    end"
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"vmware_desktop\" do |v|\n      v.vmx[\"displayName\"] = {label}\n      v.vmx[\"numvcpus\"] = \"{cpus}\"\n      v.vmx[\"memsize\"] = \"{mem}\"\n    end"
    );
    let _ = writeln!(
        out,
        "    m.vm.provider \"parallels\" do |v|\n      v.name = {label}\n      v.linked_clone = true\n      v.cpus = {cpus}\n      v.memory = {mem}\n    end"
    );
    // Apple Silicon Macs: UTM and QEMU (besides VMware Fusion and Parallels above).
    let _ = writeln!(
        out,
        "    m.vm.provider \"utm\" do |v|\n      v.name = {label}\n      v.cpus = {cpus}\n      v.memory = {mem}\n    end"
    );
    qemu_block(out, cpus, mem, qemu);
    esxi(out, guest, cpus, mem, nets);
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
}

/// The define names of Isoloom's own VMs (the router's is [`router::NAME`]).
const CONTROLLER: &str = "isoloom-controller";
const TOOL_SHELL: &str = "isoloom-tool-shell";

/// A VM's link on its private network under QEMU.
#[derive(Debug, Clone, PartialEq)]
enum Link {
    /// On no network.
    None,
    /// QEMU's `socket` netdev options: `listen=` for the first VM of the pair, `connect=` for
    /// the second.
    Socket(String),
    /// Why QEMU can't give it its network (the VM then has none there).
    Refused(String),
}

/// How a VM runs under Vagrant's QEMU provider.
struct Qemu<'a> {
    /// The libvirt-format box QEMU boots instead of the main one.
    boxed: Option<&'a str>,
    /// The machine's CPU; `None` for Isoloom's own VMs, which run on the host's.
    arch: Option<Arch>,
    link: &'a Link,
    /// A Windows machine's lab address and prefix length (no cloud-init to set it).
    windows_address: Option<(Ipv4Addr, u8)>,
}

impl<'a> Qemu<'a> {
    /// Isoloom's own Debian VMs (router, controller, tool shell): the host's CPU.
    fn helper(link: &'a Link) -> Self {
        Qemu {
            boxed: Some(images::QEMU_DEBIAN_12),
            arch: None,
            link,
            windows_address: None,
        }
    }
}

/// The QEMU (vagrant-qemu) provider block.
fn qemu_block(out: &mut String, cpus: u32, mem: u32, q: &Qemu) {
    out.push_str("    m.vm.provider \"qemu\" do |v, o|\n");
    if let Some(b) = q.boxed {
        // A box of its own: the main box's version pin isn't one of its versions.
        let _ = writeln!(out, "      o.vm.box = {}\n      o.vm.box_version = \">= 0\"", rb(b));
    }
    let qemu_arch = q.arch.map(|a| match a {
        Arch::Amd64 => "x86_64",
        Arch::Arm64 => "aarch64",
    });
    if let (Some(a), Some(qa)) = (q.arch, qemu_arch) {
        // Its own CPU, whatever the host's (Vagrant otherwise picks the box for the host's).
        let _ = writeln!(out, "      o.vm.box_architecture = {}\n      v.arch = {}", rb(a.id()), rb(qa));
    }
    let _ = writeln!(out, "      v.smp = \"cpus={cpus}\"\n      v.memory = \"{mem}M\"");
    // Each VM forwards SSH from its own host port (the plugin's default is one fixed port).
    out.push_str("      v.ssh_auto_correct = true\n");
    match q.link {
        Link::None => {}
        Link::Socket(opts) => {
            let _ = writeln!(
                out,
                "      v.advanced_network = true\n      v.net_mode = :socket\n      v.socket_opts = {}",
                rb(opts)
            );
        }
        Link::Refused(why) => {
            let _ = writeln!(out, "      # No private network on QEMU: {why}.");
        }
    }
    if let Some(qa) = qemu_arch {
        // Emulated (another CPU than the host's) runs many times slower: booting and WinRM get
        // far longer to answer.
        let winrm = if q.windows_address.is_some() {
            "\n        o.winrm.retry_limit = 180\n        o.winrm.timeout = 1800"
        } else {
            ""
        };
        let _ = writeln!(out, "      if {} != HOST_ARCH\n        o.vm.boot_timeout = 3600{winrm}\n      end", rb(qa));
    }
    if let Some((ip, len)) = q.windows_address {
        let _ = writeln!(
            out,
            "      o.vm.provision \"shell\", name: \"lab network\", inline: {}",
            rb(&windows_lab_address(ip, len))
        );
    }
    out.push_str("    end\n");
}

/// PowerShell setting a Windows VM's lab address on QEMU's second adapter: the one without a
/// default gateway (the first is QEMU's user-mode NAT, through which Vagrant reaches it).
fn windows_lab_address(ip: Ipv4Addr, len: u8) -> String {
    format!(
        "$a = Get-NetAdapter | Where-Object {{ -not (Get-NetIPConfiguration -InterfaceIndex $_.ifIndex).IPv4DefaultGateway }} | Select-Object -First 1; \
         if (-not $a) {{ throw 'no second network adapter for the lab network' }}; \
         if (-not (Get-NetIPAddress -InterfaceIndex $a.ifIndex -IPAddress {ip} -ErrorAction SilentlyContinue)) {{ \
         Remove-NetIPAddress -InterfaceIndex $a.ifIndex -AddressFamily IPv4 -Confirm:$false -ErrorAction SilentlyContinue; \
         New-NetIPAddress -InterfaceIndex $a.ifIndex -IPAddress {ip} -PrefixLength {len} | Out-Null }}; \
         Set-NetConnectionProfile -InterfaceIndex $a.ifIndex -NetworkCategory Private -ErrorAction SilentlyContinue; \
         \"lab network: {ip}/{len} on $($a.Name)\""
    )
}

/// A network's prefix length.
fn cidr_len(spec: &Spec, net: &str) -> u8 {
    crate::validate::Cidr::parse(&spec.networks[net].cidr).expect("validated cidr").len
}

/// Every VM of the Vagrantfile in the order it is defined (and started), with its networks:
/// the router, the machines, the tool shell, the controller.
fn vms_and_networks(spec: &Spec, with_controller: bool) -> Vec<(String, Vec<&str>)> {
    let all: Vec<&str> = spec.networks.keys().map(String::as_str).collect();
    let mut vms = Vec::new();
    if router::needed(spec) {
        vms.push((router::NAME.to_string(), router::networks(spec).map(String::as_str).collect()));
    }
    for name in start_order(spec) {
        let m = &spec.machines[name];
        if m.vm.is_some() {
            vms.push((name.to_string(), m.networks.keys().map(String::as_str).collect()));
        }
    }
    if spec.tools.contains_key("shell") {
        vms.push((TOOL_SHELL.to_string(), all.clone()));
    }
    if with_controller {
        vms.push((CONTROLLER.to_string(), all));
    }
    vms
}

/// Each VM's link under QEMU. Without root, QEMU's `socket` netdev joins exactly two VMs (one
/// listens, the other connects, on a loopback port of the host), and the plugin gives a VM one
/// private network.
fn qemu_links(spec: &Spec, with_controller: bool) -> IndexMap<String, Link> {
    let vms = vms_and_networks(spec, with_controller);
    let members = |net: &str| -> Vec<&str> { vms.iter().filter(|(_, nets)| nets.contains(&net)).map(|(n, _)| n.as_str()).collect() };
    vms.iter()
        .map(|(name, nets)| {
            let link = match nets.as_slice() {
                [] => Link::None,
                [net] => {
                    let on = members(net);
                    if on.len() != 2 {
                        Link::Refused(format!("network `{net}` has {} VMs, and QEMU links exactly two without root", on.len()))
                    } else {
                        let side = if on[0] == name { "listen" } else { "connect" };
                        Link::Socket(format!("{side}=127.0.0.1:{}", qemu_port(&spec.name, net)))
                    }
                }
                more => Link::Refused(format!("`{name}` is on {} networks, and QEMU gives a VM one", more.len())),
            };
            (name.clone(), link)
        })
        .collect()
}

/// The host port a network's QEMU link uses: stable for the environment and network, in
/// 20000-39999.
fn qemu_port(env: &str, net: &str) -> u16 {
    // FNV-1a: stable across builds and platforms (unlike std's hasher).
    let mut h: u32 = 0x811c_9dc5;
    for b in format!("{env}/{net}").bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    20000 + (h % 20000) as u16
}

/// Why this environment's VMs can't all have their networks under Vagrant's QEMU provider, if
/// they can't: a launcher asks before offering QEMU.
pub fn qemu_refusal(spec: &Spec) -> Option<String> {
    let plan = checks::plan(spec);
    let with_controller = !spec.provision.is_empty() || !controller_checks(spec, &plan).is_empty();
    qemu_links(spec, with_controller).into_values().find_map(|l| match l {
        Link::Refused(why) => Some(why),
        _ => None,
    })
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
fn router_vm(spec: &Spec, out: &mut String, links: &IndexMap<String, Link>) {
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
    let router_nets: Vec<&str> = router::networks(spec).map(String::as_str).collect();
    providers(
        out,
        &format!("{} · router", spec.name),
        &format!("{}-router", spec.name),
        1,
        512,
        &router_nets,
        Some(HELPER_LIBVIRT_BOX),
        &Qemu::helper(&links[router::NAME]),
    );
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
fn controller_vm(spec: &Spec, out: &mut String, with_checks: bool, links: &IndexMap<String, Link>) {
    let cidr = |net: &str| crate::validate::Cidr::parse(&spec.networks[net].cidr).expect("validated cidr");
    let _ = writeln!(out, "\n  config.vm.define {} do |m|", rb(CONTROLLER));
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
    let all: Vec<&str> = spec.networks.keys().map(String::as_str).collect();
    providers(
        out,
        &format!("{} · controller", spec.name),
        &format!("{}-controller", spec.name),
        1,
        1024,
        &all,
        Some(HELPER_LIBVIRT_BOX),
        &Qemu::helper(&links[CONTROLLER]),
    );
    // Every machine by name, at its address on its first network (the controller is on all).
    let hosts: Vec<String> = spec
        .machines
        .iter()
        .filter_map(|(n, m)| {
            m.networks
                .first()
                .map(|(net, o)| format!("'{} {}'", address(spec, net, *o), super::names_of(spec, n)))
        })
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
    let script = ansible_runs(spec);
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
    // Its checks, on demand (`vagrant provision --provision-with checks`).
    if with_checks {
        let plan = checks::plan(spec);
        let scripts = controller_checks(spec, &plan);
        let _ = writeln!(out, "{}", checks_provisioner("checks/controller.sh", &scripts));
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
pub(super) fn ansible_runs(spec: &Spec) -> String {
    // Performance: gather facts once and cache them (the WinRM `setup` module is slow and the
    // environment playbooks re-run across many imported plays), fan out across hosts (default
    // forks is 5, too few for a multi-DC range), and pipeline SSH steps (a no-op over WinRM).
    let mut script = String::from(
        "set -e\nmkdir -p /tmp/isoloom-facts\nexport PATH=/opt/ansible/bin:$PATH ANSIBLE_HOST_KEY_CHECKING=False \
         ANSIBLE_GATHERING=smart ANSIBLE_FORKS=20 ANSIBLE_PIPELINING=True ANSIBLE_CACHE_PLUGIN=jsonfile \
         ANSIBLE_CACHE_PLUGIN_CONNECTION=/tmp/isoloom-facts ANSIBLE_CACHE_PLUGIN_TIMEOUT=7200\n",
    );
    script.push_str(RETRIES);
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
            Some(r) => format!("retry_download ansible-galaxy install -r /opt/isoloom/{r}\n"),
            None => "[ ! -f requirements.yml ] || retry_download ansible-galaxy install -r requirements.yml\n".into(),
        };
        script.push_str(&format!(
            "cd /opt/isoloom/{dir}\n{requirements}run_play ansible-playbook -i /etc/isoloom/inventory.ini{extra}{vars} {file}\n"
        ));
    }
    script
}

/// The shell helpers of [`ansible_runs`] (POSIX sh, safe under `set -e`):
/// - `retry_download`: role downloads (Galaxy, from GitHub) fail now and then on the network;
///   tried 5 times, with a growing pause (`ISOLOOM_RETRY_PAUSE` seconds, 15 by default).
/// - `run_play`: a play cut by a dropped connection (WinRM's shell crashing mid-task, an SSH
///   reset, a host briefly unreachable, often under CPU emulation) runs again, up to 4 times in
///   all. The play is the environment's own setup, which re-runs idempotently, so it picks up
///   where it stopped. A task that fails on its own isn't retried.
const RETRIES: &str = r#"_isoloom_transient='winrm send_input failed|The pipe has been ended|Bad HTTP response returned from server|WinRMOperationTimeoutError|UNREACHABLE!|Connection reset by peer|Connection timed out|Remote end closed connection'
_isoloom_pause=${ISOLOOM_RETRY_PAUSE:-15}
retry_download() {
  _isoloom_n=1
  until "$@"; do
    [ "$_isoloom_n" -lt 5 ] || return 1
    echo "isoloom: download failed; trying again in $((_isoloom_n * _isoloom_pause)) s ($((_isoloom_n + 1))/5)"
    sleep $((_isoloom_n * _isoloom_pause))
    _isoloom_n=$((_isoloom_n + 1))
  done
}
run_play() {
  _isoloom_n=1
  _isoloom_log=$(mktemp)
  _isoloom_rcf=$(mktemp)
  while :; do
    { "$@" && echo 0 > "$_isoloom_rcf" || echo $? > "$_isoloom_rcf"; } 2>&1 | tee "$_isoloom_log"
    _isoloom_rc=$(cat "$_isoloom_rcf")
    if [ "$_isoloom_rc" -eq 0 ]; then
      rm -f "$_isoloom_log" "$_isoloom_rcf"
      return 0
    fi
    if [ "$_isoloom_n" -lt 4 ] && grep -qE "$_isoloom_transient" "$_isoloom_log"; then
      echo "isoloom: the play was cut by a dropped connection; running it again ($((_isoloom_n + 1))/4)"
      sleep $((2 * _isoloom_pause))
      _isoloom_n=$((_isoloom_n + 1))
    else
      rm -f "$_isoloom_log" "$_isoloom_rcf"
      return "$_isoloom_rc"
    fi
  done
}
"#;

/// A single-quoted shell word.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod retry_tests {
    use super::RETRIES;

    /// Runs `body` after the retry helpers under `sh -e`, with no pauses; returns (exit code,
    /// output, how many times the fake command ran).
    fn run(body: &str) -> (i32, String, usize) {
        let dir = std::env::temp_dir().join(format!("isoloom-retries-{}-{}", std::process::id(), body.len()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = format!("set -e\nCALLS={}/calls\n{RETRIES}{body}\n", dir.display());
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(&script)
            .env("ISOLOOM_RETRY_PAUSE", "0")
            .output()
            .unwrap();
        let calls = std::fs::read_to_string(dir.join("calls")).unwrap_or_default().lines().count();
        std::fs::remove_dir_all(&dir).ok();
        (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned(), calls)
    }

    #[test]
    fn a_play_cut_by_a_dropped_connection_runs_again_and_finishes() {
        // Cut by WinRM twice (as under emulation), then it gets through.
        let (code, out, calls) = run(
            "play() { echo x >> $CALLS; [ $(wc -l < $CALLS) -ge 3 ] && echo 'failed=0' || { echo 'fatal: [dc01]: FAILED! => {\"msg\": \"winrm send_input failed\"}'; return 2; }; }\nrun_play play\necho after",
        );
        assert_eq!((code, calls), (0, 3), "{out}");
        assert!(out.contains("running it again (3/4)") && out.trim_end().ends_with("after"), "{out}");
    }

    #[test]
    fn a_task_that_fails_on_its_own_is_not_retried() {
        let (code, out, calls) =
            run("play() { echo x >> $CALLS; echo 'fatal: [dc01]: FAILED! => {\"msg\": \"no such user\"}'; return 2; }\nrun_play play\necho after");
        assert_eq!((code, calls), (2, 1), "set -e stops the setup on the play's own failure: {out}");
        assert!(!out.contains("after"));
    }

    #[test]
    fn a_play_that_keeps_dropping_stops_after_four_runs() {
        let (code, _, calls) = run("play() { echo x >> $CALLS; echo 'UNREACHABLE!'; return 4; }\nrun_play play");
        assert_eq!((code, calls), (4, 4));
    }

    #[test]
    fn a_failed_download_is_tried_again() {
        let (code, out, calls) = run("get() { echo x >> $CALLS; [ $(wc -l < $CALLS) -ge 2 ]; }\nretry_download get\necho after");
        assert_eq!((code, calls), (0, 2), "{out}");
        let (code, _, calls) = run("get() { echo x >> $CALLS; return 1; }\nretry_download get");
        assert_eq!((code, calls), (1, 5));
    }
}
