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
    if target == Target::External {
        return Placement::Refused("`external` creates nothing: run the service as one of its machines".to_owned());
    }
    if target == Target::CloudVm {
        return managed(service);
    }
    if service.uses_identity() {
        return Placement::Refused(format!("a platform identity exists only on a cloud: `access: keys` for {}", target.id()));
    }
    match service {
        Service::ObjectStorage(o) if o.versioning => {
            Placement::Refused("the self-hosted S3 server keeps no object versions: `versioning: false`, or a cloud".to_owned())
        }
        _ => Placement::Machine,
    }
}

/// On the clouds: their managed service (each cloud of `cloud-vm` may still refuse it).
fn managed(service: &Service) -> Placement {
    match service {
        Service::Kafka(k) if k.publish.is_some() => {
            Placement::Refused("a managed Kafka's brokers stay inside the cloud's network: publish it from a container or VM target".to_owned())
        }
        _ => Placement::Managed,
    }
}
