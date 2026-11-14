//! What each target writes, and the official validators that check those files: Isoloom is a
//! converter, so a target is presented by its output and how that output is checked, never by
//! runs of it.
//!
//! The validators are the ones `cargo xtask verify` runs on every example's generated files
//! (inside its pinned toolchain image), with `terraform test` from `cargo xtask tf-test` for the
//! targets whose modules have a suite (`tests/terraform/`).

use isoloom_spec::model::Target;
use serde_json::{Value, json};

/// A target's output and its validators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Validated {
    /// The target.
    pub target: Target,
    /// What it writes, in a few words.
    pub writes: &'static str,
    /// The validators that check what it writes, as their commands are named.
    pub validators: &'static [&'static str],
}

impl Validated {
    /// The entry as JSON (`isoloom targets --json`, `isoloom matrix --json`): the target, what it
    /// writes and its validators.
    #[must_use]
    pub fn json(&self) -> Value {
        json!({ "target": self.target.id(), "writes": self.writes, "validators": self.validators })
    }
}

/// A Compose file's validator.
const COMPOSE: &str = "docker compose config";

/// A Terraform module's validators.
const TERRAFORM: [&str; 3] = ["terraform validate", "tflint", "terraform test"];

/// What `target` writes and its validators.
#[must_use]
pub fn of(target: Target) -> Validated {
    let (writes, validators): (&'static str, &'static [&'static str]) = match target {
        Target::Docker => ("a Compose file (`compose.yml`) and the files it mounts", &[COMPOSE]),
        Target::Hosted => ("the `docker` target's Compose file, for a hosting service", &[COMPOSE]),
        Target::CloudDocker => (
            "the `docker` target's Compose file, and a Terraform module per cloud for its Docker host",
            &[COMPOSE, TERRAFORM[0], TERRAFORM[1], TERRAFORM[2]],
        ),
        Target::Kubernetes => ("Kubernetes manifests with a Kustomize overlay, and a Helm chart", &["kubeconform", "helm lint"]),
        Target::Vagrant => ("a Vagrantfile, every provider's block in it", &["vagrant validate"]),
        Target::Proxmox => ("a Terraform module for Proxmox VE", &TERRAFORM),
        Target::Esxi => ("a Terraform module for a standalone ESXi host", &TERRAFORM),
        Target::CloudVm => ("a Terraform module per cloud", &TERRAFORM),
        Target::External => ("an Ansible inventory and playbook for hosts that exist", &["ansible-lint"]),
        Target::CloudServices => (
            "the fixed variables (`terraform.tfvars.json`) of the environment's own Terraform module",
            &["terraform validate", "terraform test"],
        ),
    };
    Validated { target, writes, validators }
}

/// Every target's, in the order targets are listed.
#[must_use]
pub fn all() -> Vec<Validated> {
    Target::ALL.into_iter().map(of).collect()
}

/// The name of a target, or of one of its providers or clouds: the target's id, then `/` and
/// the provider or cloud (`vagrant/libvirt`). What matrix columns are called.
pub(crate) fn name(target: Target, variant: Option<&str>) -> String {
    match variant {
        Some(v) => format!("{}/{v}", target.id()),
        None => target.id().to_owned(),
    }
}

/// The Vagrant providers the Vagrantfile has a block for, as `vagrant up --provider` names them.
pub(crate) const VAGRANT_PROVIDERS: [&str; 8] = ["virtualbox", "vmware_desktop", "hyperv", "parallels", "libvirt", "qemu", "utm", "vmware_esxi"];

/// The clouds `cloud-services` reads (`cloud.provider`).
const CLOUD_SERVICES: [&str; 3] = ["aws", "azure", "gcp"];

/// The providers or clouds `target` has, each its own column of the matrix; empty for a target
/// that has none.
#[must_use]
pub fn variants(target: Target) -> Vec<&'static str> {
    match target {
        Target::Vagrant => VAGRANT_PROVIDERS.to_vec(),
        Target::CloudVm | Target::CloudDocker => isoloom_terraform::Cloud::ALL.iter().map(|c| c.id()).collect(),
        Target::CloudServices => CLOUD_SERVICES.to_vec(),
        Target::Docker | Target::Hosted | Target::Kubernetes | Target::Proxmox | Target::Esxi | Target::External => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_names_what_it_writes_and_at_least_one_validator() {
        let all = all();
        assert_eq!(all.len(), Target::ALL.len());
        for v in &all {
            assert!(!v.writes.is_empty() && !v.validators.is_empty(), "{v:?}");
        }
        assert_eq!(of(Target::Kubernetes).validators, ["kubeconform", "helm lint"]);
        assert_eq!(of(Target::CloudVm).validators, TERRAFORM);
        assert_eq!(of(Target::Vagrant).json()["validators"], json!(["vagrant validate"]));
    }

    #[test]
    fn the_terraform_test_suites_are_the_targets_said_to_have_them() {
        let suites = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/terraform");
        for t in Target::ALL {
            let said = of(t).validators.contains(&"terraform test");
            assert_eq!(suites.join(t.id()).is_dir(), said, "{}", t.id());
        }
    }
}
