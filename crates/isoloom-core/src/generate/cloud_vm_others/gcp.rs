//! The Google Cloud (`google` provider) driver for `cloud-vm`: a faithful port of the AWS driver
//! in `cloud_vm.rs`. One VPC network holds every spec network as a subnetwork with its exact
//! range; each machine is a `google_compute_instance` at its address with a public IP, reached
//! over SSH to set up like the AWS instances; firewall rules do what the AWS security groups do.
//!
//! Two things GCP can't do the AWS way yet, so `refusal` declines them:
//! - Multiple NICs: GCP requires each network interface on a distinct VPC network, which the
//!   single-VPC port can't give. So a machine on more than one network (and, with more than one
//!   network, the controller, which spans them all) comes later.
//! - Windows: the sysprep/WinRM dance differs enough that it comes later.
//!
//! The SSH user is injected through the `ssh-keys` metadata, so it is created on first boot. We
//! inject the same per-OS user the AWS driver uses (admin/ubuntu), which lets the controller's
//! inventory (shared with AWS) stay unchanged.

use std::fmt::Write;

use super::super::cloud_vm::{CONTROLLER_OS, aligned, cidr, hcl_cmd, inventory, linux_setup_cmds, needs_controller, remote_exec, sh_quote};
use super::super::proxmox::res;
use super::super::{GeneratedFile, OUTPUT_DIR, address, header, start_order, vagrant};
use crate::model::Spec;

const DIR: &str = "cloud-vm";

/// The image family and SSH user for an OS. The user is created from the `ssh-keys` metadata, so
/// it matches the AWS driver's user and the shared inventory needs no change.
fn gcp_image(os: &str) -> Option<(&'static str, &'static str)> {
    Some(match os {
        "debian-12" => ("debian-cloud/debian-12", "admin"),
        "debian-13" => ("debian-cloud/debian-13", "admin"),
        "ubuntu-22.04" => ("ubuntu-os-cloud/ubuntu-2204-lts", "ubuntu"),
        "ubuntu-24.04" => ("ubuntu-os-cloud/ubuntu-2404-lts-amd64", "ubuntu"),
        _ => return None,
    })
}

/// The machine type for a memory size: E2 shared-core up to 4 GB, then standard.
fn gcp_type(memory_mb: u32) -> &'static str {
    match memory_mb {
        0..=1024 => "e2-small",
        1025..=2048 => "e2-medium",
        2049..=8192 => "e2-standard-2",
        _ => "e2-standard-4",
    }
}

/// What GCP can't produce yet, on top of the global gate in `cloud_vm::unsupported`.
pub(super) fn refusal(spec: &Spec) -> Option<String> {
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        if crate::images::is_windows(&vm.os) {
            return Some(format!("machine `{name}`: Windows on GCP comes later"));
        }
        if m.networks.len() > 1 {
            return Some(format!(
                "machine `{name}`: GCP multi-NIC comes later (each interface needs its own VPC network)"
            ));
        }
        if gcp_image(&vm.os).is_none() {
            return Some(format!("machine `{name}`: no GCP image for `{}` yet (Debian 12/13, Ubuntu 22.04/24.04)", vm.os));
        }
    }
    // The controller sits on every network; with more than one it is itself multi-NIC.
    if needs_controller(spec) && spec.networks.len() > 1 {
        return Some("the Ansible controller spans several networks: GCP multi-NIC comes later".into());
    }
    None
}

pub(super) fn build(spec: &Spec) -> GeneratedFile {
    let nets: Vec<&String> = spec.networks.keys().collect();
    let lab = format!(
        "{{ {} }}",
        nets.iter().map(|n| spec.networks[n.as_str()].cidr.clone()).collect::<Vec<_>>().join(", ")
    );

    let mut tf = header("#");
    tf.push_str(
        "# Start:  terraform -chdir=.isoloom/cloud-vm/gcp init && terraform -chdir=.isoloom/cloud-vm/gcp apply \\\n#           -var billing_account=<id> -var allowed_cidr=<your IP>/32 -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\" -var ssh_private_key_file=~/.ssh/id_ed25519\n# Stop:   terraform -chdir=.isoloom/cloud-vm/gcp destroy (same variables)\n\n",
    );
    let _ = write!(
        tf,
        r#"terraform {{
  required_version = ">= 1.6"
  backend "local" {{}}
  required_providers {{
    google = {{
      source  = "hashicorp/google"
      version = "~> 8.0"
    }}{tls}
  }}
}}

variable "project" {{
  type        = string
  default     = ""
  description = "An existing project; or leave empty and give billing_account for a project of its own"
}}
variable "billing_account" {{
  type        = string
  default     = ""
  description = "With no project: create one for this environment, billed here, deleted with it"
}}
variable "org_id" {{
  type    = string
  default = ""
}}
variable "region" {{
  type    = string
  default = "europe-west9"
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
        tls = if needs_controller(spec) {
            "\n    tls = {\n      source  = \"hashicorp/tls\"\n      version = \"~> 4.0\"\n    }"
        } else {
            ""
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
provider "google" {{
  region = var.region
}}

# A project of its own when none is given: everything goes when the environment is destroyed.
resource "google_project" "env" {{
  count               = var.project == "" ? 1 : 0
  name                = "{project_name}"
  project_id          = "isoloom-${{terraform_data.id.output}}"
  billing_account     = var.billing_account
  org_id              = var.org_id == "" ? null : var.org_id
  deletion_policy     = "DELETE"
  auto_create_network = false
}}

resource "google_project_service" "compute" {{
  count              = var.project == "" ? 1 : 0
  project            = google_project.env[0].project_id
  service            = "compute.googleapis.com"
  disable_on_destroy = false
}}

resource "terraform_data" "id" {{
  input = substr(replace(uuid(), "-", ""), 0, 8)
  lifecycle {{
    ignore_changes = [input]
  }}
}}

locals {{
  name    = "isoloom-{env}-${{terraform_data.id.output}}"
  root    = abspath("${{path.module}}/../../..")
  project = var.project != "" ? var.project : google_project_service.compute[0].project
  zone    = "${{var.region}}-a"
}}

# The environment's networks: one VPC, a subnetwork per network with its exact range.
resource "google_compute_network" "env" {{
  project                 = local.project
  name                    = local.name
  auto_create_subnetworks = false
}}
"#,
        env = spec.name,
        project_name = crate::generate::gcp_project_name(&spec.name),
    );
    for net in &nets {
        let _ = writeln!(
            tf,
            "\nresource \"google_compute_subnetwork\" \"{id}\" {{\n  project       = local.project\n  name          = \"${{local.name}}-{net}\"\n  network       = google_compute_network.env.id\n  ip_cidr_range = \"{c}\"\n  region        = var.region\n}}",
            id = res(net),
            c = spec.networks[net.as_str()].cidr,
        );
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
        let (image, user) = gcp_image(&vm.os).expect("checked by refusal");
        let (net, octet) = m.networks.first().expect("validated: every machine is on a network");
        let id = res(name);
        let addr = address(spec, net, *octet);
        let pip = format!("google_compute_instance.{id}.network_interface[0].access_config[0].nat_ip");
        let tag = format!("${{local.name}}-{name}");
        let mem = m.resources.and_then(|r| r.memory_mb).unwrap_or(crate::DEFAULT_MEMORY_MB);
        let disk = m.resources.and_then(|r| r.disk_gb).unwrap_or(crate::DEFAULT_DISK_GB);

        // Who may reach it: its own network, the networks `reach` opens to it, SSH and the
        // published ports from allowed_cidr. GCP firewalls are network-scoped and match the
        // instance by its tag; each distinct source gets its own rule (one source set per rule).
        let _ = write!(
            tf,
            "\n# Machine `{name}`: what may reach it.\nresource \"google_compute_firewall\" \"{id}_net\" {{\n  project       = local.project\n  name          = \"${{local.name}}-{name}-net\"\n  network       = google_compute_network.env.id\n  direction     = \"INGRESS\"\n  source_ranges = [\"{c}\"]\n  target_tags   = [\"{tag}\"]\n  allow {{\n    protocol = \"all\"\n  }}\n}}\n\nresource \"google_compute_firewall\" \"{id}_ssh\" {{\n  project       = local.project\n  name          = \"${{local.name}}-{name}-ssh\"\n  network       = google_compute_network.env.id\n  direction     = \"INGRESS\"\n  source_ranges = [var.allowed_cidr]\n  target_tags   = [\"{tag}\"]\n  allow {{\n    protocol = \"tcp\"\n    ports    = [\"22\"]\n  }}\n}}\n",
            c = spec.networks[net.as_str()].cidr,
        );
        for r in spec
            .reach
            .iter()
            .filter(|r| m.networks.contains_key(&r.to) && !m.networks.contains_key(&r.from))
        {
            let from = &spec.networks[&r.from].cidr;
            if r.ports.is_empty() {
                let _ = write!(
                    tf,
                    "\nresource \"google_compute_firewall\" \"{id}_reach_{rf}\" {{\n  project       = local.project\n  name          = \"${{local.name}}-{name}-reach-{f}\"\n  network       = google_compute_network.env.id\n  direction     = \"INGRESS\"\n  source_ranges = [\"{from}\"]\n  target_tags   = [\"{tag}\"]\n  allow {{\n    protocol = \"all\"\n  }}\n}}\n",
                    rf = res(&r.from),
                    f = r.from,
                );
            } else {
                let ports = r.ports.iter().map(|p| format!("\"{p}\"")).collect::<Vec<_>>().join(", ");
                let _ = write!(
                    tf,
                    "\nresource \"google_compute_firewall\" \"{id}_reach_{rf}\" {{\n  project       = local.project\n  name          = \"${{local.name}}-{name}-reach-{f}\"\n  network       = google_compute_network.env.id\n  direction     = \"INGRESS\"\n  source_ranges = [\"{from}\"]\n  target_tags   = [\"{tag}\"]\n  allow {{\n    protocol = \"tcp\"\n    ports    = [{ports}]\n  }}\n  allow {{\n    protocol = \"udp\"\n    ports    = [{ports}]\n  }}\n}}\n",
                    rf = res(&r.from),
                    f = r.from,
                );
            }
        }
        let mut redirects = Vec::new();
        let mut pub_ports = Vec::new();
        for sv in &m.services {
            if let Some(h) = sv.publish {
                pub_ports.push(h);
                if h != sv.port {
                    redirects.push((h, sv.port));
                }
                let label = sv.name.clone().unwrap_or_else(|| sv.port.to_string());
                published_out.push((format!("\"{name}/{label}\""), format!("\"${{{pip}}}:{h}\"")));
            }
        }
        if !pub_ports.is_empty() {
            let ports = pub_ports.iter().map(|p| format!("\"{p}\"")).collect::<Vec<_>>().join(", ");
            let _ = write!(
                tf,
                "\nresource \"google_compute_firewall\" \"{id}_published\" {{\n  project       = local.project\n  name          = \"${{local.name}}-{name}-published\"\n  network       = google_compute_network.env.id\n  direction     = \"INGRESS\"\n  source_ranges = [var.allowed_cidr]\n  target_tags   = [\"{tag}\"]\n  allow {{\n    protocol = \"tcp\"\n    ports    = [{ports}]\n  }}\n}}\n",
            );
        }

        let _ = writeln!(
            tf,
            "\nresource \"google_compute_instance\" \"{id}\" {{\n  project      = local.project\n  name         = \"${{local.name}}-{name}\"\n  machine_type = \"{itype}\"\n  zone         = local.zone\n  tags         = [\"{tag}\"]\n  boot_disk {{\n    initialize_params {{\n      image = \"{image}\"\n      size  = {disk}\n    }}\n  }}\n  network_interface {{\n    subnetwork = google_compute_subnetwork.{netid}.id\n    network_ip = \"{addr}\"\n    access_config {{}}\n  }}\n  metadata = {{\n    ssh-keys = \"{user}:${{var.ssh_public_key}}\"\n  }}\n  metadata_startup_script = var.auto_stop_minutes > 0 ? \"#!/bin/sh\\nshutdown -h +${{var.auto_stop_minutes}}\\n\" : null\n  labels = {{\n    \"isoloom-environment\" = \"{env}\"\n    \"managed-by\"          = \"isoloom\"\n    \"isoloom-instance\"   = local.name\n    \"isoloom-expires-at\" = var.expires_at\n  }}\n}}",
            itype = gcp_type(mem),
            netid = res(net),
            env = spec.name,
        );

        // Its set-up, over SSH, as on every cloud. No interface step: GCP machines here are
        // single-NIC (multi-NIC is refused).
        let cmds = linux_setup_cmds(spec, name, m, vm, &lab, &redirects, &|_| String::new());

        let deps: Vec<String> = m
            .depends_on
            .iter()
            .filter(|d| spec.machines[*d].vm.is_some())
            .map(|d| format!("terraform_data.{}", res(d)))
            .collect();
        let mut prov = format!(
            "\nresource \"terraform_data\" \"{id}\" {{\n  triggers_replace = [google_compute_instance.{id}.id]\n  connection {{\n    type        = \"ssh\"\n    host        = {pip}\n    user        = \"{user}\"\n    private_key = file(pathexpand(var.ssh_private_key_file))\n    timeout     = \"10m\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\"cloud-init status --wait >/dev/null 2>&1 || true\", \"sudo mkdir -p /opt/isoloom && sudo chown {user} /opt/isoloom\"]\n  }}\n  provisioner \"local-exec\" {{\n    command = \"tar -czf \\\"${{path.module}}/.isoloom-project-{id}.tgz\\\" --exclude=.git --exclude=.vagrant --exclude=.terraform --exclude=.isoloom-project*.tgz -C \\\"${{local.root}}\\\" .\"\n  }}\n  provisioner \"file\" {{\n    source      = \"${{path.module}}/.isoloom-project-{id}.tgz\"\n    destination = \"/tmp/isoloom-project.tgz\"\n  }}\n"
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
        controller(spec, &mut tf, &nets);
    }

    // Outputs, the same shape and semantics as the AWS driver's.
    let (first, check_user) = match access_ip {
        Some(ip) => (ip, None),
        None if needs_controller(spec) => (
            "google_compute_instance.isoloom_controller.network_interface[0].access_config[0].nat_ip".to_string(),
            Some("admin"),
        ),
        None => ("null".to_string(), None),
    };
    let _ = write!(
        tf,
        "\noutput \"machines\" {{\n  value = {{\n{}\n  }}\n}}\n\noutput \"ssh_users\" {{\n  value = {{\n{}\n  }}\n}}\n\noutput \"ip\" {{\n  value = {first}\n}}\n\noutput \"ready_file\" {{\n  value = \"/var/lib/isoloom/ready\"\n}}\n",
        aligned(&public_ips),
        aligned(&ssh_users),
    );
    let fallback = needs_controller(spec).then_some((
        "google_compute_instance.isoloom_controller.network_interface[0].access_config[0].nat_ip",
        "admin",
    ));
    let _ = check_user;
    tf.push_str(&super::super::cloud_vm::checks_output(spec, &public_ips, &ssh_users, fallback));
    if !published_out.is_empty() {
        let _ = write!(tf, "\noutput \"published\" {{\n  value = {{\n{}\n  }}\n}}\n", aligned(&published_out));
    }

    GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/gcp/main.tf"),
        contents: tf,
    }
}

/// The controller: a Debian instance on the single network (multi-net controllers are refused) at
/// the controller address, which runs the environment's playbooks once every machine is set up.
fn controller(spec: &Spec, tf: &mut String, nets: &[&String]) {
    let first = nets[0];
    let fid = res(first);
    let (image, user) = gcp_image(CONTROLLER_OS).expect("controller OS has an image");
    let c = cidr(spec, first);
    let _ = write!(
        tf,
        "\n# The controller (Ansible): its network may reach it, and SSH from allowed_cidr.\nresource \"google_compute_firewall\" \"isoloom_controller_net\" {{\n  project       = local.project\n  name          = \"${{local.name}}-controller-net\"\n  network       = google_compute_network.env.id\n  direction     = \"INGRESS\"\n  source_ranges = [\"{cidr}\"]\n  target_tags   = [\"${{local.name}}-controller\"]\n  allow {{\n    protocol = \"all\"\n  }}\n}}\n\nresource \"google_compute_firewall\" \"isoloom_controller_ssh\" {{\n  project       = local.project\n  name          = \"${{local.name}}-controller-ssh\"\n  network       = google_compute_network.env.id\n  direction     = \"INGRESS\"\n  source_ranges = [var.allowed_cidr]\n  target_tags   = [\"${{local.name}}-controller\"]\n  allow {{\n    protocol = \"tcp\"\n    ports    = [\"22\"]\n  }}\n}}\n",
        cidr = spec.networks[first.as_str()].cidr,
    );
    let _ = writeln!(
        tf,
        "\nresource \"google_compute_instance\" \"isoloom_controller\" {{\n  project      = local.project\n  name         = \"${{local.name}}-controller\"\n  machine_type = \"e2-small\"\n  zone         = local.zone\n  tags         = [\"${{local.name}}-controller\"]\n  boot_disk {{\n    initialize_params {{\n      image = \"{image}\"\n    }}\n  }}\n  network_interface {{\n    subnetwork = google_compute_subnetwork.{fid}.id\n    network_ip = \"{addr}\"\n    access_config {{}}\n  }}\n  metadata = {{\n    ssh-keys = \"{user}:${{var.ssh_public_key}}\"\n  }}\n  metadata_startup_script = var.auto_stop_minutes > 0 ? \"#!/bin/sh\\nshutdown -h +${{var.auto_stop_minutes}}\\n\" : null\n}}",
        addr = c.controller(),
    );
    // Its set-up: names, the project, its key, Ansible, the inventory, the playbooks.
    let mut cmds: Vec<String> = vec![
        "set -e".into(),
        "cloud-init status --wait >/dev/null 2>&1 || true".into(),
        "tar -xzf /tmp/isoloom-project.tgz -C /opt/isoloom && rm -f /tmp/isoloom-project.tgz".into(),
    ];
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
    cmds.push(format!("sudo sh -c {}", sh_quote(&vagrant::ansible_runs(spec))));
    cmds.push("sudo mkdir -p /var/lib/isoloom && echo ready | sudo tee /var/lib/isoloom/ready >/dev/null".into());
    let deps: Vec<String> = spec
        .machines
        .iter()
        .filter(|(_, m)| m.vm.is_some())
        .map(|(n, _)| format!("terraform_data.{}", res(n)))
        .collect();
    let _ = writeln!(
        tf,
        "\nresource \"terraform_data\" \"isoloom_controller\" {{\n  triggers_replace = [google_compute_instance.isoloom_controller.id]\n  connection {{\n    type        = \"ssh\"\n    host        = google_compute_instance.isoloom_controller.network_interface[0].access_config[0].nat_ip\n    user        = \"{user}\"\n    private_key = file(pathexpand(var.ssh_private_key_file))\n    timeout     = \"10m\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\"cloud-init status --wait >/dev/null 2>&1 || true\", \"sudo mkdir -p /opt/isoloom && sudo chown {user} /opt/isoloom\"]\n  }}\n  provisioner \"local-exec\" {{\n    command = \"tar -czf \\\"${{path.module}}/.isoloom-project-controller.tgz\\\" --exclude=.git --exclude=.vagrant --exclude=.terraform --exclude=.isoloom-project*.tgz -C \\\"${{local.root}}\\\" .\"\n  }}\n  provisioner \"file\" {{\n    source      = \"${{path.module}}/.isoloom-project-controller.tgz\"\n    destination = \"/tmp/isoloom-project.tgz\"\n  }}\n  provisioner \"file\" {{\n    content     = tls_private_key.controller.private_key_openssh\n    destination = \"/tmp/isoloom-controller-key\"\n  }}\n  provisioner \"remote-exec\" {{\n    inline = [\n{}\n    ]\n  }}\n  depends_on = [{}]\n}}",
        cmds.iter().map(|c| format!("      {}", hcl_cmd(c))).collect::<Vec<_>>().join(",\n"),
        deps.join(", "),
    );
}
