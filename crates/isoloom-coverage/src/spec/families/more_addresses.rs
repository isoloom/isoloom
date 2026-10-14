//! Every output's cells for a machine's several addresses on one network (`lan: [10, 11]`), in
//! one place: a VM's set-up adds them on every VM output, a cloud learns them too, and the
//! container and existing-host outputs refuse them.

use crate::spec::status::PROBE;
use crate::spec::status::Status::{self, Done, NotApplicable};

/// The row.
const ROW: &str = "machines.*.networks[]";

/// The Compose file's: Docker gives a container's endpoint one IPv4 address.
pub const COMPOSE: &[(&str, Status)] = &[(
    ROW,
    NotApplicable {
        why: "a container has one address per network (Docker gives an endpoint one IPv4 address): the backend refuses it",
    },
)];

/// The Kubernetes manifests': a pod has one address, the cluster's.
pub const KUBERNETES: &[(&str, Status)] = &[(
    ROW,
    NotApplicable {
        why: "a pod has one address, which the cluster picks: the backend refuses it",
    },
)];

/// Vagrant's.
pub const VAGRANT: &[(&str, Status)] = &[(
    ROW,
    Done {
        how: "each VM's set-up adds them on the interface of its primary address, now and at every boot, on every provider; hosts files and derived checks name them",
        proof: "several-addresses",
    },
)];

/// Proxmox's (and ESXi's, which shares the column's rendering).
pub const PROXMOX: &[(&str, Status)] = &[(
    ROW,
    Done {
        how: "each VM's set-up (cloud-init) adds them on the interface of its primary address; hosts files and derived checks name them (not run on a real host)",
        proof: "several-addresses",
    },
)];

/// The clouds' (claimed through the probe: one machine, which every cloud renders).
pub const CLOUD_VM: &[(&str, Status)] = &[(
    ROW,
    Done {
        how: "the cloud learns them (AWS secondary private addresses and IPv4 prefixes, Azure IP configurations, Google Cloud alias ranges, Oracle Cloud secondary private IPs, Linode routed ranges; DigitalOcean refuses) and the VM's set-up adds them (not run on a real host)",
        proof: PROBE,
    },
)];

/// The existing hosts'.
pub const EXTERNAL: &[(&str, Status)] = &[(
    ROW,
    NotApplicable {
        why: "an existing host keeps the addresses it has: the backend refuses it",
    },
)];
