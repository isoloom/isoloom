//! The Oracle Cloud (OCI) driver of the cloud-vm target: one OCI compute instance per machine
//! on one VCN. OCI can pin a VNIC's private IP, so every machine keeps its spec address and the
//! baked /etc/hosts holds. Single-network Linux labs only for now (see `refusal`): no secondary
//! VNICs, no Windows, no Ansible controller.
//!
//! OCI images are region- and tenancy-specific OCIDs, so the image isn't looked up: the user
//! gives one `image_ocid`; the SSH login still follows the OS (opc for Oracle Linux, ubuntu for
//! Ubuntu, admin for Debian).

use std::fmt::Write;

use crate::generate::cloud_vm::{aligned, has_windows, hcl_cmd, linux_setup_cmds, needs_controller, redirects};
use crate::generate::proxmox::res;
use crate::generate::{GeneratedFile, OUTPUT_DIR, address, header, start_order};
use crate::model::Spec;

const DIR: &str = "cloud-vm";
const CLOUD: &str = "oci";

/// Why OCI can't take a spec yet. Single-network Linux labs only (best-effort).
pub(super) fn refusal(spec: &Spec) -> Option<String> {
    if spec.networks.len() != 1 {
        return Some("Oracle Cloud cloud-vm takes single-network labs so far".into());
    }
    if has_windows(spec) {
        return Some("Oracle Cloud cloud-vm is Linux only so far".into());
    }
    if needs_controller(spec) {
        return Some("Oracle Cloud cloud-vm has no Ansible controller yet".into());
    }
    None
}

/// The SSH login OCI images ship with, by OS family.
fn ssh_user(os: &str) -> &'static str {
    if os.starts_with("ubuntu") {
        "ubuntu"
    } else if os.starts_with("debian") {
        "admin"
    } else {
        "opc"
    }
}

/// VM.Standard.E4.Flex sizing for a machine's memory: whole GB (at least 1), one OCPU per 8 GB.
fn oci_shape(memory_mb: u32) -> (u32, u32) {
    let gb = memory_mb.div_ceil(1024).max(1);
    ((gb / 8).max(1), gb)
}

pub(super) fn build(spec: &Spec) -> GeneratedFile {
    // Single-network Linux, guaranteed by `refusal`.
    let net = spec.networks.keys().next().expect("validated: one network");
    let net_cidr = &spec.networks[net.as_str()].cidr;
    let lab = format!("{{ {net_cidr} }}");

    let mut tf = header("#");
    tf.push_str(
        "# Start:  terraform -chdir=.isoloom/cloud-vm/oci init && terraform -chdir=.isoloom/cloud-vm/oci apply \\\n#           -var compartment_ocid=<compartment ocid> -var availability_domain=<AD name> -var image_ocid=<image ocid> \\\n#           -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-vm/oci destroy (same variables)\n\n",
    );
    tf.push_str(
        r#"terraform {
  required_version = ">= 1.6"
  backend "local" {}
  required_providers {
    oci = {
      source  = "oracle/oci"
      version = "~> 6.0"
    }
  }
}

variable "compartment_ocid" {
  type        = string
  description = "The compartment the environment goes in"
}
variable "availability_domain" {
  type        = string
  description = "An availability domain's name, e.g. the output of `oci iam availability-domain list`"
}
variable "image_ocid" {
  type        = string
  description = "The OS image's OCID (region- and tenancy-specific): Linux the login follows (opc/ubuntu/admin)"
}
variable "allowed_cidr" {
  type        = string
  description = "Who may reach the machines (SSH and the published ports), e.g. your IP/32"
}
variable "ssh_public_key" {
  type = string
}
variable "ssh_private_key_file" {
  type        = string
  description = "The private key of ssh_public_key: Terraform sets the machines up over SSH"
}
variable "auto_stop_minutes" {
  type        = number
  default     = 0
  description = "Shut the machines down after this many minutes (0: never)"
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
        r#"
provider "oci" {{}}

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

# The environment's one network: a VCN with the spec's range, routed to the internet.
resource "oci_core_vcn" "env" {{
  compartment_id = var.compartment_ocid
  cidr_block     = "{net_cidr}"
  display_name   = local.name
}}

resource "oci_core_internet_gateway" "env" {{
  compartment_id = var.compartment_ocid
  vcn_id         = oci_core_vcn.env.id
  display_name   = local.name
}}

resource "oci_core_route_table" "env" {{
  compartment_id = var.compartment_ocid
  vcn_id         = oci_core_vcn.env.id
  display_name   = local.name
  route_rules {{
    destination       = "0.0.0.0/0"
    network_entity_id = oci_core_internet_gateway.env.id
  }}
}}

# What may reach the machines: everything inside the VCN, SSH and the published ports from
# allowed_cidr; egress open.
resource "oci_core_security_list" "env" {{
  compartment_id = var.compartment_ocid
  vcn_id         = oci_core_vcn.env.id
  display_name   = local.name
  egress_security_rules {{
    destination = "0.0.0.0/0"
    protocol    = "all"
  }}
  ingress_security_rules {{
    description = "the VCN itself"
    source      = "{net_cidr}"
    protocol    = "all"
  }}
  ingress_security_rules {{
    description = "SSH from allowed_cidr"
    source      = var.allowed_cidr
    protocol    = "6"
    tcp_options {{
      min = 22
      max = 22
    }}
  }}
"#,
        env = spec.name,
    );
    let mut published_ports: Vec<u16> = spec.machines.values().flat_map(|m| m.services.iter().filter_map(|s| s.publish)).collect();
    published_ports.sort_unstable();
    published_ports.dedup();
    for p in &published_ports {
        let _ = write!(
            tf,
            "  ingress_security_rules {{\n    description = \"published\"\n    source      = var.allowed_cidr\n    protocol    = \"6\"\n    tcp_options {{\n      min = {p}\n      max = {p}\n    }}\n  }}\n"
        );
    }
    let _ = write!(
        tf,
        r#"}}

resource "oci_core_subnet" "env" {{
  compartment_id    = var.compartment_ocid
  vcn_id            = oci_core_vcn.env.id
  cidr_block        = "{net_cidr}"
  display_name      = local.name
  route_table_id    = oci_core_route_table.env.id
  security_list_ids = [oci_core_security_list.env.id]
}}
"#,
    );

    let mut access_ip: Option<String> = None;
    let mut public_ips = Vec::new();
    let mut ssh_users = Vec::new();
    let mut published_out = Vec::new();
    for name in start_order(spec) {
        let m = &spec.machines[name];
        let Some(vm) = &m.vm else { continue };
        let user = ssh_user(&vm.os);
        let (net, octet) = m.networks.first().expect("validated: every machine is on a network");
        let id = res(name);
        let addr = address(spec, net, *octet);
        let pip = format!("oci_core_instance.{id}.public_ip");
        let mem = m.resources.and_then(|r| r.memory_mb).unwrap_or(crate::DEFAULT_MEMORY_MB);
        let disk = m.resources.and_then(|r| r.disk_gb).unwrap_or(crate::DEFAULT_DISK_GB);
        let (ocpus, mem_gb) = oci_shape(mem);
        // OCI boot volumes start at 50 GB.
        let boot = disk.max(50);

        for sv in &m.services {
            if let Some(h) = sv.publish {
                let label = sv.name.clone().unwrap_or_else(|| sv.port.to_string());
                published_out.push((format!("\"{name}/{label}\""), format!("\"${{{pip}}}:{h}\"")));
            }
        }

        let _ = write!(
            tf,
            r##"
# Machine `{name}`, at its fixed address {addr}.
resource "oci_core_instance" "{id}" {{
  compartment_id      = var.compartment_ocid
  availability_domain = var.availability_domain
  display_name        = "${{local.name}}-{name}"
  shape               = "VM.Standard.E4.Flex"
  shape_config {{
    ocpus         = {ocpus}
    memory_in_gbs = {mem_gb}
  }}
  source_details {{
    source_type             = "image"
    source_id               = var.image_ocid
    boot_volume_size_in_gbs = {boot}
  }}
  create_vnic_details {{
    subnet_id        = oci_core_subnet.env.id
    private_ip       = "{addr}"
    assign_public_ip = true
    hostname_label   = "{name}"
  }}
  metadata = {{
    ssh_authorized_keys = var.ssh_public_key
    user_data           = base64encode(var.auto_stop_minutes > 0 ? "#!/bin/sh\nshutdown -h +${{var.auto_stop_minutes}}\n" : "#!/bin/sh\n")
  }}
}}
"##,
        );

        // Its set-up over SSH, shared with the other drivers. Single network: no secondary VNIC,
        // so the MAC closure is never called.
        let redir = redirects(m);
        let cmds = linux_setup_cmds(spec, name, m, vm, &lab, &redir, &|_| String::new());

        let mut prov = format!(
            "\nresource \"terraform_data\" \"{id}\" {{\n  triggers_replace = [oci_core_instance.{id}.id]\n  connection {{\n    type        = \"ssh\"\n    host        = {pip}\n    user        = \"{user}\"\n    private_key = file(pathexpand(var.ssh_private_key_file))\n    timeout     = \"10m\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\"cloud-init status --wait >/dev/null 2>&1 || true\", \"sudo mkdir -p /opt/isoloom && sudo chown {user} /opt/isoloom\"]\n  }}\n  provisioner \"local-exec\" {{\n    command = \"tar -czf \\\"${{path.module}}/.isoloom-project-{id}.tgz\\\" --exclude=.git --exclude=.vagrant --exclude=.terraform --exclude=.isoloom-project*.tgz -C \\\"${{local.root}}\\\" .\"\n  }}\n  provisioner \"file\" {{\n    source      = \"${{path.module}}/.isoloom-project-{id}.tgz\"\n    destination = \"/tmp/isoloom-project.tgz\"\n  }}\n"
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

        public_ips.push((name.to_string(), pip.clone()));
        ssh_users.push((name.to_string(), format!("\"{user}\"")));
        if m.access || access_ip.is_none() {
            access_ip = Some(pip.clone());
        }
    }

    // Outputs, the same shape the AWS driver gives: every machine's address, one to start from
    // (the access machine, else the first), its SSH user, the ready marker.
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
            "\n# The checks, from where a user stands: ssh <user>@<host> each command.\noutput \"checks\" {{\n  value = {{\n    host = {first}\n    user = null\n    commands = [\n{}\n    ]\n  }}\n}}\n",
            runs.join(",\n"),
        );
    }
    if !published_out.is_empty() {
        let _ = write!(tf, "\noutput \"published\" {{\n  value = {{\n{}\n  }}\n}}\n", aligned(&published_out));
    }

    GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/{CLOUD}/main.tf"),
        contents: tf,
    }
}
