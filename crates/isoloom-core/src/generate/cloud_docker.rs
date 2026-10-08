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
use crate::model::{Spec, Target};

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

/// The CPUs every machine needs at once: at least 2, at most 16.
pub(super) fn cpus(spec: &Spec) -> u32 {
    spec.machines
        .values()
        .filter(|m| m.docker.is_some())
        .map(|m| m.resources.and_then(|r| r.cpus).unwrap_or(1))
        .sum::<u32>()
        .clamp(2, 16)
}

/// The memory every machine needs at once, plus Docker's own, in MB.
pub(super) fn memory_mb(spec: &Spec) -> u32 {
    spec.machines
        .values()
        .filter(|m| m.docker.is_some())
        .map(|m| m.resources.and_then(|r| r.memory_mb).unwrap_or(512))
        .sum::<u32>()
        + 1024
}

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    if let Some(name) = super::arm64_machine(spec) {
        return Err(GenerateError::Unsupported {
            target: Target::CloudDocker,
            what: format!("machine `{name}`: arm64 on a cloud VM comes later (Graviton instances and arm64 images)"),
        });
    }
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
const ALLOWED_VAR: &str = r#"variable "allowed_cidr" {
  type        = string
  description = "Who may reach the VM (SSH and the published ports), e.g. your IP/32"
}
"#;

/// The SSH and auto-stop variables every output shares.
const COMMON_VARS: &str = r#"variable "ssh_public_key" {
  type = string
}
variable "ssh_private_key_file" {
  type        = string
  description = "The private key of ssh_public_key: Terraform copies the project over SSH"
}
variable "auto_stop_minutes" {
  type        = number
  default     = 0
  description = "Shut the VM down after this many minutes (0: never). Destroy still ends the billing of disks and addresses"
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
    inline = [
      "cloud-init status --wait >/dev/null 2>&1 || true",
      var.auto_stop_minutes > 0 ? "sudo shutdown -h +${var.auto_stop_minutes} >/dev/null 2>&1" : "true",
      "sudo mkdir -p /opt/isoloom && sudo chown __USER__ /opt/isoloom",
    ]
  }
  # The project as an archive: a plain copy drops the executable bits (entrypoint scripts).
  provisioner "local-exec" {
    command = "tar -czf \"${path.module}/.isoloom-project.tgz\" --exclude=.git --exclude=.vagrant --exclude=.terraform --exclude=.isoloom-project.tgz -C \"${local.root}\" ."
  }
  provisioner "file" {
    source      = "${path.module}/.isoloom-project.tgz"
    destination = "/tmp/isoloom-project.tgz"
  }
__INPUTS_FILE__  provisioner "remote-exec" {
    inline = [
      "set -e",
      "tar -xzf /tmp/isoloom-project.tgz -C /opt/isoloom && rm -f /tmp/isoloom-project.tgz",
      "command -v docker >/dev/null || curl -fsSL https://get.docker.com | sudo sh",
      "cd /opt/isoloom && __SOURCE_INPUTS____START__",
      "sudo mkdir -p /var/lib/isoloom && echo ready | sudo tee /var/lib/isoloom/ready >/dev/null",
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
    other_in(spec, "cloud-docker", cloud, template)
}

/// A provider's Terraform, under `.isoloom/<dir>/<cloud>/main.tf`.
pub(super) fn other_in(spec: &Spec, dir: &str, cloud: &str, template: &str) -> GeneratedFile {
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
            size("Standard_D2als_v6", "Standard_D2as_v6", "Standard_D4as_v6"),
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
        "proxmox" => (String::new(), "isoloom", "local.ip", "proxmox_virtual_environment_vm.env.id"),
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
        "# Start:  terraform -chdir=.isoloom/{dir}/{cloud} init && terraform -chdir=.isoloom/{dir}/{cloud} apply \\\n#           {allowed}-var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/{dir}/{cloud} destroy (same variables)\n\n",
        allowed = if cloud == "proxmox" { "-var proxmox_endpoint=https://<server>:8006/ -var proxmox_api_token=… " } else { "-var allowed_cidr=<your IP>/32 " }
    ));
    tf.push_str(
        &template
            .replace("__NAME__", &spec.name)
            .replace("__SIZE__", &sizes)
            .replace("__PORTS_LIST__", &ports_list)
            .replace("__PORTS_QUOTED__", &ports_quoted)
            .replace("__OCPUS__", &ocpus_mem.0.to_string())
            .replace("__OMEM__", &ocpus_mem.1.to_string())
            .replace("__CPUS__", &cpus(spec).to_string())
            .replace("__MEM_MB__", &mem.to_string()),
    );
    if cloud != "proxmox" {
        tf.push_str(ALLOWED_VAR);
    }
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
            .replace("__SOURCE_INPUTS__", source_inputs)
            .replace("__START__", &start(spec)),
    );
    GeneratedFile {
        path: format!("{OUTPUT_DIR}/{dir}/{cloud}/main.tf"),
        contents: tf,
    }
}

/// The published ports as an Azure security rule, a GCP firewall allow, etc. (an empty list
/// when nothing is published: the templates then open SSH only).
const AZURE: &str = r#"terraform {
  required_version = ">= 1.6"
  backend "local" {}
  required_providers {
    azurerm = {
      source  = "hashicorp/azurerm"
      version = "~> 5.0"
    }
  }
}

variable "subscription_id" {
  type        = string
  default     = null
  description = "Default: ARM_SUBSCRIPTION_ID from the environment"
}
variable "region" {
  type    = string
  default = "swedencentral"
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
  location = var.region
  tags     = local.tags
}

resource "azurerm_virtual_network" "env" {
  name                = local.name
  resource_group_name = azurerm_resource_group.env.name
  location            = var.region
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
  location            = var.region
  allocation_method   = "Static"
  sku                 = "Standard"
}

resource "azurerm_network_security_group" "env" {
  name                = local.name
  resource_group_name = azurerm_resource_group.env.name
  location            = var.region
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
  location            = var.region
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
  location              = var.region
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
  backend "local" {}
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 8.0"
    }
  }
}

variable "project" {
  type        = string
  default     = ""
  description = "An existing project; or leave empty and give billing_account for a project of its own"
}
variable "billing_account" {
  type        = string
  default     = ""
  description = "With no project: create one for this environment, billed here, deleted with it"
}
variable "org_id" {
  type    = string
  default = ""
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
  region = var.region
}

# A project of its own when none is given: everything goes when the environment is destroyed.
resource "google_project" "env" {
  count               = var.project == "" ? 1 : 0
  name                = "isoloom-__NAME__"
  project_id          = "isoloom-${terraform_data.id.output}"
  billing_account     = var.billing_account
  org_id              = var.org_id == "" ? null : var.org_id
  deletion_policy     = "DELETE"
  auto_create_network = false
}

resource "google_project_service" "compute" {
  count              = var.project == "" ? 1 : 0
  project            = google_project.env[0].project_id
  service            = "compute.googleapis.com"
  disable_on_destroy = false
}

resource "terraform_data" "id" {
  input = substr(replace(uuid(), "-", ""), 0, 8)
  lifecycle {
    ignore_changes = [input]
  }
}

locals {
  name    = "isoloom-__NAME__-${terraform_data.id.output}"
  root    = abspath("${path.module}/../../..")
  project = var.project != "" ? var.project : google_project_service.compute[0].project
}

resource "google_compute_network" "env" {
  project                 = local.project
  name                    = local.name
  auto_create_subnetworks = false
}

resource "google_compute_subnetwork" "env" {
  project       = local.project
  name          = local.name
  network       = google_compute_network.env.id
  ip_cidr_range = "10.42.1.0/24"
  region        = var.region
}

resource "google_compute_firewall" "env" {
  project       = local.project
  name          = local.name
  network       = google_compute_network.env.id
  source_ranges = [var.allowed_cidr]
  allow {
    protocol = "tcp"
    ports    = concat(["22"], [__PORTS_QUOTED__])
  }
}

resource "google_compute_instance" "env" {
  project      = local.project
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
  backend "local" {}
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
  backend "local" {}
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
    // Free Tier eligible sizes where they fit (the AWS Free plan refuses other types).
    let instance = super::cloud_vm::instance_type(mem);
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
  backend "local" {{}}
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
    let start = start(spec);
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
  # The project as an archive: a plain copy drops the executable bits (entrypoint scripts).
  provisioner "local-exec" {{
    command = "tar -czf \"${{path.module}}/.isoloom-project.tgz\" --exclude=.git --exclude=.vagrant --exclude=.terraform --exclude=.isoloom-project.tgz -C \"${{local.root}}\" ."
  }}
  provisioner "file" {{
    source      = "${{path.module}}/.isoloom-project.tgz"
    destination = "/tmp/isoloom-project.tgz"
  }}
{inputs_file}  provisioner "remote-exec" {{
    inline = [
      "set -e",
      "tar -xzf /tmp/isoloom-project.tgz -C /opt/isoloom && rm -f /tmp/isoloom-project.tgz",
      "command -v docker >/dev/null || curl -fsSL https://get.docker.com | sudo sh",
      "cd /opt/isoloom && {source_inputs}{start}",
      "sudo mkdir -p /var/lib/isoloom && echo ready | sudo tee /var/lib/isoloom/ready >/dev/null",
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

/// Docker on one VM on a Proxmox server: the VM on the uplink bridge (DHCP), its address from
/// the guest agent, then the same SSH steps as the clouds.
pub(super) const PROXMOX: &str = r##"terraform {
  required_version = ">= 1.6"
  backend "local" {}
  required_providers {
    proxmox = {
      source  = "bpg/proxmox"
      version = "~> 0.84"
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
  description = "Uploading the cloud-init snippet goes over SSH to the node"
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
  description = "Where the VM's disk goes"
}
variable "image_datastore" {
  type        = string
  default     = "local"
  description = "A datastore with 'iso' content, for the cloud image"
}
variable "snippets_datastore" {
  type        = string
  default     = "local"
  description = "A datastore with 'snippets' content, for cloud-init"
}
variable "uplink_bridge" {
  type        = string
  default     = "vmbr0"
  description = "The bridge the VM gets its address (DHCP) and the internet from"
}
variable "slot" {
  type        = number
  default     = 1
  description = "1 to 99, unique per environment on this server"
}

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
  root = abspath("${path.module}/../../..")
  # The VM's address on the uplink, from the guest agent (not the loopback).
  ip = [for a in flatten(proxmox_virtual_environment_vm.env.ipv4_addresses) : a if a != "127.0.0.1"][0]
}

resource "proxmox_download_file" "debian" {
  node_name           = var.node
  datastore_id        = var.image_datastore
  content_type        = "iso"
  url                 = "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2"
  file_name           = "iso${var.slot}-docker-debian-12.img"
  overwrite_unmanaged = true
}

resource "proxmox_virtual_environment_file" "env" {
  node_name    = var.node
  datastore_id = var.snippets_datastore
  content_type = "snippets"
  source_raw {
    file_name = "iso${var.slot}-docker.yaml"
    data = "#cloud-config\n${yamlencode({
      hostname = "__NAME__"
      users = [{
        name                = "isoloom"
        sudo                = "ALL=(ALL) NOPASSWD:ALL"
        shell               = "/bin/bash"
        ssh_authorized_keys = [var.ssh_public_key]
      }]
      packages = ["qemu-guest-agent", "curl"]
      runcmd   = [["systemctl", "enable", "--now", "qemu-guest-agent"]]
    })}"
  }
}

resource "proxmox_virtual_environment_vm" "env" {
  name      = "iso${var.slot}-__NAME__"
  node_name = var.node
  tags      = ["isoloom", "__NAME__"]
  on_boot   = false
  agent {
    enabled = true
  }
  cpu {
    cores = __CPUS__
    type  = "host"
  }
  memory {
    dedicated = __MEM_MB__
  }
  disk {
    datastore_id = var.datastore
    file_id      = proxmox_download_file.debian.id
    interface    = "virtio0"
    size         = 30
  }
  network_device {
    bridge = var.uplink_bridge
  }
  initialization {
    datastore_id      = var.datastore
    user_data_file_id = proxmox_virtual_environment_file.env.id
    ip_config {
      ipv4 {
        address = "dhcp"
      }
    }
  }
  operating_system {
    type = "l26"
  }
  serial_device {}
}

"##;

/// Starts the environment on the VM (see [`super::docker::start_commands`]).
fn start(spec: &Spec) -> String {
    super::docker::start_commands(
        "sudo -E env ISOLOOM_PUBLISH_ADDRESS=0.0.0.0 ISOLOOM_PUBLISH_FIXED=1 docker compose -f .isoloom/docker/compose.yml",
        &super::docker::leaf_jobs(spec),
        Some(900),
    )
}
