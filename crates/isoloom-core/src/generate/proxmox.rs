//! The `proxmox` target: Terraform for the bpg/proxmox provider, in `.isoloom/proxmox/main.tf`.
//!
//! - The environment gets its own SDN simple zone, and a VNet per network. No SDN subnets: a
//!   subnet's gateway lives on the host bridge, which would route between networks.
//! - A router VM sits on the server's uplink bridge and on every network at its router address.
//!   It forwards between networks with the `reach` rules (nftables), and gives the machines the
//!   internet (NAT).
//! - Each machine is a VM from a cloud image, with its fixed address on each network and the
//!   router as its gateway. cloud-init writes the project to /opt/isoloom, then runs the
//!   machine's steps: other machines' names, its volumes, waiting for its dependencies, its
//!   provisioning, and, offline, a block on new connections leaving the environment.
//! - The connection, the node, the datastores and a `slot` (unique per environment on the
//!   server: SDN ids are short) are variables.
//! - Machines are reached through the router (`ssh -J isoloom@<address>`, the `isoloom` user
//!   with `ssh_public_key`): the `machines` output gives their addresses, and the check runners
//!   in `.isoloom/proxmox/checks/<position>.sh` are piped to them by `isoloom test proxmox`
//!   (the `checks` output says which runs where).

use std::fmt::Write;

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, address, address_for, header, router, start_order, trunks};
use crate::checks::{self, Position, Probe};
use crate::images;
use crate::model::{Spec, Target};
use crate::validate::Cidr;

const DIR: &str = "proxmox";

/// Cloud images by OS name (Linux for now).
fn image_url(os: &str) -> Option<&'static str> {
    Some(match os {
        "debian-11" => "https://cloud.debian.org/images/cloud/bullseye/latest/debian-11-genericcloud-amd64.qcow2",
        "debian-12" => "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2",
        "debian-13" => "https://cloud.debian.org/images/cloud/trixie/latest/debian-13-genericcloud-amd64.qcow2",
        // End of life, still published. 14.04's cloud-init (0.7.5) is too old for the snippet.
        "ubuntu-16.04" => "https://cloud-images.ubuntu.com/xenial/current/xenial-server-cloudimg-amd64-disk1.img",
        "ubuntu-18.04" => "https://cloud-images.ubuntu.com/bionic/current/bionic-server-cloudimg-amd64.img",
        "ubuntu-20.04" => "https://cloud-images.ubuntu.com/focal/current/focal-server-cloudimg-amd64.img",
        "ubuntu-22.04" => "https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.img",
        "ubuntu-24.04" => "https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img",
        "rocky-9" => "https://dl.rockylinux.org/pub/rocky/9/images/x86_64/Rocky-9-GenericCloud-Base.latest.x86_64.qcow2",
        "almalinux-9" => "https://repo.almalinux.org/almalinux/9/cloud/x86_64/images/AlmaLinux-9-GenericCloud-latest.x86_64.qcow2",
        "centos-7" => "https://cloud.centos.org/centos/7/images/CentOS-7-x86_64-GenericCloud.qcow2",
        _ => return None,
    })
}

/// An HCL string literal (with Terraform's `${` and `%{` escaped).
pub(super) fn hcl(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace("${", "$${")
        .replace("%{", "%%{");
    format!("\"{escaped}\"")
}

fn cidr(spec: &Spec, net: &str) -> Cidr {
    Cidr::parse(&spec.networks[net].cidr).expect("validated cidr")
}

/// What this generator can't produce yet, as the reason.
fn unsupported(spec: &Spec) -> Option<String> {
    if let Some(name) = super::arm64_machine(spec) {
        return Some(format!("machine `{name}`: arm64 on Proxmox comes later (arm64 is uncommon on Proxmox hosts)"));
    }
    // Environment-level `provision:` runs from a controller VM (see `controller`), as on Vagrant
    // and the clouds; it is no longer refused here.
    if spec.networks.values().any(|n| n.gateway.is_some()) {
        return Some("networks with a `gateway` machine on Proxmox come later".into());
    }
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        if images::is_windows(&vm.os) || image_url(&vm.os).is_none() {
            return Some(format!(
                "machine `{name}`: no Proxmox image for `{}` yet (Debian, Ubuntu, Rocky, AlmaLinux, CentOS 7)",
                vm.os
            ));
        }
        // `vm.image` names Vagrant boxes only: Proxmox keeps the OS name's cloud image.
    }
    None
}

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    if let Some(what) = unsupported(spec) {
        return Err(GenerateError::Unsupported { target: Target::Proxmox, what });
    }
    let nets: Vec<&String> = spec.networks.keys().collect();
    let mut tf = header("#");
    tf.push_str("# Start:  terraform -chdir=.isoloom/proxmox init && terraform -chdir=.isoloom/proxmox apply -var proxmox_endpoint=https://<server>:8006/ -var proxmox_api_token=<user@realm!name=secret>\n# Stop:   terraform -chdir=.isoloom/proxmox destroy (same variables)\n\n");

    // Provider and connection.
    tf.push_str(
        r#"terraform {
  required_version = ">= 1.6"
  backend "local" {}
  required_providers {
    proxmox = {
      source  = "bpg/proxmox"
      version = "~> 0.115"
    }
    # The controller's SSH key (environment-level provisioning).
    tls = {
      source  = "hashicorp/tls"
      version = "~> 4.0"
    }
  }
}

variable "proxmox_endpoint" {
  type        = string
  description = "https://<server>:8006/"
}
variable "proxmox_api_token" {
  type        = string
  default     = ""
  sensitive   = true
  description = "user@realm!name=secret; or use proxmox_username and proxmox_password"
}
variable "proxmox_username" {
  type    = string
  default = "root@pam"
}
variable "proxmox_password" {
  type      = string
  default   = ""
  sensitive = true
}
variable "proxmox_insecure" {
  type        = bool
  default     = true
  description = "Accept the server's self-signed certificate"
}
variable "proxmox_ssh_username" {
  type        = string
  default     = "root"
  description = "Uploading cloud-init snippets goes over SSH to the node"
}
variable "proxmox_ssh_private_key_file" {
  type    = string
  default = ""
}
variable "proxmox_ssh_address" {
  type        = string
  default     = ""
  description = "The node's SSH address, when the API reports one this machine can't reach"
}
variable "node" {
  type    = string
  default = "pve"
}
variable "datastore" {
  type        = string
  default     = "local-lvm"
  description = "Where VM disks go"
}
variable "image_datastore" {
  type        = string
  default     = "local"
  description = "A datastore with 'iso' content, for cloud images"
}
variable "snippets_datastore" {
  type        = string
  default     = "local"
  description = "A datastore with 'snippets' content, for cloud-init"
}
variable "uplink_bridge" {
  type        = string
  default     = "vmbr0"
  description = "The bridge the router reaches the internet through"
}
variable "slot" {
  type        = number
  default     = 1
  description = "1 to 99, unique per environment on this server (SDN ids are short)"
}
variable "ssh_public_key" {
  type        = string
  default     = ""
  description = "Installed for the user `isoloom` on every VM"
}
"#,
    );
    if !spec.inputs.is_empty() {
        tf.push_str(
            "variable \"inputs\" {\n  type        = map(string)\n  default     = {}\n  sensitive   = true\n  description = \"Values given at launch\"\n}\n",
        );
    }
    tf.push_str(
        r#"
provider "proxmox" {
  endpoint  = var.proxmox_endpoint
  api_token = var.proxmox_api_token != "" ? var.proxmox_api_token : null
  username  = var.proxmox_api_token != "" ? null : var.proxmox_username
  password  = var.proxmox_api_token != "" ? null : var.proxmox_password
  insecure  = var.proxmox_insecure
  ssh {
    agent       = false
    username    = var.proxmox_ssh_username
    password    = var.proxmox_ssh_private_key_file != "" ? null : var.proxmox_password
    private_key = var.proxmox_ssh_private_key_file != "" ? file(var.proxmox_ssh_private_key_file) : null
    dynamic "node" {
      for_each = var.proxmox_ssh_address == "" ? [] : [1]
      content {
        name    = var.node
        address = var.proxmox_ssh_address
      }
    }
  }
}

locals {
  zone = "iso${var.slot}"
  # The project, written to /opt/isoloom in each machine that has steps.
  root    = abspath("${path.module}/../..")
  project = [for f in fileset(local.root, "**") : f if !startswith(f, ".git/") && !startswith(f, ".isoloom/") && !startswith(f, ".vagrant/")]
  project_files = [for f in local.project : {
    path     = "/opt/isoloom/${f}"
    encoding = "b64"
    content  = filebase64("${local.root}/${f}")
  }]
"#,
    );
    // The `isoloom` user on every VM: the operator's key and, with a controller, the controller's
    // own key too, so it can run the environment's playbooks over SSH.
    if super::cloud_vm::needs_controller(spec) {
        tf.push_str(
            "  users = [{\n    name                = \"isoloom\"\n    sudo                = \"ALL=(ALL) NOPASSWD:ALL\"\n    shell               = \"/bin/bash\"\n    ssh_authorized_keys = compact([var.ssh_public_key, trimspace(tls_private_key.controller.public_key_openssh)])\n  }]\n}\n",
        );
    } else {
        tf.push_str(
            "  users = var.ssh_public_key == \"\" ? [] : [{\n    name                = \"isoloom\"\n    sudo                = \"ALL=(ALL) NOPASSWD:ALL\"\n    shell               = \"/bin/bash\"\n    ssh_authorized_keys = [var.ssh_public_key]\n  }]\n}\n",
        );
    }
    tf.push_str(
        "\n# The environment's networks: an SDN simple zone, a VNet per network.\nresource \"proxmox_sdn_zone_simple\" \"env\" {\n  id    = local.zone\n  nodes = [var.node]\n}\n",
    );
    if super::cloud_vm::needs_controller(spec) {
        tf.push_str(
            "\n# The controller's own key: it runs the playbooks over SSH on every machine.\nresource \"tls_private_key\" \"controller\" {\n  algorithm = \"ED25519\"\n}\n",
        );
    }
    for (i, net) in nets.iter().enumerate() {
        let _ = writeln!(
            tf,
            "\n# Network `{net}`: {}\nresource \"proxmox_sdn_vnet\" \"{net_id}\" {{\n  id   = \"i${{var.slot}}n{i}\"\n  zone = proxmox_sdn_zone_simple.env.id\n}}",
            spec.networks[*net].cidr,
            net_id = res(net)
        );
    }
    let vnets: Vec<String> = nets.iter().map(|n| format!("proxmox_sdn_vnet.{}", res(n))).collect();
    let _ = writeln!(tf, "\nresource \"proxmox_sdn_applier\" \"env\" {{\n  depends_on = [{}]\n}}", vnets.join(", "));

    // Images.
    let mut oses: Vec<&str> = spec.machines.values().filter_map(|m| m.vm.as_ref()).map(|v| v.os.as_str()).collect();
    oses.push("debian-12"); // the router
    oses.sort();
    oses.dedup();
    for os in &oses {
        let _ = writeln!(
            tf,
            "\nresource \"proxmox_download_file\" \"{id}\" {{\n  node_name    = var.node\n  datastore_id = var.image_datastore\n  content_type = \"iso\"\n  url          = {url}\n  # Per environment: a shared file would go with whichever environment is destroyed first.\n  file_name           = \"iso${{var.slot}}-{os}.img\"\n  overwrite_unmanaged = true\n}}",
            id = res(os),
            url = hcl(image_url(os).expect("checked")),
        );
    }

    // The router: WAN on the uplink bridge (DHCP), a NIC per network at its router address,
    // matched by MAC.
    let router_nics: Vec<(String, String)> = nets
        .iter()
        .enumerate()
        .map(|(i, n)| (format!("format(\"02:15:%02x:01:%02x:00\", var.slot, {i})"), (*n).clone()))
        .collect();
    let mut nft = String::from(
        "flush ruleset\ntable inet isoloom {\n  chain forward {\n    type filter hook forward priority 0; policy drop;\n    ct state established,related accept\n",
    );
    let all: Vec<String> = nets.iter().map(|n| spec.networks[*n].cidr.clone()).collect();
    let lab = format!("{{ {} }}", all.join(", "));
    for r in &spec.reach {
        let from = &spec.networks[&r.from].cidr;
        let to = &spec.networks[&r.to].cidr;
        if r.ports.is_empty() {
            let _ = writeln!(nft, "    ip saddr {from} ip daddr {to} accept");
        } else {
            let ports = r.ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", ");
            let _ = writeln!(
                nft,
                "    ip saddr {from} ip daddr {to} meta l4proto {{ tcp, udp }} th dport {{ {ports} }} accept"
            );
            let _ = writeln!(nft, "    ip saddr {from} ip daddr {to} icmp type echo-request accept");
        }
    }
    // Published services: forwarded from the router's uplink address (anything not from the lab).
    let published = published(spec);
    let mut dnat = String::new();
    for (_, _, addr, port, host) in &published {
        let _ = writeln!(nft, "    ip daddr {addr} tcp dport {port} ct status dnat accept");
        let _ = writeln!(dnat, "    ip saddr != {lab} tcp dport {host} dnat ip to {addr}:{port}");
    }
    // The internet, for every network (machines install their software); offline machines block
    // it themselves once provisioned, as on local VMs.
    let _ = writeln!(
        nft,
        "    ip saddr {lab} ip daddr != {lab} accept\n  }}\n  chain prerouting {{\n    type nat hook prerouting priority -100;\n{dnat}  }}\n  chain postrouting {{\n    type nat hook postrouting priority 100;\n    ip saddr {lab} ip daddr != {lab} masquerade\n  }}\n}}"
    );
    let mut write_files = vec![
        "{ path = \"/etc/systemd/network/10-wan.network\", content = join(\"\\n\", [\"[Match]\", \"MACAddress=${format(\"02:15:%02x:00:00:00\", var.slot)}\", \"\", \"[Network]\", \"DHCP=yes\"]) }".to_string(),
        format!("{{ path = \"/etc/nftables.conf\", content = {} }}", hcl(&nft)),
        "{ path = \"/etc/sysctl.d/90-isoloom.conf\", content = \"net.ipv4.ip_forward=1\\n\" }".to_string(),
    ];
    for (mac, net) in &router_nics {
        let c = cidr(spec, net);
        write_files.push(format!(
            "{{ path = \"/etc/systemd/network/20-{net}.network\", content = join(\"\\n\", [\"[Match]\", \"MACAddress=${{{mac}}}\", \"\", \"[Network]\", \"Address={}/{}\", \"ConfigureWithoutCarrier=yes\"]) }}",
            router::address(spec, net),
            c.len
        ));
    }
    let _ = writeln!(
        tf,
        "\n# The router: forwards between networks with the reach rules, and to the internet.\nresource \"proxmox_virtual_environment_file\" \"router\" {{\n  node_name    = var.node\n  datastore_id = var.snippets_datastore\n  content_type = \"snippets\"\n  source_raw {{\n    file_name = \"iso${{var.slot}}-router.yaml\"\n    data = \"#cloud-config\\n${{yamlencode({{\n      hostname = \"isoloom-router\"\n      users    = local.users\n      packages = [\"nftables\", \"qemu-guest-agent\"]\n      write_files = [\n        {}\n      ]\n      runcmd = [\n        [\"sysctl\", \"-p\", \"/etc/sysctl.d/90-isoloom.conf\"],\n        [\"systemctl\", \"enable\", \"systemd-networkd\"],\n        [\"systemctl\", \"restart\", \"systemd-networkd\"],\n        [\"systemctl\", \"enable\", \"--now\", \"nftables\"],\n        [\"nft\", \"-f\", \"/etc/nftables.conf\"],\n        [\"systemctl\", \"enable\", \"--now\", \"qemu-guest-agent\"],\n      ]\n    }})}}\"\n  }}\n}}",
        write_files.join(",\n        ")
    );
    let mut router_nets =
        "  network_device {\n    bridge      = var.uplink_bridge\n    mac_address = upper(format(\"02:15:%02x:00:00:00\", var.slot))\n  }\n".to_string();
    for (mac, net) in &router_nics {
        let _ = writeln!(
            router_nets,
            "  network_device {{\n    bridge      = proxmox_sdn_vnet.{}.id\n    mac_address = upper({mac})\n  }}",
            res(net)
        );
    }
    let _ = writeln!(
        tf,
        "\nresource \"proxmox_virtual_environment_vm\" \"isoloom_router\" {{\n  name      = \"iso${{var.slot}}-router\"\n  node_name = var.node\n  tags      = [\"isoloom\", \"{env}\"]\n  on_boot   = false\n  # Its uplink address (DHCP), for the published ports.\n  agent {{\n    enabled = true\n  }}\n  cpu {{\n    cores = 1\n    type  = \"host\"\n  }}\n  memory {{\n    dedicated = 512\n  }}\n  disk {{\n    datastore_id = var.datastore\n    file_id      = proxmox_download_file.{img}.id\n    interface    = \"virtio0\"\n    size         = 8\n  }}\n{router_nets}  initialization {{\n    datastore_id      = var.datastore\n    user_data_file_id = proxmox_virtual_environment_file.router.id\n    ip_config {{\n      ipv4 {{\n        address = \"dhcp\"\n      }}\n    }}\n  }}\n  operating_system {{\n    type = \"l26\"\n  }}\n  serial_device {{}}\n  depends_on = [proxmox_sdn_applier.env]\n}}",
        env = spec.name,
        img = res("debian-12"),
    );

    // The checks: a runner per position, piped to its machine over SSH through the router by
    // `isoloom test proxmox`. A machine whose runner runs a script needs the project.
    let plan = checks::plan(spec);
    let groups = checks::by_position(spec, &plan);
    let runs_script: Vec<&str> = groups
        .iter()
        .filter(|(_, g)| g.iter().any(|c| matches!(c.probe, Probe::Script { .. })))
        .filter_map(|(p, _)| match p {
            Position::Machine(m) => Some(m.as_str()),
            _ => None,
        })
        .collect();

    // The machines, in start order.
    for name in start_order(spec) {
        let m = &spec.machines[name];
        let Some(vm) = &m.vm else { continue };
        let cpus = m.resources.and_then(|r| r.cpus).unwrap_or(crate::DEFAULT_CPUS);
        let mem = m.resources.and_then(|r| r.memory_mb).unwrap_or(crate::DEFAULT_MEMORY_MB);
        let disk = m.resources.and_then(|r| r.disk_gb).unwrap_or(crate::DEFAULT_DISK_GB);

        let mut runcmd: Vec<String> = Vec::new();
        // Its 802.1Q trunks, in the VM (see `trunks`; Proxmox's bridges learn the trunk's MAC).
        let vm_trunks = trunks::vm_trunks(spec);
        runcmd.extend(trunks::of(&vm_trunks, name).flat_map(|t| trunks::vm_commands(spec, t, |n, o| address(spec, n, o))));
        let hosts: Vec<String> = spec
            .machines
            .keys()
            .filter(|o| o.as_str() != name)
            .map(|o| format!("{} {}", address_for(spec, name, o), super::names_of(spec, o)))
            .collect();
        if !hosts.is_empty() {
            runcmd.push(format!(
                "printf '%s\\n' {} >> /etc/hosts",
                hosts.iter().map(|h| format!("'{h}'")).collect::<Vec<_>>().join(" ")
            ));
        }
        if !m.volumes.is_empty() {
            runcmd.push(format!("mkdir -p {}", m.volumes.values().cloned().collect::<Vec<_>>().join(" ")));
        }
        for dep in &m.depends_on {
            let ports: Vec<u16> = spec.machines[dep].ready_ports();
            runcmd.push(router::wait_for(dep, &ports, 600));
        }
        let env = if m.inputs.is_empty() {
            String::new()
        } else {
            "set -a; . /etc/isoloom/inputs.env; set +a; ".to_string()
        };
        let mut ansible = false;
        for step in &vm.provision {
            if step.ends_with(".sh") {
                runcmd.push(format!("cd /opt/isoloom && {env}sh {step}"));
            } else {
                if !ansible {
                    runcmd.push(
                        "command -v ansible-playbook >/dev/null || (apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq ansible-core)"
                            .into(),
                    );
                    ansible = true;
                }
                runcmd.push(format!("cd /opt/isoloom && {env}ansible-playbook -c local -i localhost, {step}"));
            }
        }
        // Offline: no new connections leaving the environment, once provisioned.
        if !m.networks.keys().any(|n| spec.networks[n].internet) {
            runcmd.push(format!(
                "printf '%s\\n' 'table inet isoloom-egress {{' '  chain output {{' '    type filter hook output priority 0; policy accept;' '    ip daddr != {lab} ct state new drop' '  }}' '}}' > /etc/isoloom-egress.nft && nft -f /etc/isoloom-egress.nft && echo 'nft -f /etc/isoloom-egress.nft' > /etc/rc.local && chmod +x /etc/rc.local"
            ));
        }
        runcmd.push("mkdir -p /var/lib/isoloom && echo ready > /var/lib/isoloom/ready".into());

        let mut files = Vec::new();
        if !vm.provision.is_empty() || runs_script.contains(&name) {
            files.push("local.project_files".to_string());
        }
        if !m.inputs.is_empty() {
            let lines: Vec<String> = m.inputs.iter().map(|i| format!("\"{i}=${{lookup(var.inputs, \"{i}\", \"\")}}\"")).collect();
            files.push(format!(
                "[{{ path = \"/etc/isoloom/inputs.env\", permissions = \"0600\", content = join(\"\\n\", [{}]) }}]",
                lines.join(", ")
            ));
        }
        let write_files = if files.is_empty() {
            "[]".to_string()
        } else if files.len() == 1 {
            files[0].clone()
        } else {
            format!("concat({})", files.join(", "))
        };
        let runcmd_hcl = runcmd
            .iter()
            .map(|c| format!("[\"sh\", \"-c\", {}]", hcl(c)))
            .collect::<Vec<_>>()
            .join(",\n        ");
        let id = res(name);
        let _ = writeln!(
            tf,
            "\n# Machine `{name}`.\nresource \"proxmox_virtual_environment_file\" \"{id}\" {{\n  node_name    = var.node\n  datastore_id = var.snippets_datastore\n  content_type = \"snippets\"\n  source_raw {{\n    file_name = \"iso${{var.slot}}-{name}.yaml\"\n    data = \"#cloud-config\\n${{yamlencode({{\n      hostname    = \"{name}\"\n      users       = local.users\n      packages    = [\"nftables\", \"curl\", \"netcat-openbsd\"]\n      write_files = {write_files}\n      runcmd = [\n        {runcmd_hcl}\n      ]\n    }})}}\"\n  }}\n}}"
        );
        let mut nics = String::new();
        let mut ipcfg = String::new();
        for (i, (net, octet)) in m.networks.iter().enumerate() {
            let c = cidr(spec, net);
            let _ = writeln!(nics, "  network_device {{\n    bridge = proxmox_sdn_vnet.{}.id\n  }}", res(net));
            let gw = if i == 0 {
                format!("\n        gateway = \"{}\"", router::address(spec, net))
            } else {
                String::new()
            };
            let _ = writeln!(
                ipcfg,
                "    ip_config {{\n      ipv4 {{\n        address = \"{}/{}\"{gw}\n      }}\n    }}",
                address(spec, net, *octet),
                c.len
            );
        }
        // Honour the machine's resolver when it sets one (an AD member points at the domain
        // controller); otherwise a public resolver so it can still install software.
        let servers = match m.dns.as_ref().filter(|d| !d.servers.is_empty()) {
            Some(d) => d.servers.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", "),
            None => "\"1.1.1.1\"".into(),
        };
        let domain = m
            .dns
            .as_ref()
            .and_then(|d| d.domain.as_deref())
            .map(|d| format!("      domain = \"{d}\"\n"))
            .unwrap_or_default();
        let dns = format!("    dns {{\n      servers = [{servers}]\n{domain}    }}\n");
        let _ = writeln!(
            tf,
            "\nresource \"proxmox_virtual_environment_vm\" \"{id}\" {{\n  name      = \"iso${{var.slot}}-{name}\"\n  node_name = var.node\n  tags      = [\"isoloom\", \"{env}\"]\n  on_boot   = false\n  cpu {{\n    cores = {cpus}\n    type  = \"host\"\n  }}\n  memory {{\n    dedicated = {mem}\n  }}\n  disk {{\n    datastore_id = var.datastore\n    file_id      = proxmox_download_file.{img}.id\n    interface    = \"virtio0\"\n    size         = {disk}\n  }}\n{nics}  initialization {{\n    datastore_id      = var.datastore\n    user_data_file_id = proxmox_virtual_environment_file.{id}.id\n{dns}{ipcfg}  }}\n  operating_system {{\n    type = \"l26\"\n  }}\n  serial_device {{}}\n  depends_on = [proxmox_virtual_environment_vm.isoloom_router{deps}]\n}}",
            env = spec.name,
            img = res(&vm.os),
            deps = m
                .depends_on
                .iter()
                .map(|d| format!(", proxmox_virtual_environment_vm.{}", res(d)))
                .collect::<String>(),
        );
    }

    // Environment-level `provision:`: a controller runs the playbooks once every machine is up.
    if super::cloud_vm::needs_controller(spec) {
        controller(spec, &mut tf, &nets);
    }

    // Where the environment is reached from outside: the router's uplink address (the guest
    // agent reports it), and each published service there.
    let lab_addrs: Vec<String> = nets.iter().map(|n| format!("\"{}\"", router::address(spec, n))).collect();
    let _ = writeln!(
        tf,
        "\nlocals {{\n  router_address = [for a in flatten(proxmox_virtual_environment_vm.isoloom_router.ipv4_addresses) : a if a != \"127.0.0.1\" && !contains([{}], a)][0]\n}}\n\noutput \"address\" {{\n  value = local.router_address\n}}",
        lab_addrs.join(", ")
    );
    if !published.is_empty() {
        tf.push_str("\n# Published services, from the router's uplink address.\noutput \"published\" {\n  value = {\n");
        for (machine, service, _, _, host) in &published {
            let _ = writeln!(tf, "    \"{machine}/{service}\" = \"${{local.router_address}}:{host}\"");
        }
        tf.push_str("  }\n}\n");
    }

    // Each machine at its first address, reached through the router as the `isoloom` user.
    let first_address = |name: &str| spec.machines[name].networks.first().map(|(net, o)| address(spec, net, *o));
    let machine_addrs: Vec<(String, String)> = spec
        .machines
        .iter()
        .filter(|(_, m)| m.vm.is_some())
        .filter_map(|(n, _)| first_address(n).map(|a| (format!("\"{n}\""), format!("\"{a}\""))))
        .collect();
    let _ = write!(
        tf,
        "\n# The machines, through the router: ssh -J isoloom@<address> isoloom@<machine>.\noutput \"machines\" {{\n  value = {{\n{}\n  }}\n}}\n\noutput \"ssh_user\" {{\n  value = \"isoloom\"\n}}\n",
        super::cloud_vm::aligned(&machine_addrs)
    );

    // The check runners: each on the machine it stands for; a position that isn't one of the
    // VMs (the environment's networks, a machine the runner supplies) runs from the controller,
    // else the first machine.
    let fallback: Option<(String, std::net::Ipv4Addr)> = if super::cloud_vm::needs_controller(spec) {
        Some(("controller".to_string(), cidr(spec, nets[0]).controller()))
    } else {
        start_order(spec)
            .into_iter()
            .find(|n| spec.machines[*n].vm.is_some())
            .and_then(|n| first_address(n).map(|a| (n.to_string(), a)))
    };
    let mut entries = Vec::new();
    for (pos, _) in &groups {
        let on = match pos {
            Position::Machine(m) if spec.machines[m].vm.is_some() => first_address(m).map(|a| (m.clone(), a)),
            _ => fallback.clone(),
        };
        let Some((machine, host)) = on else { continue };
        entries.push(format!(
            "    {{ position = \"{id}\", machine = \"{machine}\", host = \"{host}\", user = \"isoloom\", script = \"{OUTPUT_DIR}/{DIR}/checks/{id}.sh\" }}",
            id = pos.id()
        ));
    }
    if !entries.is_empty() {
        let _ = write!(
            tf,
            "\n# The checks: each runner piped to its machine (ssh -J isoloom@<address> isoloom@<host> sh -s < <script>), or `isoloom test proxmox`.\noutput \"checks\" {{\n  value = [\n{}\n  ]\n}}\n",
            entries.join(",\n")
        );
    }

    let mut files = vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/main.tf"),
        contents: tf,
    }];
    let host = |h: &checks::Host, _: &Position| -> String {
        match h {
            checks::Host::Literal(l) => l.clone(),
            checks::Host::Machine { name, network } => address(spec, network, spec.machines[name].networks[network]).to_string(),
        }
    };
    let run_script = |path: &str| format!("cd /opt/isoloom && sh {path}");
    let render = checks::Render {
        host: &host,
        script: &run_script,
        playbook: None,
    };
    for (pos, group) in &groups {
        files.push(GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/checks/{}.sh", pos.id()),
            contents: checks::script(pos, group, &render),
        });
    }
    Ok(files)
}

/// The controller: a Debian VM that runs the environment's playbooks once every machine is up,
/// as on Vagrant and the clouds. It sits on the uplink bridge (DHCP: internet, to install
/// Ansible) and on every network at the controller address. Its cloud-init carries the project,
/// its own key and the inventory; it then waits for each machine's ready marker over SSH and
/// runs the playbooks.
fn controller(spec: &Spec, tf: &mut String, nets: &[&String]) {
    let env = &spec.name;
    let mut runcmd: Vec<String> = Vec::new();
    // /etc/hosts: every machine at its first address.
    let hosts: Vec<String> = spec
        .machines
        .iter()
        .filter_map(|(o, om)| {
            om.networks
                .first()
                .map(|(n, oc)| format!("'{} {}'", address(spec, n, *oc), super::names_of(spec, o)))
        })
        .collect();
    if !hosts.is_empty() {
        runcmd.push(format!("printf '%s\\n' {} >> /etc/hosts", hosts.join(" ")));
    }
    // Ansible in a venv (pywinrm for the Windows machines, when Proxmox learns them).
    runcmd.push("python3 -m venv /opt/ansible && /opt/ansible/bin/pip install -q 'ansible-core>=2.15,<2.17' pywinrm".into());
    // Each Linux machine's own set-up must be done (its ready marker) before the playbooks run.
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        if images::is_windows(&vm.os) {
            continue;
        }
        let Some((net, octet)) = m.networks.first() else { continue };
        runcmd.push(format!(
            "i=0; until ssh -o StrictHostKeyChecking=no -o ConnectTimeout=5 -i /etc/isoloom/id_ed25519 isoloom@{a} test -f /var/lib/isoloom/ready 2>/dev/null; do i=$((i+1)); [ $i -gt 240 ] && {{ echo '{name} not ready'; exit 1; }}; sleep 5; done",
            a = address(spec, net, *octet)
        ));
    }
    runcmd.push(format!("sh -c {}", super::cloud_vm::sh_quote(&super::vagrant::ansible_runs(spec))));
    runcmd.push("mkdir -p /var/lib/isoloom && echo ready > /var/lib/isoloom/ready".into());
    let runcmd_hcl = runcmd
        .iter()
        .map(|c| format!("[\"sh\", \"-c\", {}]", hcl(c)))
        .collect::<Vec<_>>()
        .join(",\n        ");
    // Its files: the project, its key, the inventory.
    let write_files = format!(
        "concat(local.project_files, [{{ path = \"/etc/isoloom/id_ed25519\", permissions = \"0600\", content = tls_private_key.controller.private_key_openssh }}, {{ path = \"/etc/isoloom/inventory.ini\", permissions = \"0644\", content = {} }}])",
        hcl(&inventory(spec))
    );
    let _ = writeln!(
        tf,
        "\n# The controller: runs the environment's playbooks once every machine is up.\nresource \"proxmox_virtual_environment_file\" \"controller\" {{\n  node_name    = var.node\n  datastore_id = var.snippets_datastore\n  content_type = \"snippets\"\n  source_raw {{\n    file_name = \"iso${{var.slot}}-controller.yaml\"\n    data = \"#cloud-config\\n${{yamlencode({{\n      hostname    = \"isoloom-controller\"\n      users       = local.users\n      packages    = [\"python3-venv\", \"curl\", \"netcat-openbsd\", \"openssh-client\"]\n      write_files = {write_files}\n      runcmd = [\n        {runcmd_hcl}\n      ]\n    }})}}\"\n  }}\n}}"
    );
    // NICs: the uplink (DHCP, its route out) first, then every network at the controller address.
    let mut nics = String::from("  network_device {\n    bridge = var.uplink_bridge\n  }\n");
    let mut ipcfg = String::from("    ip_config {\n      ipv4 {\n        address = \"dhcp\"\n      }\n    }\n");
    for net in nets {
        let c = cidr(spec, net);
        let _ = writeln!(nics, "  network_device {{\n    bridge = proxmox_sdn_vnet.{}.id\n  }}", res(net));
        let _ = writeln!(
            ipcfg,
            "    ip_config {{\n      ipv4 {{\n        address = \"{}/{}\"\n      }}\n    }}",
            c.controller(),
            c.len
        );
    }
    let deps: String = spec
        .machines
        .iter()
        .filter(|(_, m)| m.vm.is_some())
        .map(|(n, _)| format!(", proxmox_virtual_environment_vm.{}", res(n)))
        .collect();
    let _ = writeln!(
        tf,
        "\nresource \"proxmox_virtual_environment_vm\" \"isoloom_controller\" {{\n  name      = \"iso${{var.slot}}-controller\"\n  node_name = var.node\n  tags      = [\"isoloom\", \"{env}\"]\n  on_boot   = false\n  cpu {{\n    cores = 1\n    type  = \"host\"\n  }}\n  memory {{\n    dedicated = 1024\n  }}\n  disk {{\n    datastore_id = var.datastore\n    file_id      = proxmox_download_file.{img}.id\n    interface    = \"virtio0\"\n    size         = 8\n  }}\n{nics}  initialization {{\n    datastore_id      = var.datastore\n    user_data_file_id = proxmox_virtual_environment_file.controller.id\n    dns {{\n      servers = [\"1.1.1.1\"]\n    }}\n{ipcfg}  }}\n  operating_system {{\n    type = \"l26\"\n  }}\n  serial_device {{}}\n  depends_on = [proxmox_virtual_environment_vm.isoloom_router{deps}]\n}}",
        img = res(super::cloud_vm::CONTROLLER_OS),
    );
}

/// The controller's inventory: every machine at its first address, as the `isoloom` user with
/// the controller's key (cloud-init made that user on each VM); then the spec's groups.
fn inventory(spec: &Spec) -> String {
    let (mut linux, mut windows) = (String::new(), String::new());
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        let Some((net, octet)) = m.networks.first() else { continue };
        if images::is_windows(&vm.os) {
            let _ = writeln!(windows, "{name} ansible_host={}", address(spec, net, *octet));
        } else {
            let _ = writeln!(linux, "{name} ansible_host={} ansible_user=isoloom", address(spec, net, *octet));
        }
    }
    let mut inv = format!("[linux]\n{linux}\n[windows]\n{windows}\n[linux:vars]\nansible_ssh_private_key_file=/etc/isoloom/id_ed25519\nansible_become=true\n");
    let mut groups: indexmap::IndexMap<&str, Vec<&str>> = indexmap::IndexMap::new();
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
        let _ = write!(inv, "\n[{g}]\n{}\n", members.join("\n"));
    }
    inv
}

/// A Terraform resource name from a machine, network or OS name.
pub(super) fn res(s: &str) -> String {
    s.replace(['-', '.'], "_")
}

/// Published services: (machine, service name, machine address, port, published port).
fn published(spec: &Spec) -> Vec<(String, String, std::net::Ipv4Addr, u16, u16)> {
    let mut out = Vec::new();
    for (name, m) in &spec.machines {
        let Some((net, octet)) = m.networks.first() else { continue };
        for sv in &m.services {
            if let Some(host) = sv.publish {
                let label = sv.name.clone().unwrap_or_else(|| sv.port.to_string());
                out.push((name.clone(), label, super::address(spec, net, *octet), sv.port, host));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{generate, image_url};
    use crate::images::is_windows;
    use crate::model::KNOWN_OS;

    const WITH_CHECKS: &str = "version: 1
name: px
networks:
  lab: { cidr: 10.9.0.0/24 }
machines:
  web:
    networks: { lab: 10 }
    services: [{ port: 80, http: true }]
    vm: { os: debian-12 }
  probe:
    networks: { lab: 11 }
    vm: { os: debian-12 }
checks:
  - { from: probe, http: http://web:80/, expect: 200 }
  - { from: probe, script: check.sh }
";

    #[test]
    fn check_runners_are_written_and_the_module_says_where_they_run() {
        let spec = crate::parse(WITH_CHECKS).unwrap();
        let files = generate(&spec).unwrap();
        let tf = &files.iter().find(|f| f.path.ends_with("main.tf")).unwrap().contents;
        assert!(files.iter().any(|f| f.path == ".isoloom/proxmox/checks/probe.sh"));
        assert!(tf.contains("output \"machines\""), "{tf}");
        assert!(tf.contains("\"probe\" = \"10.9.0.11\""), "{tf}");
        assert!(tf.contains("position = \"probe\", machine = \"probe\", host = \"10.9.0.11\""), "{tf}");
        // `probe` runs a script, so it gets the project although it has no provisioning; `web` doesn't.
        assert_eq!(tf.matches("write_files = local.project_files").count(), 1, "{tf}");
    }

    #[test]
    fn every_linux_os_has_a_cloud_image_but_kali_fedora_and_trusty() {
        for os in KNOWN_OS.iter().filter(|o| !is_windows(o) && !["kali", "fedora-42", "ubuntu-14.04"].contains(o)) {
            assert!(image_url(os).is_some(), "no Proxmox cloud image for `{os}`");
        }
    }
}
