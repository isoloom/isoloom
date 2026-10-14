//! The string-built helpers of the cloud drivers not on the plan yet (Azure, Google Cloud,
//! DigitalOcean, Linode, Oracle): their modules keep calling these until each moves onto the VM
//! core (`crate::vm`), which replaces them. AWS no longer uses them.
//!
//! A Terraform expression inside a set-up command is marked with [`tf_expr`] and interpolated by
//! [`hcl_cmd`]; on the core, a command is a `Template` instead.

use std::fmt::Write;

use crate::model::{Protocol, Spec};
use crate::shell;
use crate::validate::Cidr;
use crate::vm::res;
use crate::{address, hcl, router};

pub use super::aws::{image_user, instance_type, windows_udp_redirect};
pub use crate::vm::controller::OS as CONTROLLER_OS;
pub use crate::vm::windows::firewall as windows_firewall;

/// A network's block.
pub fn cidr(spec: &Spec, net: &str) -> Cidr {
    Cidr::parse(&spec.networks[net].cidr).unwrap_or(Cidr { base: 0, len: 32 })
}

/// The `checks` output of a cloud module: each runner with the machine it runs on, that
/// machine's public address and SSH user (HCL expressions from the driver), and the command.
/// A position that isn't one of the cloud's machines (the environment's networks, a machine the
/// runner supplies) runs from `fallback`: the controller, else the first Linux machine.
pub fn checks_output(spec: &Spec, public_ips: &[(String, String)], ssh_users: &[(String, String)], fallback: Option<(&str, &str)>) -> String {
    let plan = crate::checks::plan(spec);
    let mut entries = Vec::new();
    for (pos, _) in crate::checks::by_position(spec, &plan) {
        let on_machine = match &pos {
            crate::checks::Position::Machine(m) => public_ips.iter().find(|(n, _)| n == m).map(|(_, e)| {
                let user = ssh_users.iter().find(|(n, _)| n == m).map_or_else(|| "null".into(), |(_, u)| u.clone());
                (m.clone(), e.clone(), user)
            }),
            crate::checks::Position::Networks => None,
        };
        let on = on_machine.or_else(|| fallback.map(|(h, u)| ("controller".to_string(), h.to_string(), format!("\"{u}\""))));
        let Some((machine, host, user)) = on else { continue };
        entries.push(format!(
            "    {{ position = \"{id}\", machine = \"{machine}\", host = {host}, user = {user}, command = \"cd /opt/isoloom && sh .isoloom/cloud-vm/checks/{id}.sh\" }}",
            id = pos.id()
        ));
    }
    if entries.is_empty() {
        return String::new();
    }
    format!(
        "\n# The checks: each runner on the machine it stands for (ssh <user>@<host> '<command>'), or `isoloom test cloud-vm`.\noutput \"checks\" {{\n  value = [\n{}\n  ]\n}}\n",
        entries.join(",\n")
    )
}

/// HCL map entries with their `=` aligned (as `terraform fmt` writes them).
pub fn aligned(entries: &[(String, String)]) -> String {
    let w = entries.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    entries.iter().map(|(k, v)| format!("    {k:<w$} = {v}")).collect::<Vec<_>>().join("\n")
}

/// A controller runs the environment's playbooks, and the checks when no Linux machine can
/// (a Windows-only environment).
pub fn needs_controller(spec: &Spec) -> bool {
    !spec.provision.is_empty() || (!crate::checks::plan(spec).is_empty() && linux_machines(spec).next().is_none())
}

/// The Linux VMs.
pub fn linux_machines(spec: &Spec) -> impl Iterator<Item = (&String, &crate::model::Machine)> {
    spec.machines
        .iter()
        .filter(|(_, m)| m.vm.as_ref().is_some_and(|v| !crate::images::is_windows(&v.os)))
}

/// Whether a machine runs Windows.
pub fn has_windows(spec: &Spec) -> bool {
    spec.machines.values().any(|m| m.vm.as_ref().is_some_and(|v| crate::images::is_windows(&v.os)))
}

/// The redirects a machine needs: a published port that differs from the service's own port,
/// as (published port, port, protocol).
pub fn redirects(m: &crate::model::Machine) -> Vec<(u16, u16, Protocol)> {
    m.services
        .iter()
        .filter_map(|s| s.publish.filter(|h| *h != s.port).map(|h| (h, s.port, s.protocol)))
        .collect()
}

/// A Linux machine's set-up over SSH, the same on every cloud: the controller's key, its extra
/// interfaces (found by MAC, which each cloud expresses its own way), the other machines' names,
/// its volumes, its dependency waits, its provision steps, published-port redirects, the offline
/// egress rule, and the ready marker. `mac_expr(nid)` is the Terraform expression for the MAC of
/// the interface on network `nid` (the resource-safe network id).
pub fn linux_setup_cmds(spec: &Spec, name: &str, lab: &str, redirects: &[(u16, u16, Protocol)], mac_expr: &dyn Fn(&str) -> String) -> Vec<String> {
    let m = &spec.machines[name];
    let Some(vm) = &m.vm else { return Vec::new() };
    let mut cmds: Vec<String> = vec![
        "set -e".into(),
        "cloud-init status --wait >/dev/null 2>&1 || true".into(),
        "tar -xzf /tmp/isoloom-project.tgz -C /opt/isoloom --strip-components=1 && rm -f /tmp/isoloom-project.tgz".into(),
    ];
    if needs_controller(spec) {
        cmds.push(format!(
            "mkdir -p ~/.ssh && echo '{}' >> ~/.ssh/authorized_keys",
            tf_expr("trimspace(tls_private_key.controller.public_key_openssh)")
        ));
    }
    let interfaces = interface_cmds(spec, name, mac_expr);
    cmds.extend(interfaces.iter().cloned());
    cmds.extend(names_and_waits(spec, name));
    // The inputs, uploaded to /tmp: kept out of it when a reboot comes (/tmp is emptied at boot).
    let reboots = vm.provision.iter().any(|s| crate::model::is_reboot(s));
    let env = match (m.inputs.is_empty(), reboots) {
        (true, _) => "",
        (false, false) => "set -a; . /tmp/isoloom-inputs.env; set +a; ",
        (false, true) => {
            cmds.push("sudo mkdir -p /var/lib/isoloom && sudo install -m 600 /tmp/isoloom-inputs.env /var/lib/isoloom/inputs.env".into());
            "set -a; . /var/lib/isoloom/inputs.env; set +a; "
        }
    };
    for step in &vm.provision {
        cmds.extend(step_cmds(step, env, &interfaces));
    }
    cmds.extend(closing_cmds(spec, m, lab, redirects));
    cmds.push("sudo mkdir -p /var/lib/isoloom && echo ready | sudo tee /var/lib/isoloom/ready >/dev/null".into());
    cmds
}

/// The machine's other interfaces, found by MAC address, at their addresses.
fn interface_cmds(spec: &Spec, name: &str, mac_expr: &dyn Fn(&str) -> String) -> Vec<String> {
    spec.machines[name]
        .networks
        .iter()
        .skip(1)
        .map(|(n, o)| {
            format!(
                "IF=$(ip -o link | grep -i \"{mac}\" | awk -F': ' '{{print $2}}'); sudo ip link set \"$IF\" up && (ip -4 addr show \"$IF\" | grep -q {a}/ || sudo ip addr add {a}/{len} dev \"$IF\")",
                mac = mac_expr(&res(n)),
                a = address(spec, n, *o),
                len = cidr(spec, n).len,
            )
        })
        .collect()
}

/// The others by name (at their address on a network both are on, else their first), the
/// machine's settings, its volumes and its dependency waits.
fn names_and_waits(spec: &Spec, name: &str) -> Vec<String> {
    let m = &spec.machines[name];
    let mut cmds = Vec::new();
    let hosts: Vec<String> = spec
        .machines
        .keys()
        .filter(|o| o.as_str() != name)
        .map(|o| format!("{} {}", crate::address_for(spec, name, o), crate::names_of(spec, o)))
        .collect();
    if !hosts.is_empty() {
        cmds.push(format!(
            "printf '%s\\n' {} | sudo tee -a /etc/hosts >/dev/null",
            hosts.iter().map(|h| format!("'{h}'")).collect::<Vec<_>>().join(" ")
        ));
    }
    cmds.extend(crate::machine_setup::sudo_command(spec, name, m));
    if !m.volumes.is_empty() {
        cmds.push(format!("sudo mkdir -p {}", m.volumes.values().cloned().collect::<Vec<_>>().join(" ")));
    }
    for dep in &m.depends_on {
        let ports: Vec<u16> = spec.machines[dep].ready_ports();
        if !ports.is_empty() {
            cmds.push(format!("sh -c {}", shell::quote(&router::wait_for(dep, &ports, 900))));
        }
    }
    cmds
}

/// One provision step (`env`: how it reads the inputs); a reboot ends the part (see
/// [`remote_exec`]) and the next one goes on once the machine is back, its interfaces up again.
fn step_cmds(step: &str, env: &str, interfaces: &[String]) -> Vec<String> {
    if crate::model::is_reboot(step) {
        let mut cmds = vec![
            format!("sudo mkdir -p /var/lib/isoloom && sudo cp {BOOT_ID} /var/lib/isoloom/boot-id"),
            REBOOT_MARK.into(),
            "set -e".into(),
            format!(
                "if [ \"$(cat {BOOT_ID})\" = \"$(cat /var/lib/isoloom/boot-id)\" ]; then echo 'the reboot step: the machine did not restart' >&2; exit 1; fi"
            ),
            // The auto-stop scheduled at first boot didn't survive the restart: again, from now.
            tf_expr("var.auto_stop_minutes > 0 ? \"sudo shutdown -h +${var.auto_stop_minutes}\" : \"true\""),
        ];
        cmds.extend(interfaces.iter().cloned());
        cmds
    } else if step.ends_with(".sh") {
        vec![format!("cd /opt/isoloom && sudo -E sh -c {}", shell::quote(&format!("{env}sh {step}")))]
    } else {
        vec![
            "command -v ansible-playbook >/dev/null || (sudo apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq ansible-core)"
                .into(),
            format!(
                "cd /opt/isoloom && sudo -E sh -c {}",
                shell::quote(&format!("{env}ansible-playbook -c local -i localhost, {step}"))
            ),
        ]
    }
}

/// Published-port redirects and, offline, the egress rule.
fn closing_cmds(spec: &Spec, m: &crate::model::Machine, lab: &str, redirects: &[(u16, u16, Protocol)]) -> Vec<String> {
    let offline = !m.networks.keys().any(|n| spec.networks[n].internet);
    let mut cmds = Vec::new();
    if !redirects.is_empty() || offline {
        cmds.push("command -v nft >/dev/null || (sudo apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq nftables)".into());
    }
    for (h, p, proto) in redirects {
        cmds.push(format!(
            "sudo nft add table ip isoloom-publish && sudo nft 'add chain ip isoloom-publish prerouting {{ type nat hook prerouting priority -100; }}' && sudo nft add rule ip isoloom-publish prerouting {proto} dport {h} redirect to :{p}",
            proto = proto.id()
        ));
    }
    // Offline: no new connections leaving the environment, once provisioned.
    if offline {
        cmds.push(format!(
            "printf '%s\\n' 'table inet isoloom-egress {{' '  chain output {{' '    type filter hook output priority 0; policy accept;' '    ip daddr != {lab} ct state new drop' '  }}' '}}' | sudo tee /etc/isoloom-egress.nft >/dev/null && sudo nft -f /etc/isoloom-egress.nft"
        ));
    }
    cmds
}

/// This boot's id: it changes at each boot.
const BOOT_ID: &str = "/proc/sys/kernel/random/boot_id";

/// Where a `reboot` step ends a machine's set-up commands (see [`remote_exec`]).
pub const REBOOT_MARK: &str = "\u{3}reboot";

/// A machine's set-up commands as `remote-exec` provisioners: one, or one per part between its
/// `reboot` steps. Between two parts, a provisioner restarts the machine. New SSH connections
/// are refused first (logins closed with `/run/nologin`, gone at boot, and the SSH listener
/// stopped; the open session stays), so Terraform's next connection waits for the machine to
/// be back rather than reaching it on its way down. The dropped connection is expected
/// (`on_failure = continue`); the next part checks the boot changed.
pub fn remote_exec(cmds: &[String]) -> String {
    let mut out = String::new();
    for (i, part) in cmds.split(|c| c == REBOOT_MARK).enumerate() {
        if i > 0 {
            out.push_str("  # The reboot step: the connection drops as the machine goes down.\n  provisioner \"remote-exec\" {\n    inline     = [\"sudo touch /run/nologin\", \"sudo systemctl stop ssh.socket ssh.service sshd.service 2>/dev/null || true\", \"sudo systemctl --no-block reboot\", \"sleep 600\"]\n    on_failure = continue\n  }\n");
        }
        let _ = write!(
            out,
            "  provisioner \"remote-exec\" {{\n    inline = [\n{}\n    ]\n  }}\n",
            part.iter().map(|c| format!("      {}", hcl_cmd(c))).collect::<Vec<_>>().join(",\n")
        );
    }
    out
}

/// A Windows machine's project, for its steps: the host's archive of the project (as a Linux
/// machine's: `.isoloomignore` honored, the top folder stripped) uploaded over WinRM, and the
/// PowerShell unpacking it in C:\\isoloom (no tar.exe before Windows Server 2019) and stepping in.
pub fn windows_project(id: &str) -> (String, Vec<String>) {
    let tf = format!(
        "  provisioner \"local-exec\" {{\n    command = \"tar -czf \\\"${{path.module}}/.isoloom-project-{id}.tgz\\\" --exclude=.git --exclude=.vagrant --exclude=.terraform --exclude=.isoloom-project* -X \\\"${{abspath(local_file.isoloom_project.filename)}}\\\" -C \\\"${{dirname(local.root)}}\\\" \\\"${{basename(local.root)}}\\\"\"\n  }}\n  provisioner \"file\" {{\n    source      = \"${{path.module}}/.isoloom-project-{id}.tgz\"\n    destination = \"C:/Windows/Temp/isoloom-project.tgz\"\n  }}\n"
    );
    let ps = vec![
        format!(
            "$archive = 'C:\\Windows\\Temp\\isoloom-project.tgz'; $dest = 'C:\\isoloom'; $strip = 1\n{}",
            isoloom_render::UNTAR_PS1.trim_end()
        ),
        "Set-Location C:\\isoloom".to_string(),
    ];
    (tf, ps)
}

/// The controller's inventory (the shared one, `isoloom_render::inventory`): each Linux machine
/// as the AWS image's user, the Windows password marked for [`hcl_cmd`].
pub fn inventory(spec: &Spec) -> String {
    let env = isoloom_plan::Environment::build(spec, crate::model::Target::CloudVm, None);
    let password = tf_expr("random_password.windows.result");
    isoloom_render::inventory::render(&env, &isoloom_render::inventory::Access::cloud(|os| image_user(os).to_string(), &password))
}

/// A Terraform expression inside a set-up command: kept through `hcl`'s escaping (which turns
/// `${` into a literal) and turned into `${expr}` by `hcl_cmd`.
pub fn tf_expr(expr: &str) -> String {
    format!("\u{1}{expr}\u{2}")
}

/// A set-up command as an HCL string, its `tf` expressions interpolated. Only the text around
/// them is escaped: an expression's own quotes (`replace(x, "-", ":")`) stay HCL.
pub fn hcl_cmd(c: &str) -> String {
    let mut out = String::new();
    let mut rest = c;
    while let Some(start) = rest.find('\u{1}') {
        let end = rest[start..].find('\u{2}').map_or(rest.len(), |e| start + e);
        out.push_str(&hcl_inner(&rest[..start]));
        out.push_str("${");
        out.push_str(&rest[start + 1..end]);
        out.push('}');
        rest = rest.get(end + 1..).unwrap_or("");
    }
    out.push_str(&hcl_inner(rest));
    format!("\"{out}\"")
}

/// `hcl` without its surrounding quotes.
fn hcl_inner(s: &str) -> String {
    let q = hcl(s);
    q[1..q.len() - 1].to_string()
}
