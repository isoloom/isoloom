//! The `cloud-vm` target: one cloud VM per machine, in `.isoloom/cloud-vm/<cloud>/main.tf`
//! (AWS for now).
//!
//! - One VPC holding every network as a subnet with its exact range, so every address of the
//!   spec is kept. The VPC routes between subnets; security groups do what the router does on
//!   other targets: a network's machines reach each other, `reach` rules open one network to
//!   another (on their ports), the rest is closed.
//! - Each machine is an instance at its address, set up over SSH from where Terraform runs: the
//!   project copied to /opt/isoloom, other machines' names, volumes, waits for `depends_on`,
//!   then its steps. Offline machines stop reaching outside the environment once provisioned,
//!   as on local VMs.
//! - Published services: open to `allowed_cidr` on the instance's public address.
//! - Checks: run on demand over SSH from the access machine (else the first), where a user
//!   stands: the `checks` output gives the machine and the commands.
//! - A machine on several networks gets a network interface on each (source/destination
//!   check off, so replies leave from the right one) and an Elastic IP on the first.
//! - Not yet: Windows, environment-level `provision:`.

use std::fmt::Write;

use super::proxmox::{hcl, res};
use super::{GenerateError, GeneratedFile, OUTPUT_DIR, address, header, router, start_order};
use crate::model::{Spec, Target};
use crate::validate::Cidr;

const DIR: &str = "cloud-vm";

fn cidr(spec: &Spec, net: &str) -> Cidr {
    Cidr::parse(&spec.networks[net].cidr).expect("validated cidr")
}

/// The AMI lookup and SSH user for an OS name.
fn image(os: &str) -> Option<(&'static str, &'static str, &'static str)> {
    // (owner, name filter, ssh user)
    Some(match os {
        "debian-12" => ("136693071363", "debian-12-amd64-*", "admin"),
        "debian-13" => ("136693071363", "debian-13-amd64-*", "admin"),
        "ubuntu-22.04" => ("099720109477", "ubuntu/images/hvm-ssd*/ubuntu-jammy-22.04-amd64-server-*", "ubuntu"),
        "ubuntu-24.04" => ("099720109477", "ubuntu/images/hvm-ssd*/ubuntu-noble-24.04-amd64-server-*", "ubuntu"),
        _ => return None,
    })
}

/// The instance type for a machine's memory: Free Tier eligible up to 8 GB (accounts on the
/// AWS Free plan can't launch other types), then general purpose.
pub(super) fn instance_type(memory_mb: u32) -> &'static str {
    match memory_mb {
        0..=1024 => "t3.micro",
        1025..=2048 => "t3.small",
        2049..=4096 => "c7i-flex.large",
        4097..=8192 => "m7i-flex.large",
        _ => "m7i-flex.xlarge",
    }
}

/// The private-range family of a network (AWS can't mix them in one VPC).
fn family(c: Cidr) -> u8 {
    (c.base >> 24) as u8
}

fn unsupported(spec: &Spec) -> Option<String> {
    if !spec.provision.is_empty() {
        return Some("environment-level provisioning (`provision:`) in the cloud comes later".into());
    }
    if spec.networks.values().any(|n| n.gateway.is_some()) {
        return Some("networks with a `gateway` machine in the cloud come later".into());
    }
    if spec.checks.iter().any(|c| c.ends_with(".yml") || c.ends_with(".yaml")) {
        return Some("Ansible checks (.yml) in the cloud come later".into());
    }
    let nets: Vec<Cidr> = spec.networks.keys().map(|n| cidr(spec, n)).collect();
    if nets.iter().any(|c| c.len > 28) {
        return Some("AWS subnets are /28 or larger".into());
    }
    if nets.iter().any(|c| family(*c) != family(nets[0])) {
        return Some("AWS can't mix private ranges (10/8, 172.16/12, 192.168/16) in one VPC".into());
    }
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        if image(&vm.os).is_none() {
            return Some(format!("machine `{name}`: no AWS image for `{}` yet (Debian 12/13, Ubuntu 22.04/24.04)", vm.os));
        }
        if let Some((net, _)) = m.networks.iter().find(|(_, o)| **o <= 3) {
            return Some(format!("machine `{name}`: AWS keeps the first addresses of a subnet (.1 to .3) on `{net}`"));
        }
    }
    None
}

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    if let Some(what) = unsupported(spec) {
        return Err(GenerateError::Unsupported { target: Target::CloudVm, what });
    }
    let nets: Vec<&String> = spec.networks.keys().collect();
    let lab = format!(
        "{{ {} }}",
        nets.iter().map(|n| spec.networks[n.as_str()].cidr.clone()).collect::<Vec<_>>().join(", ")
    );

    let mut tf = header("#");
    tf.push_str(
        "# Start:  terraform -chdir=.isoloom/cloud-vm/aws init && terraform -chdir=.isoloom/cloud-vm/aws apply \\\n#           -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-vm/aws destroy (same variables)\n\n",
    );
    let _ = write!(
        tf,
        r#"terraform {{
  required_version = ">= 1.6"
  required_providers {{
    aws = {{
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }}
  }}
}}

variable "region" {{
  type    = string
  default = "eu-west-3"
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
provider "aws" {{
  region = var.region
  default_tags {{
    tags = {{ "isoloom-environment" = "{env}", "managed-by" = "isoloom" }}
  }}
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
}}

data "aws_availability_zones" "available" {{
  state = "available"
}}

# The environment's networks: one VPC, a subnet per network with its exact range.
resource "aws_vpc" "env" {{
  cidr_block = "{first}"
  tags       = {{ Name = local.name }}
}}
"#,
        env = spec.name,
        first = spec.networks[nets[0].as_str()].cidr,
    );
    for net in nets.iter().skip(1) {
        let _ = writeln!(
            tf,
            "\nresource \"aws_vpc_ipv4_cidr_block_association\" \"{id}\" {{\n  vpc_id     = aws_vpc.env.id\n  cidr_block = \"{c}\"\n}}",
            id = res(net),
            c = spec.networks[net.as_str()].cidr
        );
    }
    tf.push_str("\nresource \"aws_internet_gateway\" \"env\" {\n  vpc_id = aws_vpc.env.id\n}\n\nresource \"aws_route_table\" \"env\" {\n  vpc_id = aws_vpc.env.id\n  route {\n    cidr_block = \"0.0.0.0/0\"\n    gateway_id = aws_internet_gateway.env.id\n  }\n}\n");
    for net in &nets {
        let id = res(net);
        let dep = if *net == nets[0] {
            String::new()
        } else {
            format!("\n  depends_on        = [aws_vpc_ipv4_cidr_block_association.{id}]")
        };
        let _ = writeln!(
            tf,
            "\nresource \"aws_subnet\" \"{id}\" {{\n  vpc_id            = aws_vpc.env.id\n  cidr_block        = \"{c}\"\n  availability_zone = data.aws_availability_zones.available.names[0]\n  tags              = {{ Name = \"${{local.name}}-{net}\" }}{dep}\n}}\n\nresource \"aws_route_table_association\" \"{id}\" {{\n  subnet_id      = aws_subnet.{id}.id\n  route_table_id = aws_route_table.env.id\n}}",
            c = spec.networks[net.as_str()].cidr
        );
    }
    tf.push_str("\nresource \"aws_key_pair\" \"env\" {\n  key_name   = local.name\n  public_key = var.ssh_public_key\n}\n");

    // AMIs, once per OS.
    let mut oses: Vec<&str> = spec.machines.values().filter_map(|m| m.vm.as_ref()).map(|v| v.os.as_str()).collect();
    oses.sort();
    oses.dedup();
    for os in &oses {
        let (owner, filter, _) = image(os).expect("checked");
        let _ = writeln!(
            tf,
            "\ndata \"aws_ami\" \"{id}\" {{\n  most_recent = true\n  owners      = [\"{owner}\"]\n  filter {{\n    name   = \"name\"\n    values = [\"{filter}\"]\n  }}\n  filter {{\n    name   = \"architecture\"\n    values = [\"x86_64\"]\n  }}\n}}",
            id = res(os)
        );
    }

    let mut access_ip: Option<String> = None;
    let mut public_ips = Vec::new();
    let mut ssh_users = Vec::new();
    let mut published_out = Vec::new();
    for name in start_order(spec) {
        let m = &spec.machines[name];
        let Some(vm) = &m.vm else { continue };
        let (_, _, user) = image(&vm.os).expect("checked");
        let (net, octet) = m.networks.first().expect("validated: every machine is on a network");
        let id = res(name);
        let addr = address(spec, net, *octet);
        let multi = m.networks.len() > 1;
        // Its public address: auto-assigned with one interface; an Elastic IP with several (AWS
        // doesn't auto-assign one then).
        let pip = if multi {
            format!("aws_eip.{id}.public_ip")
        } else {
            format!("aws_instance.{id}.public_ip")
        };
        let mem = m.resources.and_then(|r| r.memory_mb).unwrap_or(crate::DEFAULT_MEMORY_MB);
        let disk = m.resources.and_then(|r| r.disk_gb).unwrap_or(crate::DEFAULT_DISK_GB);

        // Who may reach it: its own network, the networks `reach` opens to it, and allowed_cidr
        // for SSH and the published ports.
        let mut sg = format!(
            "\n# Machine `{name}`: what may reach it.\nresource \"aws_security_group\" \"{id}\" {{\n  name   = \"${{local.name}}-{name}\"\n  vpc_id = aws_vpc.env.id\n"
        );
        for n in m.networks.keys() {
            let _ = write!(
                sg,
                "  ingress {{\n    description = \"its network ({n})\"\n    from_port   = 0\n    to_port     = 0\n    protocol    = \"-1\"\n    cidr_blocks = [\"{c}\"]\n  }}\n",
                c = spec.networks[n.as_str()].cidr
            );
        }
        sg.push_str("  ingress {\n    description = \"SSH from allowed_cidr\"\n    from_port   = 22\n    to_port     = 22\n    protocol    = \"tcp\"\n    cidr_blocks = [var.allowed_cidr]\n  }\n");
        for r in spec
            .reach
            .iter()
            .filter(|r| m.networks.contains_key(&r.to) && !m.networks.contains_key(&r.from))
        {
            let from = &spec.networks[&r.from].cidr;
            if r.ports.is_empty() {
                let _ = write!(
                    sg,
                    "  ingress {{\n    description = \"reach from {f}\"\n    from_port   = 0\n    to_port     = 0\n    protocol    = \"-1\"\n    cidr_blocks = [\"{from}\"]\n  }}\n",
                    f = r.from
                );
            } else {
                for p in &r.ports {
                    for proto in ["tcp", "udp"] {
                        let _ = write!(
                            sg,
                            "  ingress {{\n    description = \"reach from {f}\"\n    from_port   = {p}\n    to_port     = {p}\n    protocol    = \"{proto}\"\n    cidr_blocks = [\"{from}\"]\n  }}\n",
                            f = r.from
                        );
                    }
                }
            }
        }
        let mut redirects = Vec::new();
        for sv in &m.services {
            if let Some(h) = sv.publish {
                let _ = write!(
                    sg,
                    "  ingress {{\n    description = \"published\"\n    from_port   = {h}\n    to_port     = {h}\n    protocol    = \"tcp\"\n    cidr_blocks = [var.allowed_cidr]\n  }}\n"
                );
                if h != sv.port {
                    redirects.push((h, sv.port));
                }
                let label = sv.name.clone().unwrap_or_else(|| sv.port.to_string());
                published_out.push((format!("\"{name}/{label}\""), format!("\"${{{pip}}}:{h}\"")));
            }
        }
        sg.push_str("  egress {\n    from_port   = 0\n    to_port     = 0\n    protocol    = \"-1\"\n    cidr_blocks = [\"0.0.0.0/0\"]\n  }\n}\n");
        tf.push_str(&sg);

        if multi {
            // An interface per network at its address; source/destination check off, so a reply
            // can leave from the interface of the network it's for.
            for (n, o) in &m.networks {
                let _ = writeln!(
                    tf,
                    "\nresource \"aws_network_interface\" \"{id}_{nid}\" {{\n  subnet_id         = aws_subnet.{nid}.id\n  private_ips       = [\"{a}\"]\n  security_groups   = [aws_security_group.{id}.id]\n  source_dest_check = false\n  tags              = {{ Name = \"${{local.name}}-{name}-{n}\" }}\n}}",
                    nid = res(n),
                    a = address(spec, n, *o),
                );
            }
            let _ = writeln!(
                tf,
                "\nresource \"aws_instance\" \"{id}\" {{\n  ami           = data.aws_ami.{ami}.id\n  instance_type = \"{itype}\"\n  key_name      = aws_key_pair.env.key_name\n  user_data     = var.auto_stop_minutes > 0 ? \"#!/bin/sh\\nshutdown -h +${{var.auto_stop_minutes}}\\n\" : null\n  primary_network_interface {{\n    network_interface_id = aws_network_interface.{id}_{netid}.id\n  }}\n  root_block_device {{\n    volume_size = {disk}\n  }}\n  tags = {{ Name = \"${{local.name}}-{name}\" }}\n}}",
                ami = res(&vm.os),
                itype = instance_type(mem),
                netid = res(net),
            );
            for (i, n) in m.networks.keys().enumerate().skip(1) {
                let _ = writeln!(
                    tf,
                    "\nresource \"aws_network_interface_attachment\" \"{id}_{nid}\" {{\n  instance_id          = aws_instance.{id}.id\n  network_interface_id = aws_network_interface.{id}_{nid}.id\n  device_index         = {i}\n}}",
                    nid = res(n),
                );
            }
            let _ = writeln!(
                tf,
                "\nresource \"aws_eip\" \"{id}\" {{\n  network_interface = aws_network_interface.{id}_{netid}.id\n  depends_on        = [aws_internet_gateway.env]\n}}",
                netid = res(net),
            );
        } else {
            let _ = writeln!(
                tf,
                "\nresource \"aws_instance\" \"{id}\" {{\n  ami                         = data.aws_ami.{ami}.id\n  instance_type               = \"{itype}\"\n  subnet_id                   = aws_subnet.{netid}.id\n  private_ip                  = \"{addr}\"\n  associate_public_ip_address = true\n  key_name                    = aws_key_pair.env.key_name\n  vpc_security_group_ids      = [aws_security_group.{id}.id]\n  user_data                   = var.auto_stop_minutes > 0 ? \"#!/bin/sh\\nshutdown -h +${{var.auto_stop_minutes}}\\n\" : null\n  root_block_device {{\n    volume_size = {disk}\n  }}\n  tags = {{ Name = \"${{local.name}}-{name}\" }}\n}}",
                ami = res(&vm.os),
                itype = instance_type(mem),
                netid = res(net),
            );
        }

        // Its set-up, over SSH: names, volumes, waits, the project, its steps.
        let mut cmds: Vec<String> = vec![
            "set -e".into(),
            "cloud-init status --wait >/dev/null 2>&1 || true".into(),
            "tar -xzf /tmp/isoloom-project.tgz -C /opt/isoloom && rm -f /tmp/isoloom-project.tgz".into(),
        ];
        // Its other interfaces, found by MAC address, at their addresses.
        for (n, o) in m.networks.iter().skip(1) {
            let c = cidr(spec, n);
            cmds.push(format!(
                "IF=$(ip -o link | grep -i \"{mac}\" | awk -F': ' '{{print $2}}'); sudo ip link set \"$IF\" up && (ip -4 addr show \"$IF\" | grep -q {a}/ || sudo ip addr add {a}/{len} dev \"$IF\")",
                mac = tf_expr(&format!("lower(aws_network_interface.{id}_{}.mac_address)", res(n))),
                a = address(spec, n, *o),
                len = c.len,
            ));
        }
        // The others by name, at their address on a network both are on (else their first).
        let hosts: Vec<String> = spec
            .machines
            .keys()
            .filter(|o| o.as_str() != name)
            .map(|o| format!("{} {o}", super::address_for(spec, name, o)))
            .collect();
        if !hosts.is_empty() {
            cmds.push(format!(
                "printf '%s\\n' {} | sudo tee -a /etc/hosts >/dev/null",
                hosts.iter().map(|h| format!("'{h}'")).collect::<Vec<_>>().join(" ")
            ));
        }
        if !m.volumes.is_empty() {
            cmds.push(format!("sudo mkdir -p {}", m.volumes.values().cloned().collect::<Vec<_>>().join(" ")));
        }
        for dep in &m.depends_on {
            let ports: Vec<u16> = spec.machines[dep].services.iter().map(|s| s.port).collect();
            if !ports.is_empty() {
                cmds.push(format!("sh -c {}", sh_quote(&router::wait_for(dep, &ports, 900))));
            }
        }
        let env = if m.inputs.is_empty() {
            ""
        } else {
            "set -a; . /tmp/isoloom-inputs.env; set +a; "
        };
        for step in &vm.provision {
            if step.ends_with(".sh") {
                cmds.push(format!("cd /opt/isoloom && sudo -E sh -c {}", sh_quote(&format!("{env}sh {step}"))));
            } else {
                cmds.push("command -v ansible-playbook >/dev/null || (sudo apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq ansible-core)".into());
                cmds.push(format!(
                    "cd /opt/isoloom && sudo -E sh -c {}",
                    sh_quote(&format!("{env}ansible-playbook -c local -i localhost, {step}"))
                ));
            }
        }
        if !redirects.is_empty() || !m.networks.keys().any(|n| spec.networks[n].internet) {
            cmds.push("command -v nft >/dev/null || (sudo apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq nftables)".into());
        }
        for (h, p) in &redirects {
            cmds.push(format!(
                "sudo nft add table ip isoloom-publish && sudo nft 'add chain ip isoloom-publish prerouting {{ type nat hook prerouting priority -100; }}' && sudo nft add rule ip isoloom-publish prerouting tcp dport {h} redirect to :{p}"
            ));
        }
        // Offline: no new connections leaving the environment, once provisioned.
        if !m.networks.keys().any(|n| spec.networks[n].internet) {
            cmds.push(format!(
                "printf '%s\\n' 'table inet isoloom-egress {{' '  chain output {{' '    type filter hook output priority 0; policy accept;' '    ip daddr != {lab} ct state new drop' '  }}' '}}' | sudo tee /etc/isoloom-egress.nft >/dev/null && sudo nft -f /etc/isoloom-egress.nft"
            ));
        }
        cmds.push("sudo mkdir -p /var/lib/isoloom && echo ready | sudo tee /var/lib/isoloom/ready >/dev/null".into());

        let deps: Vec<String> = m
            .depends_on
            .iter()
            .filter(|d| spec.machines[*d].vm.is_some())
            .map(|d| format!("terraform_data.{}", res(d)))
            .collect();
        let mut prov = format!(
            "\nresource \"terraform_data\" \"{id}\" {{\n  triggers_replace = [aws_instance.{id}.id]\n  connection {{\n    type        = \"ssh\"\n    host        = {pip}\n    user        = \"{user}\"\n    private_key = file(pathexpand(var.ssh_private_key_file))\n    timeout     = \"10m\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\"cloud-init status --wait >/dev/null 2>&1 || true\", \"sudo mkdir -p /opt/isoloom && sudo chown {user} /opt/isoloom\"]\n  }}\n  provisioner \"local-exec\" {{\n    command = \"tar -czf \\\"${{path.module}}/.isoloom-project-{id}.tgz\\\" --exclude=.git --exclude=.vagrant --exclude=.terraform --exclude=.isoloom-project*.tgz -C \\\"${{local.root}}\\\" .\"\n  }}\n  provisioner \"file\" {{\n    source      = \"${{path.module}}/.isoloom-project-{id}.tgz\"\n    destination = \"/tmp/isoloom-project.tgz\"\n  }}\n"
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
        tf.push_str(&prov);

        public_ips.push((name.to_string(), pip.clone()));
        ssh_users.push((name.to_string(), format!("\"{user}\"")));
        if m.access || access_ip.is_none() {
            access_ip = Some(pip.clone());
        }
    }

    // Outputs: every machine's address, and one to start from (the access machine, else the
    // first), with its SSH user and ready marker, as the other cloud outputs give.
    let first = access_ip.unwrap_or_else(|| "null".into());
    let _ = write!(
        tf,
        "\noutput \"machines\" {{\n  value = {{\n{}\n  }}\n}}\n\noutput \"ssh_users\" {{\n  value = {{\n{}\n  }}\n}}\n\noutput \"ip\" {{\n  value = {first}\n}}\n\noutput \"ready_file\" {{\n  value = \"/var/lib/isoloom/ready\"\n}}\n",
        aligned(&public_ips),
        aligned(&ssh_users),
    );
    if !spec.checks.is_empty() {
        let runs: Vec<String> = spec.checks.iter().map(|c| format!("      \"cd /opt/isoloom && sh {c}\"")).collect();
        let _ = write!(
            tf,
            "\n# The checks, from where a user stands: ssh <ssh_user>@<ip> each command.\noutput \"checks\" {{\n  value = {{\n    host = {first}\n    commands = [\n{}\n    ]\n  }}\n}}\n",
            runs.join(",\n")
        );
    }
    if !published_out.is_empty() {
        let _ = write!(tf, "\noutput \"published\" {{\n  value = {{\n{}\n  }}\n}}\n", aligned(&published_out));
    }

    Ok(vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/aws/main.tf"),
        contents: tf,
    }])
}

/// HCL map entries with their `=` aligned (as `terraform fmt` writes them).
fn aligned(entries: &[(String, String)]) -> String {
    let w = entries.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    entries.iter().map(|(k, v)| format!("    {k:<w$} = {v}")).collect::<Vec<_>>().join("\n")
}

/// A Terraform expression inside a set-up command: kept through `hcl`'s escaping (which turns
/// `${` into a literal) and turned into `${expr}` by `hcl_cmd`.
fn tf_expr(expr: &str) -> String {
    format!("\u{1}{expr}\u{2}")
}

/// A set-up command as an HCL string, its `tf` expressions interpolated.
fn hcl_cmd(c: &str) -> String {
    hcl(c).replace('\u{1}', "${").replace('\u{2}', "}")
}

/// A single-quoted shell word.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
