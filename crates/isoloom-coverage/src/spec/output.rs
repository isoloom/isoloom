//! The table's columns: every output Isoloom writes, and the rendering each one shares with
//! others (the Compose file runs on four targets, the Kubernetes backend writes three formats,
//! every cloud has its own module from one cloud VM core).

use isoloom_spec::model::Target;

/// A rendering several outputs share: the tables state what each does with a field once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// The Compose file.
    Compose,
    /// The Kubernetes manifests, Helm chart and Kustomize overlay.
    Kubernetes,
    /// The Vagrantfile.
    Vagrant,
    /// The Proxmox Terraform module.
    Proxmox,
    /// The cloud VM Terraform modules, one per cloud.
    CloudVm,
    /// The existing hosts' inventory and check runners.
    External,
    /// The cloud-services scripts.
    CloudServices,
}

impl Family {
    /// Every family, in the order the tests report them.
    pub const ALL: [Family; 7] = [
        Family::Compose,
        Family::Kubernetes,
        Family::Vagrant,
        Family::Proxmox,
        Family::CloudVm,
        Family::External,
        Family::CloudServices,
    ];

    /// The target whose generator the family's claims are proven on.
    pub fn target(self) -> Target {
        match self {
            Family::Compose => Target::Docker,
            Family::Kubernetes => Target::Kubernetes,
            Family::Vagrant => Target::Vagrant,
            Family::Proxmox => Target::Proxmox,
            Family::CloudVm => Target::CloudVm,
            Family::External => Target::External,
            Family::CloudServices => Target::CloudServices,
        }
    }
}

/// One column of the table: a target's output, or one cloud's module of `cloud-vm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// `docker`: the Compose file on the local Docker.
    Docker,
    /// `hosted`: the Compose file on a Docker host Isoloom doesn't own.
    Hosted,
    /// `kubernetes`: the raw manifests.
    Kubernetes,
    /// `kubernetes`: the Helm chart.
    Helm,
    /// `kubernetes`: the Kustomize overlay.
    Kustomize,
    /// `vagrant`: the Vagrantfile.
    Vagrant,
    /// `docker-vm`: the Compose file in a Vagrant VM.
    DockerVm,
    /// `hybrid`: Compose for the containers, Vagrant for the VMs.
    Hybrid,
    /// `proxmox`: the Proxmox module.
    Proxmox,
    /// `cloud-vm`: one cloud's module (its folder name: `aws`, `azure`...).
    CloudVm(&'static str),
    /// `cloud-docker`: the Compose file on a cloud VM.
    CloudDocker,
    /// `cloud-services`: the scripts applying a Terraform module.
    CloudServices,
    /// `external`: the existing hosts.
    External,
}

impl Output {
    /// Every column, in the table's order: one per cloud for `cloud-vm`, from the clouds the
    /// target writes a module for, so a new cloud gets its column without editing the table.
    pub fn all() -> Vec<Output> {
        let mut all = vec![
            Output::Docker,
            Output::Hosted,
            Output::Kubernetes,
            Output::Helm,
            Output::Kustomize,
            Output::Vagrant,
            Output::DockerVm,
            Output::Hybrid,
            Output::Proxmox,
        ];
        all.extend(isoloom_terraform::cloud_vm::CLOUDS.iter().map(|(cloud, _)| Output::CloudVm(cloud)));
        all.extend([Output::CloudDocker, Output::CloudServices, Output::External]);
        all
    }

    /// The column's heading.
    pub fn label(self) -> &'static str {
        match self {
            Output::CloudVm(cloud) => cloud,
            Output::Helm => "helm",
            Output::Kustomize => "kustomize",
            other => other.target().id(),
        }
    }

    /// The target whose backend writes it.
    pub fn target(self) -> Target {
        match self {
            Output::Docker => Target::Docker,
            Output::Hosted => Target::Hosted,
            Output::Kubernetes | Output::Helm | Output::Kustomize => Target::Kubernetes,
            Output::Vagrant => Target::Vagrant,
            Output::DockerVm => Target::DockerVm,
            Output::Hybrid => Target::Hybrid,
            Output::Proxmox => Target::Proxmox,
            Output::CloudVm(_) => Target::CloudVm,
            Output::CloudDocker => Target::CloudDocker,
            Output::CloudServices => Target::CloudServices,
            Output::External => Target::External,
        }
    }

    /// The rendering it shares; `None` for hybrid, made of two ([`super::Status::combine`]).
    pub fn family(self) -> Option<Family> {
        Some(match self {
            Output::Docker | Output::Hosted | Output::DockerVm | Output::CloudDocker => Family::Compose,
            Output::Kubernetes | Output::Helm | Output::Kustomize => Family::Kubernetes,
            Output::Vagrant => Family::Vagrant,
            Output::Hybrid => return None,
            Output::Proxmox => Family::Proxmox,
            Output::CloudVm(_) => Family::CloudVm,
            Output::CloudServices => Family::CloudServices,
            Output::External => Family::External,
        })
    }

    /// Whether its cells are its family's own claims, proven on the family's target; the other
    /// outputs inherit them, checked by refusals.
    pub fn is_primary(self) -> bool {
        match self {
            Output::CloudVm(cloud) => isoloom_terraform::cloud_vm::CLOUDS.first().is_some_and(|(first, _)| *first == cloud),
            Output::Hybrid => false,
            other => other.family().is_some_and(|f| f.target() == other.target()) && !matches!(other, Output::Helm | Output::Kustomize),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_with_a_backend_has_a_column_and_each_family_one_primary() {
        let all = Output::all();
        for t in isoloom_targets::GENERATED_TARGETS {
            assert!(all.iter().any(|o| o.target() == *t), "{} has no column", t.id());
        }
        for f in Family::ALL {
            let primaries: Vec<_> = all.iter().filter(|o| o.is_primary() && o.family() == Some(f)).collect();
            assert_eq!(primaries.len(), 1, "{f:?}: {primaries:?}");
        }
        let labels: Vec<_> = all.iter().map(|o| o.label()).collect();
        assert!(
            labels.contains(&"aws") && labels.contains(&"helm") && labels.contains(&"docker-vm"),
            "{labels:?}"
        );
    }
}
