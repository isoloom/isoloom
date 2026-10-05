//! The `cloud-docker` target: the environment's Compose file, run by Docker on one cloud VM, as
//! Terraform per cloud under `.isoloom/cloud-docker/<cloud>/` (AWS for now).
//!
//! - Its own network (VPC, subnet, internet gateway), a security group letting `allowed_cidr`
//!   in for SSH and the published ports, and one Debian instance sized for every machine.
//! - Over SSH (Terraform provisioners): the project is copied to /opt/isoloom, Docker installed,
//!   `docker compose up --wait` run, then /var/lib/isoloom/ready written.
//! - `auto_stop_minutes`: the instance shuts itself down (and is terminated) after that long, so a
//!   forgotten environment stops costing.

use std::fmt::Write;

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, header};
use crate::model::Spec;

/// The published ports of the machines that have a container form.
fn published(spec: &Spec) -> Vec<u16> {
    let mut ports: Vec<u16> = spec
        .machines
        .values()
        .filter(|m| m.docker.is_some())
        .flat_map(|m| m.services.iter().filter_map(|s| s.publish))
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
}

/// The memory every machine needs at once, plus Docker's own, in MB.
fn memory_mb(spec: &Spec) -> u32 {
    spec.machines
        .values()
        .filter(|m| m.docker.is_some())
        .map(|m| m.resources.and_then(|r| r.memory_mb).unwrap_or(512))
        .sum::<u32>()
        + 1024
}

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    Ok(vec![aws(spec)])
}

fn aws(spec: &Spec) -> GeneratedFile {
    let mem = memory_mb(spec);
    let instance = if mem <= 4096 {
        "t3.medium"
    } else if mem <= 8192 {
        "t3.large"
    } else {
        "t3.xlarge"
    };
    let disk: u32 = spec
        .machines
        .values()
        .filter(|m| m.docker.is_some())
        .map(|m| m.resources.and_then(|r| r.disk_gb).unwrap_or(5))
        .sum::<u32>()
        .max(20);
    let mut tf = header("#");
    tf.push_str("# Start:  terraform -chdir=.isoloom/cloud-docker/aws init && terraform -chdir=.isoloom/cloud-docker/aws apply \\\n#           -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-docker/aws destroy (same variables)\n\n");
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
  description = "Who may reach the VM (SSH and the published ports), e.g. your IP/32"
}}
variable "ssh_public_key" {{
  type = string
}}
variable "ssh_private_key_file" {{
  type        = string
  description = "The private key of ssh_public_key: Terraform copies the project over SSH"
}}
variable "instance_type" {{
  type    = string
  default = "{instance}"
}}
variable "auto_stop_minutes" {{
  type        = number
  default     = 0
  description = "Shut the VM down (and terminate it) after this long; 0 = never"
}}
"#
    );
    if !spec.inputs.is_empty() {
        tf.push_str("variable \"inputs\" {\n  type      = map(string)\n  default   = {}\n  sensitive = true\n}\n");
    }
    let _ = write!(
        tf,
        r#"
provider "aws" {{
  region = var.region
  default_tags {{
    tags = {{
      "isoloom:environment" = "{name}"
      "managed-by"          = "isoloom"
    }}
  }}
}}

# A suffix, so two copies of the environment in one account don't collide.
resource "terraform_data" "id" {{
  input = substr(replace(uuid(), "-", ""), 0, 8)
  lifecycle {{
    ignore_changes = [input]
  }}
}}

locals {{
  name = "isoloom-{name}-${{terraform_data.id.output}}"
  root = abspath("${{path.module}}/../../..")
}}

data "aws_availability_zones" "available" {{
  state = "available"
}}

data "aws_ami" "debian" {{
  most_recent = true
  owners      = ["136693071363"]
  filter {{
    name   = "name"
    values = ["debian-12-amd64-*"]
  }}
}}

resource "aws_vpc" "env" {{
  cidr_block = "10.42.0.0/16"
  tags       = {{ Name = local.name }}
}}

resource "aws_internet_gateway" "env" {{
  vpc_id = aws_vpc.env.id
  tags   = {{ Name = local.name }}
}}

resource "aws_subnet" "env" {{
  vpc_id                  = aws_vpc.env.id
  cidr_block              = "10.42.1.0/24"
  availability_zone       = data.aws_availability_zones.available.names[0]
  map_public_ip_on_launch = true
  tags                    = {{ Name = local.name }}
}}

resource "aws_route_table" "env" {{
  vpc_id = aws_vpc.env.id
  route {{
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.env.id
  }}
  tags = {{ Name = local.name }}
}}

resource "aws_route_table_association" "env" {{
  subnet_id      = aws_subnet.env.id
  route_table_id = aws_route_table.env.id
}}

resource "aws_key_pair" "env" {{
  key_name   = local.name
  public_key = var.ssh_public_key
}}

resource "aws_security_group" "env" {{
  name   = local.name
  vpc_id = aws_vpc.env.id
  ingress {{
    description = "SSH"
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = [var.allowed_cidr]
  }}
"#,
        name = spec.name
    );
    for p in published(spec) {
        let _ = writeln!(
            tf,
            "  ingress {{\n    description = \"published {p}\"\n    from_port   = {p}\n    to_port     = {p}\n    protocol    = \"tcp\"\n    cidr_blocks = [var.allowed_cidr]\n  }}"
        );
    }
    let inputs_file = if spec.inputs.is_empty() {
        String::new()
    } else {
        "  provisioner \"file\" {\n    content     = join(\"\\n\", [for k, v in var.inputs : \"${k}=${jsonencode(v)}\"])\n    destination = \"/tmp/isoloom-inputs.env\"\n  }\n".to_string()
    };
    let source_inputs = if spec.inputs.is_empty() {
        ""
    } else {
        "set -a; . /tmp/isoloom-inputs.env; set +a; "
    };
    let _ = write!(
        tf,
        r##"  egress {{
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }}
}}

resource "aws_instance" "env" {{
  ami                                  = data.aws_ami.debian.id
  instance_type                        = var.instance_type
  subnet_id                            = aws_subnet.env.id
  key_name                             = aws_key_pair.env.key_name
  vpc_security_group_ids               = [aws_security_group.env.id]
  instance_initiated_shutdown_behavior = "terminate"
  user_data                            = var.auto_stop_minutes > 0 ? "#!/bin/sh\nshutdown -h +${{var.auto_stop_minutes}}\n" : null
  root_block_device {{
    volume_size = {disk}
    volume_type = "gp3"
    encrypted   = true
  }}
  metadata_options {{
    http_tokens = "required"
  }}
  depends_on = [aws_route_table_association.env]
  tags       = {{ Name = local.name }}
}}

# The environment, over SSH: the project, Docker, then the Compose file.
resource "terraform_data" "environment" {{
  triggers_replace = [aws_instance.env.id]
  connection {{
    type        = "ssh"
    host        = aws_instance.env.public_ip
    user        = "admin"
    private_key = file(pathexpand(var.ssh_private_key_file))
    timeout     = "10m"
  }}
  provisioner "remote-exec" {{
    inline = ["cloud-init status --wait >/dev/null 2>&1 || true", "sudo mkdir -p /opt/isoloom && sudo chown admin /opt/isoloom"]
  }}
  provisioner "file" {{
    source      = "${{local.root}}/"
    destination = "/opt/isoloom"
  }}
{inputs_file}  provisioner "remote-exec" {{
    inline = [
      "command -v docker >/dev/null || curl -fsSL https://get.docker.com | sudo sh",
      "cd /opt/isoloom && {source_inputs}sudo -E env ISOLOOM_PUBLISH_ADDRESS=0.0.0.0 docker compose -f .isoloom/docker/compose.yml up -d --build --wait --wait-timeout 900",
      "sudo mkdir -p /var/lib/isoloom && sudo touch /var/lib/isoloom/ready",
    ]
  }}
}}

output "ip" {{
  value = aws_instance.env.public_ip
}}
output "ssh_user" {{
  value = "admin"
}}
output "ready_file" {{
  value = "/var/lib/isoloom/ready"
}}
"##
    );
    GeneratedFile {
        path: format!("{OUTPUT_DIR}/cloud-docker/aws/main.tf"),
        contents: tf,
    }
}
