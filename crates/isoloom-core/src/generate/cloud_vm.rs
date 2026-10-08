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
//! - Environment-level `provision:`: a Debian controller on every network (at the controller
//!   address) runs the playbooks over SSH with a key of its own, once every machine is set up.
//! - Windows (Amazon's Server images): an `isoloom` administrator with a generated password and
//!   WinRM at first boot; its `.ps1` steps uploaded and run over WinRM; Ansible reaches it
//!   from the controller, which also runs the checks when no Linux machine can.
//! - Not yet: Windows machines on several networks.

use std::fmt::Write;

use super::proxmox::{hcl, res};
use super::{GenerateError, GeneratedFile, OUTPUT_DIR, address, header, router, start_order};
use crate::model::{Spec, Target};
use crate::validate::Cidr;

const DIR: &str = "cloud-vm";

pub(super) fn cidr(spec: &Spec, net: &str) -> Cidr {
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
        // Amazon's Windows Server images (license included, billed per hour).
        "windows-server-2016" => ("801119661308", "Windows_Server-2016-English-Full-Base-*", "isoloom"),
        "windows-server-2019" => ("801119661308", "Windows_Server-2019-English-Full-Base-*", "isoloom"),
        "windows-server-2022" => ("801119661308", "Windows_Server-2022-English-Full-Base-*", "isoloom"),
        "windows-server-2025" => ("801119661308", "Windows_Server-2025-English-Full-Base-*", "isoloom"),
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
pub(super) fn family(c: Cidr) -> u8 {
    (c.base >> 24) as u8
}

fn unsupported(spec: &Spec) -> Option<String> {
    if let Some(name) = super::arm64_machine(spec) {
        return Some(format!("machine `{name}`: arm64 on AWS comes later (Graviton instances and arm64 AMIs)"));
    }
    if spec.networks.values().any(|n| n.gateway.is_some()) {
        return Some("networks with a `gateway` machine in the cloud come later".into());
    }
    if spec.checks.iter().any(|c| c.is_playbook()) {
        return Some("Ansible checks (.yml) in the cloud come later".into());
    }
    if spec.networks.values().any(|n| n.tc.is_some()) {
        return Some("link impairment (`tc`) needs a router in the path; the cloud's security groups have none".into());
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
            return Some(format!(
                "machine `{name}`: no AWS image for `{}` yet (Debian 12/13, Ubuntu 22.04/24.04, Windows Server 2016 to 2025)",
                vm.os
            ));
        }
        if crate::images::is_windows(&vm.os) && m.networks.len() > 1 {
            return Some(format!("machine `{name}`: Windows machines on several networks in the cloud come later"));
        }
        if crate::images::is_windows(&vm.os) && vm.provision.iter().any(|p| !p.ends_with(".ps1")) {
            return Some(format!("machine `{name}`: Windows steps are .ps1 scripts"));
        }
        if let Some((net, _)) = m.networks.iter().find(|(_, o)| **o <= 3) {
            return Some(format!("machine `{name}`: AWS keeps the first addresses of a subnet (.1 to .3) on `{net}`"));
        }
    }
    None
}

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    // Things no cloud can do yet fail the whole target; a single cloud's own limits only drop
    // that cloud's module (best-effort: a lab runs on the clouds whose model fits it).
    if let Some(what) = unsupported(spec) {
        return Err(GenerateError::Unsupported { target: Target::CloudVm, what });
    }
    let mut files = vec![aws(spec)];
    for (sub, builder, refusal) in super::cloud_vm_others::DRIVERS {
        if refusal(spec).is_none() {
            files.push(builder(spec));
        }
        let _ = sub;
    }
    files.extend(check_scripts(spec));
    Ok(files)
}

/// The check runners, one per position, shared by every cloud (addresses are kept as written
/// in the cloud): `.isoloom/cloud-vm/checks/<position>.sh`, run on the machine over SSH from
/// the project copy at /opt/isoloom.
fn check_scripts(spec: &Spec) -> Vec<GeneratedFile> {
    let plan = crate::checks::plan(spec);
    let host = |h: &crate::checks::Host, _: &crate::checks::Position| -> String {
        match h {
            crate::checks::Host::Literal(l) => l.clone(),
            crate::checks::Host::Machine { name, network } => address(spec, network, spec.machines[name].networks[network]).to_string(),
        }
    };
    let run_script = |path: &str| format!("cd /opt/isoloom && sh {path}");
    let render = crate::checks::Render {
        host: &host,
        script: &run_script,
        playbook: None,
    };
    crate::checks::by_position(spec, &plan)
        .into_iter()
        .map(|(pos, group)| GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/checks/{}.sh", pos.id()),
            contents: crate::checks::script(&pos, &group, &render),
        })
        .collect()
}

/// The `checks` output of a cloud module: each runner with the machine it runs on, that
/// machine's public address and SSH user (HCL expressions from the driver), and the command.
/// A position that isn't one of the cloud's machines (the environment's networks, a machine the
/// runner supplies) runs from `fallback`: the controller, else the first Linux machine.
pub(super) fn checks_output(spec: &Spec, public_ips: &[(String, String)], ssh_users: &[(String, String)], fallback: Option<(&str, &str)>) -> String {
    let plan = crate::checks::plan(spec);
    let mut entries = Vec::new();
    for (pos, _) in crate::checks::by_position(spec, &plan) {
        let on = match &pos {
            crate::checks::Position::Machine(m) if public_ips.iter().any(|(n, _)| n == m) => Some((
                m.clone(),
                public_ips.iter().find(|(n, _)| n == m).map(|(_, e)| e.clone()).expect("found above"),
                ssh_users.iter().find(|(n, _)| n == m).map(|(_, u)| u.clone()).unwrap_or_else(|| "null".into()),
            )),
            _ => fallback.map(|(h, u)| ("controller".to_string(), h.to_string(), format!("\"{u}\""))),
        };
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

fn aws(spec: &Spec) -> GeneratedFile {
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
  backend "local" {{}}
  required_providers {{
    aws = {{
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }}{tls}
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
"#,
        tls = if spec.provision.is_empty() {
            ""
        } else {
            "\n    tls = {\n      source  = \"hashicorp/tls\"\n      version = \"~> 4.0\"\n    }"
        },
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
    if has_windows(spec) {
        tf.push_str("\n# The Windows machines' administrator password (user isoloom), for WinRM.\nresource \"random_password\" \"windows\" {\n  length      = 24\n  special     = false\n  min_upper   = 2\n  min_lower   = 2\n  min_numeric = 2\n}\n");
    }
    if needs_controller(spec) {
        oses.push(CONTROLLER_OS);
        tf.push_str("\n# The controller's own key: it runs the playbooks over SSH on every machine.\nresource \"tls_private_key\" \"controller\" {\n  algorithm = \"ED25519\"\n}\n");
    }
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
        if crate::images::is_windows(&vm.os) {
            windows_machine(spec, name, &mut tf, mem, disk, &mut published_out);
            public_ips.push((name.to_string(), pip.clone()));
            ssh_users.push((name.to_string(), "\"isoloom\"".into()));
            continue;
        }

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
        let redirects = redirects(m);
        for sv in &m.services {
            if let Some(h) = sv.publish {
                let _ = write!(
                    sg,
                    "  ingress {{\n    description = \"published\"\n    from_port   = {h}\n    to_port     = {h}\n    protocol    = \"tcp\"\n    cidr_blocks = [var.allowed_cidr]\n  }}\n"
                );
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
        let cmds = linux_setup_cmds(spec, name, m, vm, &lab, &redirects, &|nid| {
            tf_expr(&format!("lower(aws_network_interface.{id}_{nid}.mac_address)"))
        });

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

    if needs_controller(spec) {
        controller(spec, &mut tf, &lab);
    }

    // Outputs: every machine's address, and one to start from (the access machine, else the
    // first), with its SSH user and ready marker, as the other cloud outputs give.
    // Where a user stands (checks, the launcher's SSH): the access machine, else the first Linux
    // machine, else the controller (a Windows-only environment).
    let (first, check_user) = match access_ip {
        Some(ip) => (ip, None),
        None if needs_controller(spec) => ("aws_eip.isoloom_controller.public_ip".to_string(), Some("admin")),
        None => ("null".to_string(), None),
    };
    let _ = write!(
        tf,
        "\noutput \"machines\" {{\n  value = {{\n{}\n  }}\n}}\n\noutput \"ssh_users\" {{\n  value = {{\n{}\n  }}\n}}\n\noutput \"ip\" {{\n  value = {first}\n}}\n\noutput \"ready_file\" {{\n  value = \"/var/lib/isoloom/ready\"\n}}\n",
        aligned(&public_ips),
        aligned(&ssh_users),
    );
    let fallback = needs_controller(spec).then_some(("aws_eip.isoloom_controller.public_ip", "admin"));
    let _ = check_user;
    tf.push_str(&checks_output(spec, &public_ips, &ssh_users, fallback));
    if !published_out.is_empty() {
        let _ = write!(tf, "\noutput \"published\" {{\n  value = {{\n{}\n  }}\n}}\n", aligned(&published_out));
    }

    GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/aws/main.tf"),
        contents: tf,
    }
}

/// HCL map entries with their `=` aligned (as `terraform fmt` writes them).
pub(super) fn aligned(entries: &[(String, String)]) -> String {
    let w = entries.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    entries.iter().map(|(k, v)| format!("    {k:<w$} = {v}")).collect::<Vec<_>>().join("\n")
}

pub(super) const CONTROLLER_OS: &str = "debian-12";

/// A controller runs the environment's playbooks, and the checks when no Linux machine can
/// (a Windows-only environment).
pub(super) fn needs_controller(spec: &Spec) -> bool {
    !spec.provision.is_empty() || (!crate::checks::plan(spec).is_empty() && linux_machines(spec).next().is_none())
}

pub(super) fn linux_machines(spec: &Spec) -> impl Iterator<Item = (&String, &crate::model::Machine)> {
    spec.machines
        .iter()
        .filter(|(_, m)| m.vm.as_ref().is_some_and(|v| !crate::images::is_windows(&v.os)))
}

pub(super) fn has_windows(spec: &Spec) -> bool {
    spec.machines.values().any(|m| m.vm.as_ref().is_some_and(|v| crate::images::is_windows(&v.os)))
}

/// The redirects a machine needs: a published port that differs from the service's own port.
pub(super) fn redirects(m: &crate::model::Machine) -> Vec<(u16, u16)> {
    m.services
        .iter()
        .filter_map(|s| s.publish.filter(|h| *h != s.port).map(|h| (h, s.port)))
        .collect()
}

/// A Linux machine's set-up over SSH, the same on every cloud: the controller's key, its extra
/// interfaces (found by MAC, which each cloud expresses its own way), the other machines' names,
/// its volumes, its dependency waits, its provision steps, published-port redirects, the offline
/// egress rule, and the ready marker. `mac_expr(nid)` is the Terraform expression for the MAC of
/// the interface on network `nid` (the resource-safe network id).
pub(super) fn linux_setup_cmds(
    spec: &Spec,
    name: &str,
    m: &crate::model::Machine,
    vm: &crate::model::VmImpl,
    lab: &str,
    redirects: &[(u16, u16)],
    mac_expr: &dyn Fn(&str) -> String,
) -> Vec<String> {
    let mut cmds: Vec<String> = vec![
        "set -e".into(),
        "cloud-init status --wait >/dev/null 2>&1 || true".into(),
        "tar -xzf /tmp/isoloom-project.tgz -C /opt/isoloom && rm -f /tmp/isoloom-project.tgz".into(),
    ];
    if needs_controller(spec) {
        cmds.push(format!(
            "mkdir -p ~/.ssh && echo '{}' >> ~/.ssh/authorized_keys",
            tf_expr("trimspace(tls_private_key.controller.public_key_openssh)")
        ));
    }
    // Its other interfaces, found by MAC address, at their addresses.
    for (n, o) in m.networks.iter().skip(1) {
        let c = cidr(spec, n);
        cmds.push(format!(
            "IF=$(ip -o link | grep -i \"{mac}\" | awk -F': ' '{{print $2}}'); sudo ip link set \"$IF\" up && (ip -4 addr show \"$IF\" | grep -q {a}/ || sudo ip addr add {a}/{len} dev \"$IF\")",
            mac = mac_expr(&res(n)),
            a = address(spec, n, *o),
            len = c.len,
        ));
    }
    // The others by name, at their address on a network both are on (else their first).
    let hosts: Vec<String> = spec
        .machines
        .keys()
        .filter(|o| o.as_str() != name)
        .map(|o| format!("{} {}", super::address_for(spec, name, o), super::names_of(spec, o)))
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
    for (h, p) in redirects {
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
    cmds
}

/// The controller: a Debian instance with an interface on every network at the controller
/// address, which runs the environment's playbooks once every machine is set up.
fn controller(spec: &Spec, tf: &mut String, lab: &str) {
    let nets: Vec<&String> = spec.networks.keys().collect();
    let _ = lab;
    let mut sg = String::from(
        "\n# The controller (Ansible): every network may reach it, and SSH from allowed_cidr.\nresource \"aws_security_group\" \"isoloom_controller\" {\n  name   = \"${local.name}-controller\"\n  vpc_id = aws_vpc.env.id\n",
    );
    for n in &nets {
        let _ = write!(
            sg,
            "  ingress {{\n    description = \"{n}\"\n    from_port   = 0\n    to_port     = 0\n    protocol    = \"-1\"\n    cidr_blocks = [\"{}\"]\n  }}\n",
            spec.networks[n.as_str()].cidr
        );
    }
    sg.push_str("  ingress {\n    description = \"SSH from allowed_cidr\"\n    from_port   = 22\n    to_port     = 22\n    protocol    = \"tcp\"\n    cidr_blocks = [var.allowed_cidr]\n  }\n  egress {\n    from_port   = 0\n    to_port     = 0\n    protocol    = \"-1\"\n    cidr_blocks = [\"0.0.0.0/0\"]\n  }\n}\n");
    tf.push_str(&sg);
    for n in &nets {
        let _ = writeln!(
            tf,
            "\nresource \"aws_network_interface\" \"isoloom_controller_{nid}\" {{\n  subnet_id         = aws_subnet.{nid}.id\n  private_ips       = [\"{a}\"]\n  security_groups   = [aws_security_group.isoloom_controller.id]\n  source_dest_check = false\n}}",
            nid = res(n),
            a = cidr(spec, n).controller(),
        );
    }
    let first = res(nets[0]);
    let ctl_size = instance_type(super::controller_size(spec).1);
    let _ = writeln!(
        tf,
        "\nresource \"aws_instance\" \"isoloom_controller\" {{\n  ami           = data.aws_ami.{os}.id\n  instance_type = \"{ctl_size}\"\n  key_name      = aws_key_pair.env.key_name\n  user_data     = var.auto_stop_minutes > 0 ? \"#!/bin/sh\\nshutdown -h +${{var.auto_stop_minutes}}\\n\" : null\n  primary_network_interface {{\n    network_interface_id = aws_network_interface.isoloom_controller_{first}.id\n  }}\n  tags = {{ Name = \"${{local.name}}-controller\" }}\n}}\n\nresource \"aws_eip\" \"isoloom_controller\" {{\n  network_interface = aws_network_interface.isoloom_controller_{first}.id\n  depends_on        = [aws_internet_gateway.env]\n}}",
        os = res(CONTROLLER_OS),
    );
    for (i, n) in nets.iter().enumerate().skip(1) {
        let _ = writeln!(
            tf,
            "\nresource \"aws_network_interface_attachment\" \"isoloom_controller_{nid}\" {{\n  instance_id          = aws_instance.isoloom_controller.id\n  network_interface_id = aws_network_interface.isoloom_controller_{nid}.id\n  device_index         = {i}\n}}",
            nid = res(n),
        );
    }
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
            mac = tf_expr(&format!("lower(aws_network_interface.isoloom_controller_{}.mac_address)", res(n))),
            a = c.controller(),
            len = c.len,
        ));
    }
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
    cmds.push(format!("sudo sh -c {}", sh_quote(&super::vagrant::ansible_runs(spec, &[]))));
    cmds.push("sudo mkdir -p /var/lib/isoloom && echo ready | sudo tee /var/lib/isoloom/ready >/dev/null".into());
    let deps: Vec<String> = spec
        .machines
        .iter()
        .filter(|(_, m)| m.vm.is_some())
        .map(|(n, _)| format!("terraform_data.{}", res(n)))
        .collect();
    let _ = writeln!(
        tf,
        "\nresource \"terraform_data\" \"isoloom_controller\" {{\n  triggers_replace = [aws_instance.isoloom_controller.id]\n  connection {{\n    type        = \"ssh\"\n    host        = aws_eip.isoloom_controller.public_ip\n    user        = \"admin\"\n    private_key = file(pathexpand(var.ssh_private_key_file))\n    timeout     = \"10m\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\"cloud-init status --wait >/dev/null 2>&1 || true\", \"sudo mkdir -p /opt/isoloom && sudo chown admin /opt/isoloom\"]\n  }}\n  provisioner \"local-exec\" {{\n    command = \"tar -czf \\\"${{path.module}}/.isoloom-project-controller.tgz\\\" --exclude=.git --exclude=.vagrant --exclude=.terraform --exclude=.isoloom-project*.tgz -C \\\"${{local.root}}\\\" .\"\n  }}\n  provisioner \"file\" {{\n    source      = \"${{path.module}}/.isoloom-project-controller.tgz\"\n    destination = \"/tmp/isoloom-project.tgz\"\n  }}\n  provisioner \"file\" {{\n    content     = tls_private_key.controller.private_key_openssh\n    destination = \"/tmp/isoloom-controller-key\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\n{}\n    ]\n  }}\n  depends_on = [{}]\n}}",
        cmds.iter().map(|c| format!("      {}", hcl_cmd(c))).collect::<Vec<_>>().join(",\n"),
        deps.join(", "),
    );
}

/// A Windows machine: Amazon's image; at first boot (user data) the `isoloom` administrator with
/// the generated password, WinRM over HTTP, its name; then over WinRM (from `allowed_cidr`): the
/// other machines' names, its `.ps1` steps, its firewall openings, published ports, the ready
/// marker.
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
        "\n# Machine `{name}` (Windows): what may reach it.\nresource \"aws_security_group\" \"{id}\" {{\n  name   = \"${{local.name}}-{name}\"\n  vpc_id = aws_vpc.env.id\n  ingress {{\n    description = \"its network ({net})\"\n    from_port   = 0\n    to_port     = 0\n    protocol    = \"-1\"\n    cidr_blocks = [\"{c}\"]\n  }}\n  ingress {{\n    description = \"WinRM from allowed_cidr (its set-up)\"\n    from_port   = 5985\n    to_port     = 5985\n    protocol    = \"tcp\"\n    cidr_blocks = [var.allowed_cidr]\n  }}\n",
        c = spec.networks[net.as_str()].cidr
    );
    for r in spec.reach.iter().filter(|r| &r.to == net) {
        let from = &spec.networks[&r.from].cidr;
        let ports: Vec<String> = if r.ports.is_empty() {
            vec!["0".into()]
        } else {
            r.ports.iter().map(u16::to_string).collect()
        };
        for p in ports {
            let (proto, to) = if p == "0" { ("-1", "0".to_string()) } else { ("tcp", p.clone()) };
            let _ = write!(
                sg,
                "  ingress {{\n    description = \"reach from {f}\"\n    from_port   = {p}\n    to_port     = {to}\n    protocol    = \"{proto}\"\n    cidr_blocks = [\"{from}\"]\n  }}\n",
                f = r.from
            );
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
            let _ = write!(
                sg,
                "  ingress {{\n    description = \"published\"\n    from_port   = {h}\n    to_port     = {h}\n    protocol    = \"tcp\"\n    cidr_blocks = [var.allowed_cidr]\n  }}\n"
            );
            let label = sv.name.clone().unwrap_or_else(|| sv.port.to_string());
            published_out.push((format!("\"{name}/{label}\""), format!("\"${{aws_instance.{id}.public_ip}}:{h}\"")));
            if h != sv.port {
                ps.push(format!(
                    "netsh interface portproxy add v4tov4 listenport={h} listenaddress=0.0.0.0 connectport={p} connectaddress=127.0.0.1 | Out-Null",
                    p = sv.port
                ));
            }
        }
    }
    sg.push_str("  egress {\n    from_port   = 0\n    to_port     = 0\n    protocol    = \"-1\"\n    cidr_blocks = [\"0.0.0.0/0\"]\n  }\n}\n");
    tf.push_str(&sg);

    // First boot: the administrator and WinRM.
    let user_data = "<powershell>\n$p = ConvertTo-SecureString '${random_password.windows.result}' -AsPlainText -Force\nNew-LocalUser -Name isoloom -Password $p -PasswordNeverExpires -AccountNeverExpires | Out-Null\nAdd-LocalGroupMember -Group Administrators -Member isoloom\nEnable-PSRemoting -Force -SkipNetworkProfileCheck | Out-Null\nSet-Item WSMan:\\localhost\\Service\\AllowUnencrypted $true\nSet-Item WSMan:\\localhost\\Service\\Auth\\Basic $true\nNew-NetFirewallRule -DisplayName 'isoloom WinRM' -Direction Inbound -Protocol TCP -LocalPort 5985 -Action Allow | Out-Null\n${var.auto_stop_minutes > 0 ? \"Register-ScheduledTask -TaskName isoloom-auto-stop -Action (New-ScheduledTaskAction -Execute shutdown.exe -Argument '/s /t 0') -Trigger (New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(${var.auto_stop_minutes})) -User SYSTEM -RunLevel Highest -Force | Out-Null\" : \"\"}\n</powershell>\n".to_string();
    let _ = writeln!(
        tf,
        "\nresource \"aws_instance\" \"{id}\" {{\n  ami                         = data.aws_ami.{ami}.id\n  instance_type               = \"{itype}\"\n  subnet_id                   = aws_subnet.{netid}.id\n  private_ip                  = \"{addr}\"\n  associate_public_ip_address = true\n  vpc_security_group_ids      = [aws_security_group.{id}.id]\n  # No key pair: AWS refuses ED25519 keys on Windows, and WinRM uses the generated password.\n  user_data = <<-EOT\n{ud}  EOT\n  root_block_device {{\n    volume_size = {disk}\n  }}\n  tags = {{ Name = \"${{local.name}}-{name}\" }}\n}}",
        ami = res(&vm.os),
        itype = instance_type(mem),
        netid = res(net),
        ud = user_data.lines().map(|l| format!("    {l}\n")).collect::<String>(),
        disk = disk.max(50),
    );

    // Over WinRM: names, firewall openings for its services, steps, the ready marker.
    let hosts: Vec<String> = spec
        .machines
        .keys()
        .filter(|o| o.as_str() != name)
        .map(|o| format!("{} {}", super::address_for(spec, name, o), super::names_of(spec, o)))
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
    for step in &vm.provision {
        ps.push(format!(
            "& powershell -NoProfile -ExecutionPolicy Bypass -File 'C:\\isoloom\\{}'; if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}",
            step.replace('/', "\\")
        ));
    }
    ps.push("New-Item -ItemType Directory -Force C:\\ProgramData\\isoloom | Out-Null; Set-Content C:\\ProgramData\\isoloom\\ready 'ready'".into());

    let conn = format!(
        "  connection {{\n    type     = \"winrm\"\n    host     = aws_instance.{id}.public_ip\n    user     = \"isoloom\"\n    password = random_password.windows.result\n    https    = false\n    timeout  = \"30m\"\n  }}\n"
    );
    // Its name: renamed, restarted, then a pause so the set-up reconnects after the restart.
    let _ = writeln!(
        tf,
        "\nresource \"terraform_data\" \"{id}_name\" {{\n  triggers_replace = [aws_instance.{id}.id]\n{conn}  provisioner \"remote-exec\" {{\n    inline = [\"powershell -NoProfile -Command \\\"if ($env:COMPUTERNAME -ne '{up}') {{ Rename-Computer -NewName '{host}' -Force; shutdown /r /t 10 }}\\\"\"]\n  }}\n}}\n\nresource \"time_sleep\" \"{id}_restart\" {{\n  create_duration = \"90s\"\n  depends_on      = [terraform_data.{id}_name]\n}}",
        up = host.to_uppercase(),
    );
    let mut prov = format!("\nresource \"terraform_data\" \"{id}\" {{\n  triggers_replace = [aws_instance.{id}.id]\n{conn}");
    for step in &vm.provision {
        let _ = write!(
            prov,
            "  provisioner \"file\" {{\n    source      = \"${{local.root}}/{step}\"\n    destination = \"C:/isoloom/{step}\"\n  }}\n"
        );
    }
    let _ = write!(
        prov,
        "  provisioner \"file\" {{\n    content     = {}\n    destination = \"C:/isoloom/setup.ps1\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\"powershell -NoProfile -ExecutionPolicy Bypass -File C:/isoloom/setup.ps1\"]\n  }}\n",
        hcl(&(ps.join("\n") + "\n"))
    );
    let mut deps: Vec<String> = vec![format!("time_sleep.{id}_restart")];
    deps.extend(
        m.depends_on
            .iter()
            .filter(|d| spec.machines[*d].vm.is_some())
            .map(|d| format!("terraform_data.{}", res(d))),
    );
    let _ = writeln!(prov, "  depends_on = [{}]", deps.join(", "));
    prov.push_str("}\n");
    tf.push_str(&prov);
}

/// The controller's inventory: every machine at its address, with its SSH user and the
/// controller's key; the spec's groups.
pub(super) fn inventory(spec: &Spec) -> String {
    let (mut linux, mut windows) = (String::new(), String::new());
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        let Some((net, octet)) = m.networks.first() else { continue };
        let user = image(&vm.os).map(|i| i.2).unwrap_or("admin");
        if crate::images::is_windows(&vm.os) {
            let _ = writeln!(windows, "{name} ansible_host={}", address(spec, net, *octet));
        } else {
            let _ = writeln!(linux, "{name} ansible_host={} ansible_user={user}", address(spec, net, *octet));
        }
    }
    let mut inv = format!("[linux]\n{linux}\n[windows]\n{windows}\n");
    inv.push_str("[linux:vars]\nansible_ssh_private_key_file=/etc/isoloom/id_ed25519\nansible_become=true\n");
    if has_windows(spec) {
        let _ = write!(
            inv,
            "\n[windows:vars]\nansible_user=isoloom\nansible_password={}\nansible_connection=winrm\nansible_port=5985\nansible_winrm_scheme=http\nansible_winrm_transport=basic\nansible_winrm_server_cert_validation=ignore\nansible_winrm_operation_timeout_sec=400\nansible_winrm_read_timeout_sec=500\n",
            tf_expr("random_password.windows.result")
        );
    }
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
        let _ = write!(inv, "\n[{g}]\n{}\n", members.join("\n"));
    }
    inv
}

/// A Terraform expression inside a set-up command: kept through `hcl`'s escaping (which turns
/// `${` into a literal) and turned into `${expr}` by `hcl_cmd`.
pub(super) fn tf_expr(expr: &str) -> String {
    format!("\u{1}{expr}\u{2}")
}

/// A set-up command as an HCL string, its `tf` expressions interpolated. Only the text around
/// them is escaped: an expression's own quotes (`replace(x, "-", ":")`) stay HCL.
pub(super) fn hcl_cmd(c: &str) -> String {
    let mut out = String::new();
    let mut rest = c;
    while let Some(start) = rest.find('\u{1}') {
        let end = rest[start..].find('\u{2}').map(|e| start + e).unwrap_or(rest.len());
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

/// A single-quoted shell word.
pub(super) fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
