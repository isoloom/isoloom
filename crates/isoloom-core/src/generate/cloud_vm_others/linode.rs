//! Linode (Akamai) driver for the `cloud-vm` target: one Linode per machine on a single VPC
//! subnet, each with a pinned private IPv4 (`ipv4.vpc`) so inter-machine addresses are exactly
//! the spec's. Best-effort: single-network, Linux-only labs; multi-network, Windows, and
//! controller-needing specs are refused.

use std::fmt::Write;

use crate::generate::cloud_vm::{aligned, linux_setup_cmds, needs_controller, redirects, remote_exec};
use crate::generate::proxmox::res;
use crate::generate::{GeneratedFile, OUTPUT_DIR, address, header, start_order};
use crate::model::Spec;

/// The Linode image and SSH user for an OS name (Linode logs in as root).
fn linode_image(os: &str) -> Option<(&'static str, &'static str)> {
    Some(match os {
        "debian-12" => ("linode/debian12", "root"),
        "debian-13" => ("linode/debian13", "root"),
        "ubuntu-22.04" => ("linode/ubuntu22.04", "root"),
        "ubuntu-24.04" => ("linode/ubuntu24.04", "root"),
        _ => return None,
    })
}

/// The Linode plan for a machine's memory.
fn linode_type(memory_mb: u32) -> &'static str {
    match memory_mb {
        0..=1024 => "g6-nanode-1",
        1025..=2048 => "g6-standard-1",
        2049..=4096 => "g6-standard-2",
        4097..=8192 => "g6-standard-4",
        _ => "g6-standard-6",
    }
}

pub(super) fn refusal(spec: &Spec) -> Option<String> {
    if spec.networks.len() != 1 {
        return Some("Linode cloud-vm takes single-network labs so far".into());
    }
    if crate::generate::cloud_vm::has_windows(spec) {
        return Some("Linode cloud-vm is Linux only so far".into());
    }
    if needs_controller(spec) {
        return Some("Linode cloud-vm has no Ansible controller yet".into());
    }
    None
}

pub(super) fn build(spec: &Spec) -> GeneratedFile {
    // Single-network Linux (refusal() guaranteed it): the one network and its range.
    let net = spec.networks.keys().next().expect("one network");
    let lab = format!("{{ {} }}", spec.networks[net].cidr);

    let mut tf = header("#");
    tf.push_str(
        "# Auth:   export LINODE_TOKEN=<token> (the provider reads it).\n# Start:  terraform -chdir=.isoloom/cloud-vm/linode init && terraform -chdir=.isoloom/cloud-vm/linode apply \\\n#           -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-vm/linode destroy (same variables)\n\n",
    );
    let _ = write!(
        tf,
        r#"terraform {{
  required_version = ">= 1.6"
  backend "local" {{}}
  required_providers {{
    linode = {{
      source  = "linode/linode"
      version = "~> 2.0"
    }}
    random = {{
      source  = "hashicorp/random"
      version = "~> 3.0"
    }}
  }}
}}

variable "region" {{
  type    = string
  default = "fr-par"
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
"#,
    );
    if !spec.inputs.is_empty() {
        tf.push_str(
            "variable \"inputs\" {\n  type        = map(string)\n  default     = {}\n  sensitive   = true\n  description = \"Values given at launch\"\n}\n",
        );
    }
    let _ = write!(
        tf,
        r#"
provider "linode" {{}}

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

# The environment's one network: a VPC with a subnet at its exact range, so every machine keeps
# its address ({cidr}).
resource "linode_vpc" "env" {{
  label  = local.name
  region = var.region
}}

resource "linode_vpc_subnet" "env" {{
  vpc_id = linode_vpc.env.id
  label  = "{net}"
  ipv4   = "{cidr}"
}}
"#,
        env = spec.name,
        cidr = spec.networks[net].cidr,
    );

    // Who may reach the machines: SSH and the published ports from allowed_cidr, everything from
    // the VPC subnet, the rest dropped. One firewall for every instance.
    let machines: Vec<&str> = start_order(spec).into_iter().filter(|n| spec.machines[*n].vm.is_some()).collect();
    let published: Vec<u16> = {
        let mut v: Vec<u16> = spec.machines.values().flat_map(|m| m.services.iter().filter_map(|s| s.publish)).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let mut fw = String::from(
        "\nresource \"linode_firewall\" \"env\" {\n  label           = local.name\n  inbound_policy  = \"DROP\"\n  outbound_policy = \"ACCEPT\"\n  inbound {\n    label    = \"ssh\"\n    action   = \"ACCEPT\"\n    protocol = \"TCP\"\n    ports    = \"22\"\n    ipv4     = [var.allowed_cidr]\n  }\n",
    );
    if !published.is_empty() {
        let _ = write!(
            fw,
            "  inbound {{\n    label    = \"published\"\n    action   = \"ACCEPT\"\n    protocol = \"TCP\"\n    ports    = \"{}\"\n    ipv4     = [var.allowed_cidr]\n  }}\n",
            published.iter().map(u16::to_string).collect::<Vec<_>>().join(",")
        );
    }
    let _ = write!(
        fw,
        "  inbound {{\n    label    = \"vpc-tcp\"\n    action   = \"ACCEPT\"\n    protocol = \"TCP\"\n    ports    = \"1-65535\"\n    ipv4     = [\"{c}\"]\n  }}\n  inbound {{\n    label    = \"vpc-udp\"\n    action   = \"ACCEPT\"\n    protocol = \"UDP\"\n    ports    = \"1-65535\"\n    ipv4     = [\"{c}\"]\n  }}\n  inbound {{\n    label    = \"vpc-icmp\"\n    action   = \"ACCEPT\"\n    protocol = \"ICMP\"\n    ipv4     = [\"{c}\"]\n  }}\n",
        c = spec.networks[net].cidr,
    );
    let _ = write!(
        fw,
        "  linodes = [{}]\n}}\n",
        machines.iter().map(|n| format!("linode_instance.{}.id", res(n))).collect::<Vec<_>>().join(", ")
    );
    tf.push_str(&fw);

    let mut access_ip: Option<String> = None;
    let mut public_ips = Vec::new();
    let mut ssh_users = Vec::new();
    let mut published_out = Vec::new();
    for name in &machines {
        let name = *name;
        let m = &spec.machines[name];
        let vm = m.vm.as_ref().expect("filtered to VM machines");
        let (image, user) = linode_image(&vm.os).expect("single-network Linux spec");
        let (_, octet) = m.networks.first().expect("validated: every machine is on a network");
        let id = res(name);
        let addr = address(spec, net, *octet);
        let pip = format!("one(linode_instance.{id}.ipv4)");
        let mem = m.resources.and_then(|r| r.memory_mb).unwrap_or(crate::DEFAULT_MEMORY_MB);

        // Its root password (for the console; SSH logs in with the key).
        let _ = write!(
            tf,
            "\nresource \"random_password\" \"{id}\" {{\n  length      = 24\n  special     = false\n  min_upper   = 2\n  min_lower   = 2\n  min_numeric = 2\n}}\n"
        );
        // The instance: a public interface for its public IP, and a VPC interface pinned to its
        // address, so the machines reach each other at exactly the spec's addresses.
        let _ = write!(
            tf,
            "\nresource \"linode_instance\" \"{id}\" {{\n  label           = \"${{local.name}}-{name}\"\n  region          = var.region\n  type            = \"{itype}\"\n  image           = \"{image}\"\n  authorized_keys = [trimspace(var.ssh_public_key)]\n  root_pass       = random_password.{id}.result\n  booted          = true\n  tags            = concat([\"isoloom\", \"{env}\", local.name], var.expires_at == \"\" ? [] : [\"isoloom-expires-${{var.expires_at}}\"])\n  metadata {{\n    user_data = base64encode(var.auto_stop_minutes > 0 ? \"#!/bin/sh\\nshutdown -h +${{var.auto_stop_minutes}}\\n\" : \"\")\n  }}\n  interface {{\n    purpose = \"public\"\n  }}\n  interface {{\n    purpose   = \"vpc\"\n    subnet_id = linode_vpc_subnet.env.id\n    ipv4 {{\n      vpc = \"{addr}\"\n    }}\n  }}\n}}\n",
            itype = linode_type(mem),
            env = spec.name,
        );

        // Its set-up over SSH: single network, so no secondary NIC (mac_expr is never called).
        let redirects = redirects(m);
        let cmds = linux_setup_cmds(spec, name, m, vm, &lab, &redirects, &|_| String::new());

        let mut deps: Vec<String> = vec!["linode_firewall.env".to_string()];
        deps.extend(
            m.depends_on
                .iter()
                .filter(|d| spec.machines[*d].vm.is_some())
                .map(|d| format!("terraform_data.{}", res(d))),
        );
        let mut prov = format!(
            "\nresource \"terraform_data\" \"{id}\" {{\n  triggers_replace = [linode_instance.{id}.id]\n  connection {{\n    type        = \"ssh\"\n    host        = {pip}\n    user        = \"{user}\"\n    private_key = file(pathexpand(var.ssh_private_key_file))\n    timeout     = \"10m\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\"cloud-init status --wait >/dev/null 2>&1 || true\", \"command -v sudo >/dev/null || (apt-get update -qq && apt-get install -y -qq sudo)\", \"sudo mkdir -p /opt/isoloom && sudo chown {user} /opt/isoloom\"]\n  }}\n  provisioner \"local-exec\" {{\n    command = \"tar -czf \\\"${{path.module}}/.isoloom-project-{id}.tgz\\\" --exclude=.git --exclude=.vagrant --exclude=.terraform --exclude=.isoloom-project*.tgz -C \\\"${{local.root}}\\\" .\"\n  }}\n  provisioner \"file\" {{\n    source      = \"${{path.module}}/.isoloom-project-{id}.tgz\"\n    destination = \"/tmp/isoloom-project.tgz\"\n  }}\n"
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
        prov.push_str(&remote_exec(&cmds));
        let _ = writeln!(prov, "  depends_on = [{}]", deps.join(", "));
        prov.push_str("}\n");
        tf.push_str(&prov);

        for sv in &m.services {
            if let Some(h) = sv.publish {
                let label = sv.name.clone().unwrap_or_else(|| sv.port.to_string());
                published_out.push((format!("\"{name}/{label}\""), format!("\"${{{pip}}}:{h}\"")));
            }
        }
        public_ips.push((name.to_string(), pip.clone()));
        ssh_users.push((name.to_string(), format!("\"{user}\"")));
        if m.access || access_ip.is_none() {
            access_ip = Some(pip.clone());
        }
    }

    // Outputs: every machine's address, and one to start from (the access machine, else the
    // first), with its SSH user and ready marker, as the other cloud outputs give.
    let first = access_ip.clone().unwrap_or_else(|| "null".into());
    let check_user = if access_ip.is_some() { "\"root\"" } else { "null" };
    let _ = write!(
        tf,
        "\noutput \"machines\" {{\n  value = {{\n{}\n  }}\n}}\n\noutput \"ssh_users\" {{\n  value = {{\n{}\n  }}\n}}\n\noutput \"ip\" {{\n  value = {first}\n}}\n\noutput \"ready_file\" {{\n  value = \"/var/lib/isoloom/ready\"\n}}\n",
        aligned(&public_ips),
        aligned(&ssh_users),
    );
    let _ = check_user;
    tf.push_str(&super::super::cloud_vm::checks_output(spec, &public_ips, &ssh_users, None));
    if !published_out.is_empty() {
        let _ = write!(tf, "\noutput \"published\" {{\n  value = {{\n{}\n  }}\n}}\n", aligned(&published_out));
    }

    GeneratedFile {
        path: format!("{OUTPUT_DIR}/cloud-vm/linode/main.tf"),
        contents: tf,
    }
}
