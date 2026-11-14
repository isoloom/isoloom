//! Where a service goes on each target: the cloud's managed service, a machine running the
//! self-hosted equivalent, or a refusal saying why. The clouds of `cloud-vm` refuse it one by one
//! in the Terraform backend; this table is per target.

use super::model::Service;
use crate::model::Target;

/// How a target renders a service.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Placement {
    /// As the cloud's managed service, beside the machines' module (`services.tf`).
    Managed,
    /// As a machine of the environment running the self-hosted equivalent.
    Machine,
    /// Not at all, with why and what to do instead.
    Refused(String),
}

/// How `target` renders `service`.
#[must_use]
pub fn placement(service: &Service, target: Target) -> Placement {
    match target {
        Target::External => Placement::Refused("`external` creates nothing: run the service as one of its machines".to_owned()),
        Target::CloudVm => managed(service),
        Target::Docker | Target::Hosted | Target::CloudDocker | Target::Kubernetes => container(service),
        Target::Vagrant | Target::Proxmox | Target::Esxi => vm(service),
    }
}

/// On the clouds: managed, but a bucket's facts are only known once applied.
fn managed(service: &Service) -> Placement {
    match service {
        Service::ObjectStorage(o) if !o.clients.is_empty() => Placement::Refused(
            "a cloud bucket's name and keys are only known once applied, and a cloud VM can't receive them yet: leave `clients` out (the module's outputs give them)".to_owned(),
        ),
        _ => Placement::Managed,
    }
}

/// On the container targets: the self-hosted server's container.
fn container(service: &Service) -> Placement {
    match service {
        Service::ObjectStorage(o) if o.versioning => {
            Placement::Refused("the self-hosted S3 server keeps no object versions: `versioning: false`, or a cloud".to_owned())
        }
        _ => Placement::Machine,
    }
}

/// On the VM targets: a VM running the self-hosted server, where one has a pinned VM form.
fn vm(service: &Service) -> Placement {
    match service {
        Service::Postgres(_) => Placement::Machine,
        Service::ObjectStorage(_) => {
            Placement::Refused("no S3 server has a pinned VM form yet (MinIO no longer publishes builds): a container target, or a cloud".to_owned())
        }
    }
}
