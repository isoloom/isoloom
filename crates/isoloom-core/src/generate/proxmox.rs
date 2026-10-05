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

use std::fmt::Write;

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, address, address_for, header, router, start_order};
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
fn hcl(s: &str) -> String {
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
    if !spec.provision.is_empty() {
        return Some("environment-level provisioning (`provision:`) on Proxmox comes later".into());
    }
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
        if m.services.iter().any(|s| s.publish.is_some()) {
            return Some(format!(
                "machine `{name}`: published ports on Proxmox (a port forward on the router) come later"
            ));
        }
        if vm.image.is_some() {
            return Some(format!("machine `{name}`: `vm.image` has no Proxmox entry yet"));
        }
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
  required_providers {
    proxmox = {
      source  = "bpg/proxmox"
      version = "~> 0.115"
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
        tf.push_str("variable \"inputs\" {\n  type      = map(string)\n  default   = {}\n  sensitive = true\n  description = \"Values given at launch\"\n}\n");
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
  users = var.ssh_public_key == "" ? [] : [{
    name                = "isoloom"
    sudo                = "ALL=(ALL) NOPASSWD:ALL"
    shell               = "/bin/bash"
    ssh_authorized_keys = [var.ssh_public_key]
  }]
}

# The environment's networks: an SDN simple zone, a VNet per network.
resource "proxmox_sdn_zone_simple" "env" {
  id    = local.zone
  nodes = [var.node]
}
"#,
    );
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
    // The internet, for every network (machines install their software); offline machines block
    // it themselves once provisioned, as on local VMs.
    let _ = writeln!(
        nft,
        "    ip saddr {lab} ip daddr != {lab} accept\n  }}\n  chain postrouting {{\n    type nat hook postrouting priority 100;\n    ip saddr {lab} ip daddr != {lab} masquerade\n  }}\n}}"
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
        "\n# The router: forwards between networks with the reach rules, and to the internet.\nresource \"proxmox_virtual_environment_file\" \"router\" {{\n  node_name    = var.node\n  datastore_id = var.snippets_datastore\n  content_type = \"snippets\"\n  source_raw {{\n    file_name = \"iso${{var.slot}}-router.yaml\"\n    data = \"#cloud-config\\n${{yamlencode({{\n      hostname = \"isoloom-router\"\n      users    = local.users\n      packages = [\"nftables\"]\n      write_files = [\n        {}\n      ]\n      runcmd = [\n        [\"sysctl\", \"-p\", \"/etc/sysctl.d/90-isoloom.conf\"],\n        [\"systemctl\", \"enable\", \"systemd-networkd\"],\n        [\"systemctl\", \"restart\", \"systemd-networkd\"],\n        [\"systemctl\", \"enable\", \"--now\", \"nftables\"],\n        [\"nft\", \"-f\", \"/etc/nftables.conf\"],\n      ]\n    }})}}\"\n  }}\n}}",
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
        "\nresource \"proxmox_virtual_environment_vm\" \"isoloom_router\" {{\n  name      = \"iso${{var.slot}}-router\"\n  node_name = var.node\n  tags      = [\"isoloom\", \"{env}\"]\n  on_boot   = false\n  cpu {{\n    cores = 1\n    type  = \"host\"\n  }}\n  memory {{\n    dedicated = 512\n  }}\n  disk {{\n    datastore_id = var.datastore\n    file_id      = proxmox_download_file.{img}.id\n    interface    = \"virtio0\"\n    size         = 8\n  }}\n{router_nets}  initialization {{\n    datastore_id      = var.datastore\n    user_data_file_id = proxmox_virtual_environment_file.router.id\n    ip_config {{\n      ipv4 {{\n        address = \"dhcp\"\n      }}\n    }}\n  }}\n  operating_system {{\n    type = \"l26\"\n  }}\n  serial_device {{}}\n  depends_on = [proxmox_sdn_applier.env]\n}}",
        env = spec.name,
        img = res("debian-12"),
    );

    // The machines, in start order.
    for name in start_order(spec) {
        let m = &spec.machines[name];
        let Some(vm) = &m.vm else { continue };
        let cpus = m.resources.and_then(|r| r.cpus).unwrap_or(crate::DEFAULT_CPUS);
        let mem = m.resources.and_then(|r| r.memory_mb).unwrap_or(crate::DEFAULT_MEMORY_MB);
        let disk = m.resources.and_then(|r| r.disk_gb).unwrap_or(crate::DEFAULT_DISK_GB);

        let mut runcmd: Vec<String> = Vec::new();
        let hosts: Vec<String> = spec
            .machines
            .keys()
            .filter(|o| o.as_str() != name)
            .map(|o| format!("{} {o}", address_for(spec, name, o)))
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
            let ports: Vec<u16> = spec.machines[dep].services.iter().map(|s| s.port).collect();
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
        if !vm.provision.is_empty() {
            files.push("local.project_files".to_string());
        }
        if !m.inputs.is_empty() {
            let lines: Vec<String> = m
                .inputs
                .iter()
                .map(|i| format!("\"{i}=${{lookup(var.inputs, \\\"{i}\\\", \\\"\\\")}}\""))
                .collect();
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
        let _ = writeln!(
            tf,
            "\nresource \"proxmox_virtual_environment_vm\" \"{id}\" {{\n  name      = \"iso${{var.slot}}-{name}\"\n  node_name = var.node\n  tags      = [\"isoloom\", \"{env}\"]\n  on_boot   = false\n  cpu {{\n    cores = {cpus}\n    type  = \"host\"\n  }}\n  memory {{\n    dedicated = {mem}\n  }}\n  disk {{\n    datastore_id = var.datastore\n    file_id      = proxmox_download_file.{img}.id\n    interface    = \"virtio0\"\n    size         = {disk}\n  }}\n{nics}  initialization {{\n    datastore_id      = var.datastore\n    user_data_file_id = proxmox_virtual_environment_file.{id}.id\n    dns {{\n      servers = [\"1.1.1.1\"]\n    }}\n{ipcfg}  }}\n  operating_system {{\n    type = \"l26\"\n  }}\n  serial_device {{}}\n  depends_on = [proxmox_virtual_environment_vm.isoloom_router{deps}]\n}}",
            env = spec.name,
            img = res(&vm.os),
            deps = m
                .depends_on
                .iter()
                .map(|d| format!(", proxmox_virtual_environment_vm.{}", res(d)))
                .collect::<String>(),
        );
    }

    Ok(vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/main.tf"),
        contents: tf,
    }])
}

/// A Terraform resource name from a machine, network or OS name.
fn res(s: &str) -> String {
    s.replace(['-', '.'], "_")
}

#[cfg(test)]
mod tests {
    use super::image_url;
    use crate::images::is_windows;
    use crate::model::KNOWN_OS;

    #[test]
    fn every_linux_os_has_a_cloud_image_but_kali_and_fedora() {
        for os in KNOWN_OS.iter().filter(|o| !is_windows(o) && !["kali", "fedora-42"].contains(o)) {
            assert!(image_url(os).is_some(), "no Proxmox cloud image for `{os}`");
        }
    }
}
