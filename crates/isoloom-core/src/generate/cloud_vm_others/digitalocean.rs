//! DigitalOcean `cloud-vm`: one droplet for the lab's single Linux machine, on a VPC with the
//! network's range. DigitalOcean can't pin a droplet's private IPv4 to a chosen address, so the
//! lab's fixed inter-machine addressing doesn't hold; this driver therefore takes only
//! single-network, single-Linux-VM labs and refuses the rest. Best effort, by design.

use std::fmt::Write;

use crate::generate::cloud_vm::{aligned, has_windows, hcl_cmd, linux_setup_cmds, needs_controller, redirects};
use crate::generate::proxmox::res;
use crate::generate::{GeneratedFile, OUTPUT_DIR, header, start_order};
use crate::model::Spec;

/// The droplet image slug and SSH user for an OS name (DigitalOcean images log in as root).
fn do_image(os: &str) -> Option<(&'static str, &'static str)> {
    Some(match os {
        "debian-12" => ("debian-12-x64", "root"),
        "debian-13" => ("debian-13-x64", "root"),
        "ubuntu-22.04" => ("ubuntu-22-04-x64", "root"),
        "ubuntu-24.04" => ("ubuntu-24-04-x64", "root"),
        _ => return None,
    })
}

/// The droplet size for a machine's memory.
fn do_size(memory_mb: u32) -> &'static str {
    match memory_mb {
        0..=1024 => "s-1vcpu-1gb",
        1025..=2048 => "s-2vcpu-2gb",
        2049..=4096 => "s-2vcpu-4gb",
        _ => "s-4vcpu-8gb",
    }
}

/// Why DigitalOcean can't take this spec, if it can't. It keeps only single-network,
/// single-Linux-VM labs, because it can't pin private addresses.
pub(super) fn refusal(spec: &Spec) -> Option<String> {
    if spec.networks.len() != 1 {
        return Some("DigitalOcean cloud-vm takes single-network labs so far".into());
    }
    if has_windows(spec) {
        return Some("DigitalOcean cloud-vm is Linux only so far".into());
    }
    if needs_controller(spec) {
        return Some("DigitalOcean cloud-vm has no Ansible controller yet".into());
    }
    if spec.machines.values().filter(|m| m.vm.is_some()).count() > 1 {
        return Some("DigitalOcean cloud-vm takes single-machine labs so far (it can't pin private addresses)".into());
    }
    // An OS without a droplet image: refuse rather than build something that won't boot.
    for (name, m) in &spec.machines {
        if let Some(vm) = &m.vm {
            if do_image(&vm.os).is_none() {
                return Some(format!(
                    "machine `{name}`: no DigitalOcean image for `{}` yet (Debian 12/13, Ubuntu 22.04/24.04)",
                    vm.os
                ));
            }
        }
    }
    None
}

/// The droplet, firewall and set-up for the lab's single Linux machine. `refusal` has already
/// ruled out everything else, so a single-network single-VM lab is assumed.
pub(super) fn build(spec: &Spec) -> GeneratedFile {
    let net = spec.networks.keys().next().expect("validated: one network");
    let net_cidr = spec.networks[net].cidr.clone();
    let lab = format!("{{ {net_cidr} }}");

    let name = start_order(spec)
        .into_iter()
        .find(|n| spec.machines[*n].vm.is_some())
        .expect("validated: one VM machine");
    let m = &spec.machines[name];
    let vm = m.vm.as_ref().expect("checked");
    let (slug, user) = do_image(&vm.os).expect("checked by refusal");
    let id = res(name);
    let mem = m.resources.and_then(|r| r.memory_mb).unwrap_or(crate::DEFAULT_MEMORY_MB);
    // The droplet's public address.
    let pip = format!("digitalocean_droplet.{id}.ipv4_address");
    let redirects = redirects(m);

    let mut tf = header("#");
    tf.push_str(
        "# Auth:   export DIGITALOCEAN_TOKEN=<token>\n# Start:  terraform -chdir=.isoloom/cloud-vm/digitalocean init && terraform -chdir=.isoloom/cloud-vm/digitalocean apply \\\n#           -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-vm/digitalocean destroy (same variables)\n\n",
    );
    tf.push_str(
        r#"terraform {
  required_version = ">= 1.6"
  backend "local" {}
  required_providers {
    digitalocean = {
      source  = "digitalocean/digitalocean"
      version = "~> 2.0"
    }
  }
}

variable "region" {
  type    = string
  default = "fra1"
}
variable "allowed_cidr" {
  type        = string
  description = "Who may reach the machine (SSH and the published ports), e.g. your IP/32"
}
variable "ssh_public_key" {
  type = string
}
variable "ssh_private_key_file" {
  type        = string
  description = "The private key of ssh_public_key: Terraform sets the machine up over SSH"
}
variable "auto_stop_minutes" {
  type        = number
  default     = 0
  description = "Shut the machine down after this many minutes (0: never)"
}
"#,
    );
    if !spec.inputs.is_empty() {
        tf.push_str(
            "variable \"inputs\" {\n  type        = map(string)\n  default     = {}\n  sensitive   = true\n  description = \"Values given at launch\"\n}\n",
        );
    }
    let _ = write!(
        tf,
        r##"
provider "digitalocean" {{}}

resource "terraform_data" "id" {{
  input = substr(replace(uuid(), "-", ""), 0, 8)
  lifecycle {{
    ignore_changes = [input]
  }}
}}

locals {{
  name = "isoloom-{env}-${{terraform_data.id.output}}"
  root = abspath("${{path.module}}/../../..")
}}

# The environment's network: a VPC with the lab network's range. DigitalOcean assigns the
# droplet a private address in it (it can't be pinned), so this is best-effort addressing.
resource "digitalocean_vpc" "env" {{
  name     = local.name
  region   = var.region
  ip_range = "{net_cidr}"
}}

resource "digitalocean_ssh_key" "env" {{
  name       = local.name
  public_key = var.ssh_public_key
}}

resource "digitalocean_droplet" "{id}" {{
  name      = "${{local.name}}-{name}"
  region    = var.region
  size      = "{size}"
  image     = "{slug}"
  vpc_uuid  = digitalocean_vpc.env.id
  ssh_keys  = [digitalocean_ssh_key.env.fingerprint]
  user_data = var.auto_stop_minutes > 0 ? "#!/bin/sh\nshutdown -h +${{var.auto_stop_minutes}}\n" : null
  tags      = ["isoloom", "{env}"]
}}

# What may reach it: SSH and the published ports from allowed_cidr, everything from the VPC.
resource "digitalocean_firewall" "{id}" {{
  name        = local.name
  droplet_ids = [digitalocean_droplet.{id}.id]
  inbound_rule {{
    protocol         = "tcp"
    port_range       = "22"
    source_addresses = [var.allowed_cidr]
  }}
"##,
        env = spec.name,
        size = do_size(mem),
    );
    let mut published_out = Vec::new();
    for sv in &m.services {
        if let Some(h) = sv.publish {
            let _ = write!(
                tf,
                "  inbound_rule {{\n    protocol         = \"tcp\"\n    port_range       = \"{h}\"\n    source_addresses = [var.allowed_cidr]\n  }}\n"
            );
            let label = sv.name.clone().unwrap_or_else(|| sv.port.to_string());
            published_out.push((format!("\"{name}/{label}\""), format!("\"${{{pip}}}:{h}\"")));
        }
    }
    tf.push_str(
        "  inbound_rule {\n    protocol         = \"tcp\"\n    port_range       = \"1-65535\"\n    source_addresses = [digitalocean_vpc.env.ip_range]\n  }\n  inbound_rule {\n    protocol         = \"udp\"\n    port_range       = \"1-65535\"\n    source_addresses = [digitalocean_vpc.env.ip_range]\n  }\n  inbound_rule {\n    protocol         = \"icmp\"\n    source_addresses = [digitalocean_vpc.env.ip_range]\n  }\n  outbound_rule {\n    protocol              = \"tcp\"\n    port_range            = \"1-65535\"\n    destination_addresses = [\"0.0.0.0/0\", \"::/0\"]\n  }\n  outbound_rule {\n    protocol              = \"udp\"\n    port_range            = \"1-65535\"\n    destination_addresses = [\"0.0.0.0/0\", \"::/0\"]\n  }\n  outbound_rule {\n    protocol              = \"icmp\"\n    destination_addresses = [\"0.0.0.0/0\", \"::/0\"]\n  }\n}\n",
    );

    // Its set-up, over SSH. One network: the MAC closure is never called.
    let cmds = linux_setup_cmds(spec, name, m, vm, &lab, &redirects, &|_| String::new());
    let mut prov = format!(
        "\nresource \"terraform_data\" \"{id}\" {{\n  triggers_replace = [digitalocean_droplet.{id}.id]\n  connection {{\n    type        = \"ssh\"\n    host        = {pip}\n    user        = \"{user}\"\n    private_key = file(pathexpand(var.ssh_private_key_file))\n    timeout     = \"10m\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\"cloud-init status --wait >/dev/null 2>&1 || true\", \"sudo mkdir -p /opt/isoloom && sudo chown {user} /opt/isoloom\"]\n  }}\n  provisioner \"local-exec\" {{\n    command = \"tar -czf \\\"${{path.module}}/.isoloom-project-{id}.tgz\\\" --exclude=.git --exclude=.vagrant --exclude=.terraform --exclude=.isoloom-project*.tgz -C \\\"${{local.root}}\\\" .\"\n  }}\n  provisioner \"file\" {{\n    source      = \"${{path.module}}/.isoloom-project-{id}.tgz\"\n    destination = \"/tmp/isoloom-project.tgz\"\n  }}\n"
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
        "  provisioner \"remote-exec\" {{\n    inline = [\n{}\n    ]\n  }}\n}}\n",
        cmds.iter().map(|c| format!("      {}", hcl_cmd(c))).collect::<Vec<_>>().join(",\n")
    );
    tf.push_str(&prov);

    // Outputs: the same shape as the AWS driver.
    let machines = aligned(&[(name.to_string(), pip.clone())]);
    let ssh_users = aligned(&[(name.to_string(), format!("\"{user}\""))]);
    let _ = write!(
        tf,
        "\noutput \"machines\" {{\n  value = {{\n{machines}\n  }}\n}}\n\noutput \"ssh_users\" {{\n  value = {{\n{ssh_users}\n  }}\n}}\n\noutput \"ip\" {{\n  value = {pip}\n}}\n\noutput \"ready_file\" {{\n  value = \"/var/lib/isoloom/ready\"\n}}\n",
    );
    if !spec.checks.is_empty() {
        let runs: Vec<String> = spec.checks.iter().map(|c| format!("      \"cd /opt/isoloom && sh {c}\"")).collect();
        let _ = write!(
            tf,
            "\n# The checks, from where a user stands: ssh <user>@<host> each command.\noutput \"checks\" {{\n  value = {{\n    host = {pip}\n    user = \"{user}\"\n    commands = [\n{}\n    ]\n  }}\n}}\n",
            runs.join(",\n"),
        );
    }
    if !published_out.is_empty() {
        let _ = write!(tf, "\noutput \"published\" {{\n  value = {{\n{}\n  }}\n}}\n", aligned(&published_out));
    }

    GeneratedFile {
        path: format!("{OUTPUT_DIR}/cloud-vm/digitalocean/main.tf"),
        contents: tf,
    }
}
