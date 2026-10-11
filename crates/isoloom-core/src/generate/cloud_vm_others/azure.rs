//! The `cloud-vm` Azure driver: a faithful port of the AWS driver in `cloud_vm` to the azurerm
//! provider. One resource group and virtual network hold every network as a subnet with its
//! exact range; each machine is a VM at its address with a public IP, a network security group
//! doing what the router does elsewhere, and the same SSH (or WinRM) set-up. Outputs match AWS
//! in name and shape (the module interface in docs/cloud-modules.md), so they read the same way.

use std::fmt::Write;

use super::super::cloud_vm::{CONTROLLER_OS, aligned, cidr, has_windows, hcl_cmd, inventory, linux_setup_cmds, needs_controller, redirects, sh_quote, tf_expr};
use super::super::proxmox::{hcl, res};
use super::super::{GeneratedFile, OUTPUT_DIR, address, address_for, header, start_order};
use crate::model::Spec;

/// Azure matches AWS's model (static private IPs, multi-NIC machines, Windows, a controller),
/// so nothing is refused here. The global `cloud_vm::unsupported` gate still applies.
pub(super) fn refusal(_spec: &Spec) -> Option<String> {
    None
}

/// The Azure Marketplace image for an OS name: (publisher, offer, sku). Azure sets the admin
/// user itself, so there is no per-image login (we always use `isoloom`).
fn azure_image(os: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match os {
        "debian-12" => ("Debian", "debian-12", "12-gen2"),
        "debian-13" => ("Debian", "debian-13", "13-gen2"),
        "ubuntu-22.04" => ("Canonical", "0001-com-ubuntu-server-jammy", "22_04-lts-gen2"),
        "ubuntu-24.04" => ("Canonical", "ubuntu-24_04-lts", "server-gen1"),
        "windows-server-2016" => ("MicrosoftWindowsServer", "WindowsServer", "2016-datacenter-gensecond"),
        "windows-server-2019" => ("MicrosoftWindowsServer", "WindowsServer", "2019-datacenter-gensecond"),
        "windows-server-2022" => ("MicrosoftWindowsServer", "WindowsServer", "2022-datacenter-azure-edition"),
        "windows-server-2025" => ("MicrosoftWindowsServer", "WindowsServer", "2025-datacenter-azure-edition"),
        _ => return None,
    })
}

/// The VM size for a machine's memory: B-series burstable, which covers the lab sizes.
fn azure_size(memory_mb: u32) -> &'static str {
    match memory_mb {
        0..=1024 => "Standard_B1s",
        1025..=2048 => "Standard_B1ms",
        2049..=4096 => "Standard_B2s",
        4097..=8192 => "Standard_B2ms",
        _ => "Standard_B4ms",
    }
}

/// Azure sets the admin user when it creates the VM, so we pick one it never reserves.
const USER: &str = "isoloom";

pub(super) fn build(spec: &Spec) -> GeneratedFile {
    let nets: Vec<&String> = spec.networks.keys().collect();
    let lab = format!(
        "{{ {} }}",
        nets.iter().map(|n| spec.networks[n.as_str()].cidr.clone()).collect::<Vec<_>>().join(", ")
    );

    let mut tf = header("#");
    tf.push_str(
        "# Start:  terraform -chdir=.isoloom/cloud-vm/azure init && terraform -chdir=.isoloom/cloud-vm/azure apply \\\n#           -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-vm/azure destroy (same variables)\n\n",
    );
    let mut providers = String::new();
    if needs_controller(spec) {
        providers.push_str("\n    tls = {\n      source  = \"hashicorp/tls\"\n      version = \"~> 4.0\"\n    }");
    }
    if has_windows(spec) {
        providers.push_str("\n    random = {\n      source  = \"hashicorp/random\"\n      version = \"~> 3.0\"\n    }");
    }
    let _ = write!(
        tf,
        r#"terraform {{
  required_version = ">= 1.6"
  backend "local" {{}}
  required_providers {{
    azurerm = {{
      source  = "hashicorp/azurerm"
      version = "~> 5.0"
    }}{providers}
  }}
}}

variable "subscription_id" {{
  type        = string
  default     = null
  description = "Default: ARM_SUBSCRIPTION_ID from the environment"
}}
variable "region" {{
  type    = string
  default = "swedencentral"
}}
variable "allowed_cidr" {{
  type        = string
  description = "Who may reach the machines (SSH and the published ports), e.g. your IP/32"
}}
variable "ssh_public_key" {{
  type = string
}}
variable "ssh_private_key_file" {{
  type        = string
  description = "The private key of ssh_public_key: Terraform sets the machines up over SSH"
}}
variable "auto_stop_minutes" {{
  type        = number
  default     = 0
  description = "Shut the machines down after this many minutes (0: never)"
}}

variable "expires_at" {{
  type        = string
  default     = ""
  description = "When the environment should end, in Unix seconds (empty: no end), as a tag on every resource so a reaper can find what to destroy"
}}
"#
    );
    if !spec.inputs.is_empty() {
        tf.push_str(
            "variable \"inputs\" {\n  type        = map(string)\n  default     = {}\n  sensitive   = true\n  description = \"Values given at launch\"\n}\n",
        );
    }
    let _ = write!(
        tf,
        r#"
provider "azurerm" {{
  features {{}}
  subscription_id = var.subscription_id
}}

resource "terraform_data" "id" {{
  input = substr(replace(uuid(), "-", ""), 0, 8)
  lifecycle {{
    ignore_changes = [input]
  }}
}}

locals {{
  name = "isoloom-{env}-${{terraform_data.id.output}}"
  root = abspath("${{path.module}}/../../..")
  tags = {{ "isoloom-environment" = "{env}", "managed-by" = "isoloom", "isoloom-instance" = local.name, "isoloom-expires-at" = var.expires_at }}
}}

resource "azurerm_resource_group" "env" {{
  name     = local.name
  location = var.region
  tags     = local.tags
}}

# The environment's networks: one virtual network, a subnet per network with its exact range.
resource "azurerm_virtual_network" "env" {{
  name                = local.name
  resource_group_name = azurerm_resource_group.env.name
  location            = var.region
  address_space       = [{spaces}]
  tags                = local.tags
}}
"#,
        env = spec.name,
        spaces = nets
            .iter()
            .map(|n| format!("\"{}\"", spec.networks[n.as_str()].cidr))
            .collect::<Vec<_>>()
            .join(", "),
    );
    for net in &nets {
        let _ = writeln!(
            tf,
            "\nresource \"azurerm_subnet\" \"{id}\" {{\n  name                 = \"{net}\"\n  resource_group_name  = azurerm_resource_group.env.name\n  virtual_network_name = azurerm_virtual_network.env.name\n  address_prefixes     = [\"{c}\"]\n}}",
            id = res(net),
            c = spec.networks[net.as_str()].cidr,
        );
    }
    if has_windows(spec) {
        tf.push_str("\n# The Windows machines' administrator password (user isoloom), for WinRM.\nresource \"random_password\" \"windows\" {\n  length      = 24\n  special     = false\n  min_upper   = 2\n  min_lower   = 2\n  min_numeric = 2\n}\n");
    }
    if needs_controller(spec) {
        tf.push_str("\n# The controller's own key: it runs the playbooks over SSH on every machine.\nresource \"tls_private_key\" \"controller\" {\n  algorithm = \"ED25519\"\n}\n");
    }

    let mut access_ip: Option<String> = None;
    let mut public_ips = Vec::new();
    let mut ssh_users = Vec::new();
    let mut published_out = Vec::new();
    for name in start_order(spec) {
        let m = &spec.machines[name];
        let Some(vm) = &m.vm else { continue };
        let id = res(name);
        let pip = format!("azurerm_public_ip.{id}.ip_address");
        let mem = m.resources.and_then(|r| r.memory_mb).unwrap_or(crate::DEFAULT_MEMORY_MB);
        let disk = m.resources.and_then(|r| r.disk_gb).unwrap_or(crate::DEFAULT_DISK_GB);
        if crate::images::is_windows(&vm.os) {
            windows_machine(spec, name, &mut tf, mem, disk, &mut published_out);
            public_ips.push((name.to_string(), pip.clone()));
            ssh_users.push((name.to_string(), format!("\"{USER}\"")));
            continue;
        }

        // Who may reach it: its own networks, what `reach` opens, SSH and the published ports
        // from allowed_cidr. Azure numbers the rules; names are unique within the group.
        let mut sg = format!(
            "\n# Machine `{name}`: what may reach it.\nresource \"azurerm_network_security_group\" \"{id}\" {{\n  name                = \"${{local.name}}-{name}\"\n  resource_group_name = azurerm_resource_group.env.name\n  location            = var.region\n  tags                = local.tags\n"
        );
        let mut prio = 100;
        for n in m.networks.keys() {
            inbound(
                &mut sg,
                &mut prio,
                &format!("net-{n}"),
                "*",
                "*",
                &format!("\"{}\"", spec.networks[n.as_str()].cidr),
            );
        }
        inbound(&mut sg, &mut prio, "ssh", "Tcp", "22", "var.allowed_cidr");
        for (ri, r) in spec
            .reach
            .iter()
            .filter(|r| m.networks.contains_key(&r.to) && !m.networks.contains_key(&r.from))
            .enumerate()
        {
            let from = format!("\"{}\"", spec.networks[&r.from].cidr);
            if r.ports.is_empty() {
                inbound(&mut sg, &mut prio, &format!("reach-{ri}-{}", r.from), "*", "*", &from);
            } else {
                for p in &r.ports {
                    inbound(&mut sg, &mut prio, &format!("reach-{ri}-{}-{p}-tcp", r.from), "Tcp", &p.to_string(), &from);
                    inbound(&mut sg, &mut prio, &format!("reach-{ri}-{}-{p}-udp", r.from), "Udp", &p.to_string(), &from);
                }
            }
        }
        let redirects = redirects(m);
        for sv in &m.services {
            if let Some(h) = sv.publish {
                inbound(&mut sg, &mut prio, &format!("published-{h}"), "Tcp", &h.to_string(), "var.allowed_cidr");
                let label = sv.name.clone().unwrap_or_else(|| sv.port.to_string());
                published_out.push((format!("\"{name}/{label}\""), format!("\"${{{pip}}}:{h}\"")));
            }
        }
        outbound_allow(&mut sg, 100);
        sg.push_str("}\n");
        tf.push_str(&sg);

        public_ip(&mut tf, &id, name);
        let multi = m.networks.len() > 1;
        // A NIC per network at its address; the primary (the first) carries the public IP.
        // Multi-homed machines forward packets that are not addressed to their primary IP.
        for (i, (n, o)) in m.networks.iter().enumerate() {
            nic(&mut tf, &id, name, n, address(spec, n, *o), i == 0, multi);
        }

        let nic_ids = m
            .networks
            .keys()
            .map(|n| format!("azurerm_network_interface.{id}_{}.id", res(n)))
            .collect::<Vec<_>>()
            .join(", ");
        let (publisher, offer, sku) = azure_image(&vm.os).expect("checked");
        let _ = writeln!(
            tf,
            "\nresource \"azurerm_linux_virtual_machine\" \"{id}\" {{\n  name                  = \"${{local.name}}-{name}\"\n  resource_group_name   = azurerm_resource_group.env.name\n  location              = var.region\n  size                  = \"{size}\"\n  admin_username        = \"{USER}\"\n  network_interface_ids = [{nic_ids}]\n  custom_data           = var.auto_stop_minutes > 0 ? base64encode(\"#!/bin/sh\\nshutdown -h +${{var.auto_stop_minutes}}\\n\") : null\n  admin_ssh_key {{\n    username   = \"{USER}\"\n    public_key = var.ssh_public_key\n  }}\n  os_disk {{\n    caching              = \"ReadWrite\"\n    storage_account_type = \"StandardSSD_LRS\"\n    disk_size_gb         = {disk}\n  }}\n  source_image_reference {{\n    publisher = \"{publisher}\"\n    offer     = \"{offer}\"\n    sku       = \"{sku}\"\n    version   = \"latest\"\n  }}\n  tags = local.tags\n}}",
            size = azure_size(mem),
            disk = disk.max(30),
        );

        // Its set-up, over SSH: the MAC of each extra interface comes from its Azure NIC
        // (uppercase, dash-separated), lowered and colon-joined to match `ip link`.
        let cmds = linux_setup_cmds(spec, name, m, vm, &lab, &redirects, &|nid| {
            tf_expr(&format!("lower(replace(azurerm_network_interface.{id}_{nid}.mac_address, \"-\", \":\"))"))
        });
        tf.push_str(&provision(spec, &id, &pip, USER, m, &cmds));

        public_ips.push((name.to_string(), pip.clone()));
        ssh_users.push((name.to_string(), format!("\"{USER}\"")));
        if m.access || access_ip.is_none() {
            access_ip = Some(pip.clone());
        }
    }

    if needs_controller(spec) {
        controller(spec, &mut tf);
    }

    // Outputs: every machine's address, and one to start from (the access machine, else the
    // first), with its SSH user and ready marker. Where a user stands (checks, an SSH
    // session): the access machine, else the first Linux machine, else the controller.
    let (first, check_user) = match access_ip {
        Some(ip) => (ip, None),
        None if needs_controller(spec) => ("azurerm_public_ip.isoloom_controller.ip_address".to_string(), Some(USER)),
        None => ("null".to_string(), None),
    };
    let _ = write!(
        tf,
        "\noutput \"machines\" {{\n  value = {{\n{}\n  }}\n}}\n\noutput \"ssh_users\" {{\n  value = {{\n{}\n  }}\n}}\n\noutput \"ip\" {{\n  value = {first}\n}}\n\noutput \"ready_file\" {{\n  value = \"/var/lib/isoloom/ready\"\n}}\n",
        aligned(&public_ips),
        aligned(&ssh_users),
    );
    let fallback = needs_controller(spec).then_some(("azurerm_public_ip.isoloom_controller.ip_address", USER));
    let _ = check_user;
    tf.push_str(&super::super::cloud_vm::checks_output(spec, &public_ips, &ssh_users, fallback));
    if !published_out.is_empty() {
        let _ = write!(tf, "\noutput \"published\" {{\n  value = {{\n{}\n  }}\n}}\n", aligned(&published_out));
    }

    GeneratedFile {
        path: format!("{OUTPUT_DIR}/cloud-vm/azure/main.tf"),
        contents: tf,
    }
}

/// One inbound allow rule in a network security group.
fn inbound(sg: &mut String, prio: &mut u32, name: &str, proto: &str, port: &str, src: &str) {
    let _ = write!(
        sg,
        "  security_rule {{\n    name                       = \"{name}\"\n    priority                   = {prio}\n    direction                  = \"Inbound\"\n    access                     = \"Allow\"\n    protocol                   = \"{proto}\"\n    source_port_range          = \"*\"\n    destination_port_range     = \"{port}\"\n    source_address_prefix      = {src}\n    destination_address_prefix = \"*\"\n  }}\n"
    );
    *prio += 10;
}

/// The default outbound allow (Azure allows outbound by default; made explicit, as AWS does).
fn outbound_allow(sg: &mut String, prio: u32) {
    let _ = write!(
        sg,
        "  security_rule {{\n    name                       = \"outbound\"\n    priority                   = {prio}\n    direction                  = \"Outbound\"\n    access                     = \"Allow\"\n    protocol                   = \"*\"\n    source_port_range          = \"*\"\n    destination_port_range     = \"*\"\n    source_address_prefix      = \"*\"\n    destination_address_prefix = \"*\"\n  }}\n"
    );
}

/// A machine's public IP (static, so the address `ip` gives holds across a stop).
fn public_ip(tf: &mut String, id: &str, name: &str) {
    let _ = writeln!(
        tf,
        "\nresource \"azurerm_public_ip\" \"{id}\" {{\n  name                = \"${{local.name}}-{name}\"\n  resource_group_name = azurerm_resource_group.env.name\n  location            = var.region\n  allocation_method   = \"Static\"\n  sku                 = \"Standard\"\n  tags                = local.tags\n}}"
    );
}

/// A network interface on `net` at `addr`, with its security group; the primary one carries the
/// public IP. Forwarding is on for multi-homed machines (the AWS source/destination check off).
fn nic(tf: &mut String, id: &str, name: &str, net: &str, addr: std::net::Ipv4Addr, primary: bool, forwarding: bool) {
    let nid = res(net);
    let public = if primary {
        "\n    public_ip_address_id          = azurerm_public_ip.{id}.id".replace("{id}", id)
    } else {
        String::new()
    };
    let fwd = if forwarding { "\n  ip_forwarding_enabled = true" } else { "" };
    let _ = writeln!(
        tf,
        "\nresource \"azurerm_network_interface\" \"{id}_{nid}\" {{\n  name                = \"${{local.name}}-{name}-{net}\"\n  resource_group_name = azurerm_resource_group.env.name\n  location            = var.region\n  tags                = local.tags{fwd}\n  ip_configuration {{\n    name                          = \"primary\"\n    subnet_id                     = azurerm_subnet.{nid}.id\n    private_ip_address_allocation = \"Static\"\n    private_ip_address            = \"{addr}\"{public}\n  }}\n}}\n\nresource \"azurerm_network_interface_security_group_association\" \"{id}_{nid}\" {{\n  network_interface_id      = azurerm_network_interface.{id}_{nid}.id\n  network_security_group_id = azurerm_network_security_group.{id}.id\n}}"
    );
}

/// A machine's set-up over SSH: the project copied to /opt/isoloom, its inputs, then its steps.
/// A faithful port of the AWS provisioning block, swapping the resource refs.
fn provision(spec: &Spec, id: &str, pip: &str, user: &str, m: &crate::model::Machine, cmds: &[String]) -> String {
    let deps: Vec<String> = m
        .depends_on
        .iter()
        .filter(|d| spec.machines[*d].vm.is_some())
        .map(|d| format!("terraform_data.{}", res(d)))
        .collect();
    let mut prov = format!(
        "\nresource \"terraform_data\" \"{id}\" {{\n  triggers_replace = [azurerm_linux_virtual_machine.{id}.id]\n  connection {{\n    type        = \"ssh\"\n    host        = {pip}\n    user        = \"{user}\"\n    private_key = file(pathexpand(var.ssh_private_key_file))\n    timeout     = \"10m\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\"cloud-init status --wait >/dev/null 2>&1 || true\", \"sudo mkdir -p /opt/isoloom && sudo chown {user} /opt/isoloom\"]\n  }}\n  provisioner \"local-exec\" {{\n    command = \"tar -czf \\\"${{path.module}}/.isoloom-project-{id}.tgz\\\" --exclude=.git --exclude=.vagrant --exclude=.terraform --exclude=.isoloom-project*.tgz -C \\\"${{local.root}}\\\" .\"\n  }}\n  provisioner \"file\" {{\n    source      = \"${{path.module}}/.isoloom-project-{id}.tgz\"\n    destination = \"/tmp/isoloom-project.tgz\"\n  }}\n"
    );
    if !m.inputs.is_empty() {
        let lines: Vec<String> = m
            .inputs
            .iter()
            .map(|i| format!("\"{i}=${{jsonencode(lookup(var.inputs, \"{i}\", \"\"))}}\""))
            .collect();
        let _ = write!(
            prov,
            "  provisioner \"file\" {{\n    content     = join(\"\\n\", [{}])\n    destination = \"/tmp/isoloom-inputs.env\"\n  }}\n",
            lines.join(", ")
        );
    }
    let _ = write!(
        prov,
        "  provisioner \"remote-exec\" {{\n    inline = [\n{}\n    ]\n  }}\n",
        cmds.iter().map(|c| format!("      {}", hcl_cmd(c))).collect::<Vec<_>>().join(",\n")
    );
    if !deps.is_empty() {
        let _ = writeln!(prov, "  depends_on = [{}]", deps.join(", "));
    }
    prov.push_str("}\n");
    prov
}

/// The controller: a Debian VM with an interface on every network at the controller address,
/// which runs the environment's playbooks once every machine is set up. A port of AWS's.
fn controller(spec: &Spec, tf: &mut String) {
    let nets: Vec<&String> = spec.networks.keys().collect();
    let mut sg = String::from(
        "\n# The controller (Ansible): every network may reach it, and SSH from allowed_cidr.\nresource \"azurerm_network_security_group\" \"isoloom_controller\" {\n  name                = \"${local.name}-controller\"\n  resource_group_name = azurerm_resource_group.env.name\n  location            = var.region\n  tags                = local.tags\n",
    );
    let mut prio = 100;
    for n in &nets {
        inbound(
            &mut sg,
            &mut prio,
            &format!("net-{n}"),
            "*",
            "*",
            &format!("\"{}\"", spec.networks[n.as_str()].cidr),
        );
    }
    inbound(&mut sg, &mut prio, "ssh", "Tcp", "22", "var.allowed_cidr");
    outbound_allow(&mut sg, 100);
    sg.push_str("}\n");
    tf.push_str(&sg);

    public_ip(tf, "isoloom_controller", "controller");
    let multi = nets.len() > 1;
    for (i, n) in nets.iter().enumerate() {
        let _ = writeln!(
            tf,
            "\nresource \"azurerm_network_interface\" \"isoloom_controller_{nid}\" {{\n  name                = \"${{local.name}}-controller-{n}\"\n  resource_group_name = azurerm_resource_group.env.name\n  location            = var.region\n  tags                = local.tags{fwd}\n  ip_configuration {{\n    name                          = \"primary\"\n    subnet_id                     = azurerm_subnet.{nid}.id\n    private_ip_address_allocation = \"Static\"\n    private_ip_address            = \"{a}\"{public}\n  }}\n}}\n\nresource \"azurerm_network_interface_security_group_association\" \"isoloom_controller_{nid}\" {{\n  network_interface_id      = azurerm_network_interface.isoloom_controller_{nid}.id\n  network_security_group_id = azurerm_network_security_group.isoloom_controller.id\n}}",
            nid = res(n),
            a = cidr(spec, n).controller(),
            fwd = if multi { "\n  ip_forwarding_enabled = true" } else { "" },
            public = if i == 0 {
                "\n    public_ip_address_id          = azurerm_public_ip.isoloom_controller.id"
            } else {
                ""
            },
        );
    }
    let nic_ids = nets
        .iter()
        .map(|n| format!("azurerm_network_interface.isoloom_controller_{}.id", res(n)))
        .collect::<Vec<_>>()
        .join(", ");
    let (publisher, offer, sku) = azure_image(CONTROLLER_OS).expect("controller image");
    let _ = writeln!(
        tf,
        "\nresource \"azurerm_linux_virtual_machine\" \"isoloom_controller\" {{\n  name                  = \"${{local.name}}-controller\"\n  resource_group_name   = azurerm_resource_group.env.name\n  location              = var.region\n  size                  = \"Standard_B1ms\"\n  admin_username        = \"{USER}\"\n  network_interface_ids = [{nic_ids}]\n  custom_data           = var.auto_stop_minutes > 0 ? base64encode(\"#!/bin/sh\\nshutdown -h +${{var.auto_stop_minutes}}\\n\") : null\n  admin_ssh_key {{\n    username   = \"{USER}\"\n    public_key = var.ssh_public_key\n  }}\n  os_disk {{\n    caching              = \"ReadWrite\"\n    storage_account_type = \"StandardSSD_LRS\"\n    disk_size_gb         = 30\n  }}\n  source_image_reference {{\n    publisher = \"{publisher}\"\n    offer     = \"{offer}\"\n    sku       = \"{sku}\"\n    version   = \"latest\"\n  }}\n  tags = local.tags\n}}"
    );

    // Its set-up: interfaces, names, the project, its key, Ansible, the inventory, the playbooks.
    let mut cmds: Vec<String> = vec![
        "set -e".into(),
        "cloud-init status --wait >/dev/null 2>&1 || true".into(),
        "tar -xzf /tmp/isoloom-project.tgz -C /opt/isoloom && rm -f /tmp/isoloom-project.tgz".into(),
    ];
    for n in nets.iter().skip(1) {
        let c = cidr(spec, n);
        cmds.push(format!(
            "IF=$(ip -o link | grep -i \"{mac}\" | awk -F': ' '{{print $2}}'); sudo ip link set \"$IF\" up && (ip -4 addr show \"$IF\" | grep -q {a}/ || sudo ip addr add {a}/{len} dev \"$IF\")",
            mac = tf_expr(&format!(
                "lower(replace(azurerm_network_interface.isoloom_controller_{}.mac_address, \"-\", \":\"))",
                res(n)
            )),
            a = c.controller(),
            len = c.len,
        ));
    }
    let hosts: Vec<String> = spec
        .machines
        .iter()
        .filter_map(|(o, om)| om.networks.first().map(|(n, oc)| format!("'{} {o}'", address(spec, n, *oc))))
        .collect();
    if !hosts.is_empty() {
        cmds.push(format!("printf '%s\\n' {} | sudo tee -a /etc/hosts >/dev/null", hosts.join(" ")));
    }
    cmds.push(
        "sudo mkdir -p /etc/isoloom && sudo install -m 0600 /tmp/isoloom-controller-key /etc/isoloom/id_ed25519 && rm -f /tmp/isoloom-controller-key".into(),
    );
    cmds.push("sudo DEBIAN_FRONTEND=noninteractive apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq python3-venv curl netcat-openbsd >/dev/null".into());
    cmds.push(
        "[ -x /opt/ansible/bin/ansible-playbook ] || { sudo python3 -m venv /opt/ansible && sudo /opt/ansible/bin/pip install -q 'ansible-core>=2.15,<2.17' pywinrm; }"
            .into(),
    );
    cmds.push(format!(
        "printf '%s' {} | sudo tee /etc/isoloom/inventory.ini >/dev/null",
        sh_quote(&inventory(spec))
    ));
    cmds.push(format!("sudo sh -c {}", sh_quote(&super::super::vagrant::ansible_runs(spec))));
    cmds.push("sudo mkdir -p /var/lib/isoloom && echo ready | sudo tee /var/lib/isoloom/ready >/dev/null".into());
    let deps: Vec<String> = spec
        .machines
        .iter()
        .filter(|(_, m)| m.vm.is_some())
        .map(|(n, _)| format!("terraform_data.{}", res(n)))
        .collect();
    let _ = writeln!(
        tf,
        "\nresource \"terraform_data\" \"isoloom_controller\" {{\n  triggers_replace = [azurerm_linux_virtual_machine.isoloom_controller.id]\n  connection {{\n    type        = \"ssh\"\n    host        = azurerm_public_ip.isoloom_controller.ip_address\n    user        = \"{USER}\"\n    private_key = file(pathexpand(var.ssh_private_key_file))\n    timeout     = \"10m\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\"cloud-init status --wait >/dev/null 2>&1 || true\", \"sudo mkdir -p /opt/isoloom && sudo chown {USER} /opt/isoloom\"]\n  }}\n  provisioner \"local-exec\" {{\n    command = \"tar -czf \\\"${{path.module}}/.isoloom-project-controller.tgz\\\" --exclude=.git --exclude=.vagrant --exclude=.terraform --exclude=.isoloom-project*.tgz -C \\\"${{local.root}}\\\" .\"\n  }}\n  provisioner \"file\" {{\n    source      = \"${{path.module}}/.isoloom-project-controller.tgz\"\n    destination = \"/tmp/isoloom-project.tgz\"\n  }}\n  provisioner \"file\" {{\n    content     = tls_private_key.controller.private_key_openssh\n    destination = \"/tmp/isoloom-controller-key\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\n{}\n    ]\n  }}\n  depends_on = [{}]\n}}",
        cmds.iter().map(|c| format!("      {}", hcl_cmd(c))).collect::<Vec<_>>().join(",\n"),
        deps.join(", "),
    );
}

/// A Windows machine: Azure's image with the `isoloom` administrator and a generated password;
/// its computer name set at creation (so no rename), WinRM over HTTP from `allowed_cidr`, then
/// over WinRM its `.ps1` steps, the other machines' names, firewall openings, published-port
/// redirects and the ready marker. A port of AWS's `windows_machine`.
fn windows_machine(spec: &Spec, name: &str, tf: &mut String, mem: u32, disk: u32, published_out: &mut Vec<(String, String)>) {
    let m = &spec.machines[name];
    let vm = m.vm.as_ref().expect("a VM machine");
    let id = res(name);
    let (net, octet) = m.networks.first().expect("validated: every machine is on a network");
    let addr = address(spec, net, *octet);
    // Windows wants 2 GB and more.
    let mem = mem.max(2048);
    // Its Windows name (NetBIOS: 15 characters).
    let host: String = name.chars().take(15).collect();

    let mut sg = format!(
        "\n# Machine `{name}` (Windows): what may reach it.\nresource \"azurerm_network_security_group\" \"{id}\" {{\n  name                = \"${{local.name}}-{name}\"\n  resource_group_name = azurerm_resource_group.env.name\n  location            = var.region\n  tags                = local.tags\n"
    );
    let mut prio = 100;
    inbound(
        &mut sg,
        &mut prio,
        &format!("net-{net}"),
        "*",
        "*",
        &format!("\"{}\"", spec.networks[net.as_str()].cidr),
    );
    inbound(&mut sg, &mut prio, "winrm", "Tcp", "5985", "var.allowed_cidr");
    for (ri, r) in spec.reach.iter().filter(|r| &r.to == net).enumerate() {
        let from = format!("\"{}\"", spec.networks[&r.from].cidr);
        if r.ports.is_empty() {
            inbound(&mut sg, &mut prio, &format!("reach-{ri}-{}", r.from), "*", "*", &from);
        } else {
            for p in &r.ports {
                inbound(&mut sg, &mut prio, &format!("reach-{ri}-{}-{p}", r.from), "Tcp", &p.to_string(), &from);
            }
        }
    }
    let mut ps: Vec<String> = vec![
        "$ErrorActionPreference = 'Stop'".into(),
        format!(
            "if ($env:COMPUTERNAME -ne '{}') {{ throw \"still named $env:COMPUTERNAME: the rename hasn't taken effect\" }}",
            host.to_uppercase()
        ),
    ];
    for sv in &m.services {
        if let Some(h) = sv.publish {
            inbound(&mut sg, &mut prio, &format!("published-{h}"), "Tcp", &h.to_string(), "var.allowed_cidr");
            let label = sv.name.clone().unwrap_or_else(|| sv.port.to_string());
            published_out.push((format!("\"{name}/{label}\""), format!("\"${{azurerm_public_ip.{id}.ip_address}}:{h}\"")));
            if h != sv.port {
                ps.push(format!(
                    "netsh interface portproxy add v4tov4 listenport={h} listenaddress=0.0.0.0 connectport={p} connectaddress=127.0.0.1 | Out-Null",
                    p = sv.port
                ));
            }
        }
    }
    outbound_allow(&mut sg, 100);
    sg.push_str("}\n");
    tf.push_str(&sg);

    public_ip(tf, &id, name);
    // One network (the global gate refuses Windows on several), so the NIC carries the public IP.
    let _ = writeln!(
        tf,
        "\nresource \"azurerm_network_interface\" \"{id}_{nid}\" {{\n  name                = \"${{local.name}}-{name}-{net}\"\n  resource_group_name = azurerm_resource_group.env.name\n  location            = var.region\n  tags                = local.tags\n  ip_configuration {{\n    name                          = \"primary\"\n    subnet_id                     = azurerm_subnet.{nid}.id\n    private_ip_address_allocation = \"Static\"\n    private_ip_address            = \"{addr}\"\n    public_ip_address_id          = azurerm_public_ip.{id}.id\n  }}\n}}\n\nresource \"azurerm_network_interface_security_group_association\" \"{id}_{nid}\" {{\n  network_interface_id      = azurerm_network_interface.{id}_{nid}.id\n  network_security_group_id = azurerm_network_security_group.{id}.id\n}}",
        nid = res(net),
    );

    let (publisher, offer, sku) = azure_image(&vm.os).expect("checked");
    let _ = writeln!(
        tf,
        "\nresource \"azurerm_windows_virtual_machine\" \"{id}\" {{\n  name                  = \"${{local.name}}-{name}\"\n  computer_name         = \"{host}\"\n  resource_group_name   = azurerm_resource_group.env.name\n  location              = var.region\n  size                  = \"{size}\"\n  admin_username        = \"{USER}\"\n  admin_password        = random_password.windows.result\n  network_interface_ids = [azurerm_network_interface.{id}_{nid}.id]\n  winrm_listener {{\n    protocol = \"Http\"\n  }}\n  os_disk {{\n    caching              = \"ReadWrite\"\n    storage_account_type = \"StandardSSD_LRS\"\n    disk_size_gb         = {disk}\n  }}\n  source_image_reference {{\n    publisher = \"{publisher}\"\n    offer     = \"{offer}\"\n    sku       = \"{sku}\"\n    version   = \"latest\"\n  }}\n  tags = local.tags\n}}",
        size = azure_size(mem),
        nid = res(net),
        disk = disk.max(127),
    );

    // Over WinRM: names, firewall openings for its services, steps, the ready marker.
    let hosts: Vec<String> = spec
        .machines
        .keys()
        .filter(|o| o.as_str() != name)
        .map(|o| format!("{} {o}", address_for(spec, name, o)))
        .collect();
    if !hosts.is_empty() {
        ps.push(format!(
            "Add-Content -Path \"$env:windir\\System32\\drivers\\etc\\hosts\" -Value @({})",
            hosts.iter().map(|h| format!("'{h}'")).collect::<Vec<_>>().join(", ")
        ));
    }
    for sv in &m.services {
        ps.push(format!(
            "New-NetFirewallRule -DisplayName 'isoloom {p}' -Direction Inbound -Protocol TCP -LocalPort {p} -Action Allow | Out-Null",
            p = sv.port
        ));
        if let Some(h) = sv.publish.filter(|h| *h != sv.port) {
            ps.push(format!(
                "New-NetFirewallRule -DisplayName 'isoloom {h}' -Direction Inbound -Protocol TCP -LocalPort {h} -Action Allow | Out-Null"
            ));
        }
    }
    ps.push("New-Item -ItemType Directory -Force C:\\ProgramData\\isoloom | Out-Null; Set-Content C:\\ProgramData\\isoloom\\ready 'ready'".into());

    let conn = format!(
        "  connection {{\n    type     = \"winrm\"\n    host     = azurerm_public_ip.{id}.ip_address\n    user     = \"{USER}\"\n    password = random_password.windows.result\n    https    = false\n    use_ntlm = true\n    timeout  = \"30m\"\n  }}\n"
    );
    let mut prov = format!("\nresource \"terraform_data\" \"{id}\" {{\n  triggers_replace = [azurerm_windows_virtual_machine.{id}.id]\n{conn}");
    // Auto-stop as a scheduled task (Azure runs no first-boot user data on Windows).
    let _ = write!(
        prov,
        "  provisioner \"remote-exec\" {{\n    inline = [var.auto_stop_minutes > 0 ? \"powershell -NoProfile -Command \\\"Register-ScheduledTask -TaskName isoloom-auto-stop -Action (New-ScheduledTaskAction -Execute shutdown.exe -Argument '/s /t 0') -Trigger (New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(${{var.auto_stop_minutes}})) -User SYSTEM -RunLevel Highest -Force\\\"\" : \"cmd /c ver\"]\n  }}\n"
    );
    for step in &vm.provision {
        let _ = write!(
            prov,
            "  provisioner \"file\" {{\n    source      = \"${{local.root}}/{step}\"\n    destination = \"C:/isoloom/{step}\"\n  }}\n"
        );
    }
    for step in &vm.provision {
        ps.push(format!(
            "& powershell -NoProfile -ExecutionPolicy Bypass -File 'C:\\isoloom\\{}'; if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}",
            step.replace('/', "\\")
        ));
    }
    let _ = write!(
        prov,
        "  provisioner \"file\" {{\n    content     = {}\n    destination = \"C:/isoloom/setup.ps1\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\"powershell -NoProfile -ExecutionPolicy Bypass -File C:/isoloom/setup.ps1\"]\n  }}\n",
        hcl(&(ps.join("\n") + "\n"))
    );
    let deps: Vec<String> = m
        .depends_on
        .iter()
        .filter(|d| spec.machines[*d].vm.is_some())
        .map(|d| format!("terraform_data.{}", res(d)))
        .collect();
    if !deps.is_empty() {
        let _ = writeln!(prov, "  depends_on = [{}]", deps.join(", "));
    }
    prov.push_str("}\n");
    tf.push_str(&prov);
}
