//! The `cloud-docker` target: the environment's Compose file, run by Docker on one cloud VM, as
//! Terraform per cloud under `.isoloom/cloud-docker/<cloud>/`: AWS, Azure, Google Cloud,
//! DigitalOcean, Linode and Oracle Cloud.
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
    Ok(vec![
        aws(spec),
        other(spec, "azure", AZURE),
        other(spec, "gcp", GCP),
        other(spec, "digitalocean", DIGITALOCEAN),
        other(spec, "linode", LINODE),
        other(spec, "oci", OCI),
    ])
}

/// The variables every cloud shares.
const COMMON_VARS: &str = r#"variable "allowed_cidr" {
  type        = string
  description = "Who may reach the VM (SSH and the published ports), e.g. your IP/32"
}
variable "ssh_public_key" {
  type = string
}
variable "ssh_private_key_file" {
  type        = string
  description = "The private key of ssh_public_key: Terraform copies the project over SSH"
}
"#;

/// The environment over SSH, on any cloud: the project, Docker, the Compose file, the ready
/// marker. `__HOST__`, `__USER__` and `__ID__` are the VM's address, login and id expressions.
const ENVIRONMENT: &str = r#"
# The environment, over SSH: the project, Docker, then the Compose file.
resource "terraform_data" "environment" {
  triggers_replace = [__ID__]
  connection {
    type        = "ssh"
    host        = __HOST__
    user        = "__USER__"
    private_key = file(pathexpand(var.ssh_private_key_file))
    timeout     = "10m"
  }
  provisioner "remote-exec" {
    inline = ["cloud-init status --wait >/dev/null 2>&1 || true", "sudo mkdir -p /opt/isoloom && sudo chown __USER__ /opt/isoloom"]
  }
  provisioner "file" {
    source      = "${local.root}/"
    destination = "/opt/isoloom"
  }
__INPUTS_FILE__  provisioner "remote-exec" {
    inline = [
      "command -v docker >/dev/null || curl -fsSL https://get.docker.com | sudo sh",
      "cd /opt/isoloom && __SOURCE_INPUTS__sudo -E env ISOLOOM_PUBLISH_ADDRESS=0.0.0.0 docker compose -f .isoloom/docker/compose.yml up -d --build --wait --wait-timeout 900",
      "sudo mkdir -p /var/lib/isoloom && sudo touch /var/lib/isoloom/ready",
    ]
  }
}

output "ip" {
  value = __HOST__
}
output "ssh_user" {
  value = "__USER__"
}
output "ready_file" {
  value = "/var/lib/isoloom/ready"
}
"#;

/// A cloud's Terraform from its template: the provider, the network, firewall and VM
/// (`__NAME__`, `__SIZE__`, `__PORTS__` filled in), then the shared variables and environment.
fn other(spec: &Spec, cloud: &str, template: &str) -> GeneratedFile {
    let mem = memory_mb(spec);
    let size = |small: &str, medium: &str, large: &str| {
        (if mem <= 4096 {
            small
        } else if mem <= 8192 {
            medium
        } else {
            large
        })
        .to_string()
    };
    let (sizes, user, host, id) = match cloud {
        "azure" => (
            size("Standard_B2s", "Standard_B2ms", "Standard_D4s_v5"),
            "isoloom",
            "azurerm_public_ip.env.ip_address",
            "azurerm_linux_virtual_machine.env.id",
        ),
        "gcp" => (
            size("e2-medium", "e2-standard-2", "e2-standard-4"),
            "isoloom",
            "google_compute_instance.env.network_interface[0].access_config[0].nat_ip",
            "google_compute_instance.env.id",
        ),
        "digitalocean" => (
            size("s-2vcpu-4gb", "s-4vcpu-8gb", "s-8vcpu-16gb"),
            "root",
            "digitalocean_droplet.env.ipv4_address",
            "digitalocean_droplet.env.id",
        ),
        "linode" => (
            size("g6-standard-2", "g6-standard-4", "g6-standard-6"),
            "root",
            "one(linode_instance.env.ipv4)",
            "linode_instance.env.id",
        ),
        _ => (
            size("VM.Standard.E4.Flex", "VM.Standard.E4.Flex", "VM.Standard.E4.Flex"),
            "ubuntu",
            "oci_core_instance.env.public_ip",
            "oci_core_instance.env.id",
        ),
    };
    let ports = published(spec);
    let ports_list = ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", ");
    let ports_quoted = ports.iter().map(|p| format!("\"{p}\"")).collect::<Vec<_>>().join(", ");
    let ocpus_mem = if mem <= 4096 {
        (1, 4)
    } else if mem <= 8192 {
        (2, 8)
    } else {
        (4, 16)
    };
    let mut tf = header("#");
    tf.push_str(&format!(
        "# Start:  terraform -chdir=.isoloom/cloud-docker/{cloud} init && terraform -chdir=.isoloom/cloud-docker/{cloud} apply \\\n#           -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-docker/{cloud} destroy (same variables)\n\n"
    ));
    tf.push_str(
        &template
            .replace("__NAME__", &spec.name)
            .replace("__SIZE__", &sizes)
            .replace("__PORTS_LIST__", &ports_list)
            .replace("__PORTS_QUOTED__", &ports_quoted)
            .replace("__OCPUS__", &ocpus_mem.0.to_string())
            .replace("__OMEM__", &ocpus_mem.1.to_string()),
    );
    tf.push_str(COMMON_VARS);
    if !spec.inputs.is_empty() {
        tf.push_str("variable \"inputs\" {\n  type      = map(string)\n  default   = {}\n  sensitive = true\n}\n");
    }
    let (inputs_file, source_inputs) = if spec.inputs.is_empty() {
        (String::new(), "")
    } else {
        (
            "  provisioner \"file\" {\n    content     = join(\"\\n\", [for k, v in var.inputs : \"${k}=${jsonencode(v)}\"])\n    destination = \"/tmp/isoloom-inputs.env\"\n  }\n".to_string(),
            "set -a; . /tmp/isoloom-inputs.env; set +a; ",
        )
    };
    tf.push_str(
        &ENVIRONMENT
            .replace("__HOST__", host)
            .replace("__USER__", user)
            .replace("__ID__", id)
            .replace("__INPUTS_FILE__", &inputs_file)
            .replace("__SOURCE_INPUTS__", source_inputs),
    );
    GeneratedFile {
        path: format!("{OUTPUT_DIR}/cloud-docker/{cloud}/main.tf"),
        contents: tf,
    }
}

/// The published ports as an Azure security rule, a GCP firewall allow, etc. (an empty list
/// when nothing is published: the templates then open SSH only).
const AZURE: &str = r#"terraform {
  required_version = ">= 1.6"
  required_providers {
    azurerm = {
      source  = "hashicorp/azurerm"
      version = "~> 5.0"
    }
  }
}

variable "subscription_id" {
  type = string
}
variable "location" {
  type    = string
  default = "francecentral"
}
variable "size" {
  type    = string
  default = "__SIZE__"
}

provider "azurerm" {
  features {}
  subscription_id = var.subscription_id
}

resource "terraform_data" "id" {
  input = substr(replace(uuid(), "-", ""), 0, 8)
  lifecycle {
    ignore_changes = [input]
  }
}

locals {
  name = "isoloom-__NAME__-${terraform_data.id.output}"
  root = abspath("${path.module}/../../..")
  tags = { "isoloom-environment" = "__NAME__", "managed-by" = "isoloom" }
}

resource "azurerm_resource_group" "env" {
  name     = local.name
  location = var.location
  tags     = local.tags
}

resource "azurerm_virtual_network" "env" {
  name                = local.name
  resource_group_name = azurerm_resource_group.env.name
  location            = var.location
  address_space       = ["10.42.0.0/16"]
}

resource "azurerm_subnet" "env" {
  name                 = local.name
  resource_group_name  = azurerm_resource_group.env.name
  virtual_network_name = azurerm_virtual_network.env.name
  address_prefixes     = ["10.42.1.0/24"]
}

resource "azurerm_public_ip" "env" {
  name                = local.name
  resource_group_name = azurerm_resource_group.env.name
  location            = var.location
  allocation_method   = "Static"
  sku                 = "Standard"
}

resource "azurerm_network_security_group" "env" {
  name                = local.name
  resource_group_name = azurerm_resource_group.env.name
  location            = var.location
  security_rule {
    name                       = "ssh-and-published"
    priority                   = 100
    direction                  = "Inbound"
    access                     = "Allow"
    protocol                   = "Tcp"
    source_port_range          = "*"
    destination_port_ranges    = concat(["22"], [__PORTS_QUOTED__])
    source_address_prefix      = var.allowed_cidr
    destination_address_prefix = "*"
  }
}

resource "azurerm_network_interface" "env" {
  name                = local.name
  resource_group_name = azurerm_resource_group.env.name
  location            = var.location
  ip_configuration {
    name                          = "env"
    subnet_id                     = azurerm_subnet.env.id
    private_ip_address_allocation = "Dynamic"
    public_ip_address_id          = azurerm_public_ip.env.id
  }
}

resource "azurerm_network_interface_security_group_association" "env" {
  network_interface_id      = azurerm_network_interface.env.id
  network_security_group_id = azurerm_network_security_group.env.id
}

resource "azurerm_linux_virtual_machine" "env" {
  name                  = local.name
  resource_group_name   = azurerm_resource_group.env.name
  location              = var.location
  size                  = var.size
  admin_username        = "isoloom"
  network_interface_ids = [azurerm_network_interface.env.id]
  admin_ssh_key {
    username   = "isoloom"
    public_key = var.ssh_public_key
  }
  os_disk {
    caching              = "ReadWrite"
    storage_account_type = "StandardSSD_LRS"
  }
  source_image_reference {
    publisher = "Debian"
    offer     = "debian-12"
    sku       = "12-gen2"
    version   = "latest"
  }
  tags       = local.tags
  depends_on = [azurerm_network_interface_security_group_association.env]
}

"#;

const GCP: &str = r#"terraform {
  required_version = ">= 1.6"
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 8.0"
    }
  }
}

variable "project" {
  type = string
}
variable "region" {
  type    = string
  default = "europe-west9"
}
variable "machine_type" {
  type    = string
  default = "__SIZE__"
}

provider "google" {
  project = var.project
  region  = var.region
}

resource "terraform_data" "id" {
  input = substr(replace(uuid(), "-", ""), 0, 8)
  lifecycle {
    ignore_changes = [input]
  }
}

locals {
  name = "isoloom-__NAME__-${terraform_data.id.output}"
  root = abspath("${path.module}/../../..")
}

resource "google_compute_network" "env" {
  name                    = local.name
  auto_create_subnetworks = false
}

resource "google_compute_subnetwork" "env" {
  name          = local.name
  network       = google_compute_network.env.id
  ip_cidr_range = "10.42.1.0/24"
  region        = var.region
}

resource "google_compute_firewall" "env" {
  name          = local.name
  network       = google_compute_network.env.id
  source_ranges = [var.allowed_cidr]
  allow {
    protocol = "tcp"
    ports    = concat(["22"], [__PORTS_QUOTED__])
  }
}

resource "google_compute_instance" "env" {
  name         = local.name
  machine_type = var.machine_type
  zone         = "${var.region}-a"
  labels       = { "isoloom-environment" = "__NAME__", "managed-by" = "isoloom" }
  boot_disk {
    initialize_params {
      image = "debian-cloud/debian-12"
      size  = 30
    }
  }
  network_interface {
    subnetwork = google_compute_subnetwork.env.id
    access_config {}
  }
  metadata = {
    ssh-keys = "isoloom:${var.ssh_public_key}"
  }
}

"#;

const DIGITALOCEAN: &str = r#"terraform {
  required_version = ">= 1.6"
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
variable "size" {
  type    = string
  default = "__SIZE__"
}

resource "terraform_data" "id" {
  input = substr(replace(uuid(), "-", ""), 0, 8)
  lifecycle {
    ignore_changes = [input]
  }
}

locals {
  name = "isoloom-__NAME__-${terraform_data.id.output}"
  root = abspath("${path.module}/../../..")
}

resource "digitalocean_vpc" "env" {
  name     = local.name
  region   = var.region
  ip_range = "10.42.1.0/24"
}

resource "digitalocean_ssh_key" "env" {
  name       = local.name
  public_key = var.ssh_public_key
}

resource "digitalocean_droplet" "env" {
  name     = local.name
  region   = var.region
  size     = var.size
  image    = "debian-12-x64"
  vpc_uuid = digitalocean_vpc.env.id
  ssh_keys = [digitalocean_ssh_key.env.id]
  tags     = ["isoloom", "__NAME__"]
}

resource "digitalocean_firewall" "env" {
  name        = local.name
  droplet_ids = [digitalocean_droplet.env.id]
  inbound_rule {
    protocol         = "tcp"
    port_range       = "22"
    source_addresses = [var.allowed_cidr]
  }
  dynamic "inbound_rule" {
    for_each = [__PORTS_LIST__]
    content {
      protocol         = "tcp"
      port_range       = tostring(inbound_rule.value)
      source_addresses = [var.allowed_cidr]
    }
  }
  outbound_rule {
    protocol              = "tcp"
    port_range            = "1-65535"
    destination_addresses = ["0.0.0.0/0", "::/0"]
  }
  outbound_rule {
    protocol              = "udp"
    port_range            = "1-65535"
    destination_addresses = ["0.0.0.0/0", "::/0"]
  }
}

"#;

const LINODE: &str = r#"terraform {
  required_version = ">= 1.6"
  required_providers {
    linode = {
      source  = "linode/linode"
      version = "~> 4.0"
    }
  }
}

variable "region" {
  type    = string
  default = "fr-par"
}
variable "type" {
  type    = string
  default = "__SIZE__"
}

resource "terraform_data" "id" {
  input = substr(replace(uuid(), "-", ""), 0, 8)
  lifecycle {
    ignore_changes = [input]
  }
}

locals {
  name = "isoloom-__NAME__-${terraform_data.id.output}"
  root = abspath("${path.module}/../../..")
}

resource "linode_instance" "env" {
  label           = local.name
  region          = var.region
  type            = var.type
  image           = "linode/debian12"
  authorized_keys = [trimspace(var.ssh_public_key)]
  tags            = ["isoloom", "__NAME__"]
}

resource "linode_firewall" "env" {
  label           = local.name
  inbound_policy  = "DROP"
  outbound_policy = "ACCEPT"
  inbound {
    label    = "ssh-and-published"
    action   = "ACCEPT"
    protocol = "TCP"
    ports    = join(",", concat(["22"], [__PORTS_QUOTED__]))
    ipv4     = [var.allowed_cidr]
  }
  linodes = [linode_instance.env.id]
}

"#;

const OCI: &str = r#"terraform {
  required_version = ">= 1.6"
  required_providers {
    oci = {
      source  = "oracle/oci"
      version = "~> 9.0"
    }
  }
}

variable "compartment_id" {
  type = string
}
variable "region" {
  type    = string
  default = "eu-paris-1"
}

provider "oci" {
  region = var.region
}

resource "terraform_data" "id" {
  input = substr(replace(uuid(), "-", ""), 0, 8)
  lifecycle {
    ignore_changes = [input]
  }
}

locals {
  name = "isoloom-__NAME__-${terraform_data.id.output}"
  root = abspath("${path.module}/../../..")
}

data "oci_identity_availability_domains" "ads" {
  compartment_id = var.compartment_id
}

# Ubuntu: Oracle Cloud has no Debian image of its own.
data "oci_core_images" "ubuntu" {
  compartment_id           = var.compartment_id
  operating_system         = "Canonical Ubuntu"
  operating_system_version = "24.04"
  shape                    = "VM.Standard.E4.Flex"
  sort_by                  = "TIMECREATED"
  sort_order               = "DESC"
}

resource "oci_core_vcn" "env" {
  compartment_id = var.compartment_id
  display_name   = local.name
  cidr_blocks    = ["10.42.0.0/16"]
}

resource "oci_core_internet_gateway" "env" {
  compartment_id = var.compartment_id
  vcn_id         = oci_core_vcn.env.id
  display_name   = local.name
}

resource "oci_core_route_table" "env" {
  compartment_id = var.compartment_id
  vcn_id         = oci_core_vcn.env.id
  route_rules {
    destination       = "0.0.0.0/0"
    network_entity_id = oci_core_internet_gateway.env.id
  }
}

resource "oci_core_security_list" "env" {
  compartment_id = var.compartment_id
  vcn_id         = oci_core_vcn.env.id
  display_name   = local.name
  egress_security_rules {
    destination = "0.0.0.0/0"
    protocol    = "all"
  }
  dynamic "ingress_security_rules" {
    for_each = concat([22], [__PORTS_LIST__])
    content {
      source   = var.allowed_cidr
      protocol = "6"
      tcp_options {
        min = ingress_security_rules.value
        max = ingress_security_rules.value
      }
    }
  }
}

resource "oci_core_subnet" "env" {
  compartment_id    = var.compartment_id
  vcn_id            = oci_core_vcn.env.id
  cidr_block        = "10.42.1.0/24"
  display_name      = local.name
  route_table_id    = oci_core_route_table.env.id
  security_list_ids = [oci_core_security_list.env.id]
}

resource "oci_core_instance" "env" {
  compartment_id      = var.compartment_id
  availability_domain = data.oci_identity_availability_domains.ads.availability_domains[0].name
  display_name        = local.name
  shape               = "VM.Standard.E4.Flex"
  shape_config {
    ocpus         = __OCPUS__
    memory_in_gbs = __OMEM__
  }
  source_details {
    source_type = "image"
    source_id   = data.oci_core_images.ubuntu.images[0].id
  }
  create_vnic_details {
    subnet_id        = oci_core_subnet.env.id
    assign_public_ip = true
  }
  metadata = {
    ssh_authorized_keys = var.ssh_public_key
  }
}

"#;

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
