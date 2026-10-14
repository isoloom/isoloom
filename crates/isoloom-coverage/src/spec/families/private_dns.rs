//! Every output's cells for the environment's private DNS zone (`dns.private`, `dns.domain`), in
//! one place: the platform's own DNS where it has one, each VM's hosts file everywhere.

use crate::spec::status::PROBE;
use crate::spec::status::Status::{self, Done, Planned};

/// The Compose file's.
pub const COMPOSE: &[(&str, Status)] = &[
    (
        "dns.private",
        Done {
            how: "Docker's DNS answers every name and alias under the zone's domain too (network aliases)",
            proof: "private-names",
        },
    ),
    (
        "dns.domain",
        Done {
            how: "the domain of those aliases",
            proof: "private-names",
        },
    ),
];

/// The Kubernetes manifests'.
pub const KUBERNETES: &[(&str, Status)] = &[
    (
        "dns.private",
        Planned {
            note: "the cluster's DNS serving a zone of the environment's (a CoreDNS stub domain): a cluster names Services under its own domain",
        },
    ),
    (
        "dns.domain",
        Planned {
            note: "the cluster's DNS serving a zone of the environment's (a CoreDNS stub domain): a cluster names Services under its own domain",
        },
    ),
];

/// The VM outputs' with no DNS of their own (Vagrant, Proxmox): `how` says where the names go.
const fn vm(how: &'static str) -> [(&'static str, Status); 2] {
    [
        ("dns.private", Done { how, proof: "private-names" }),
        (
            "dns.domain",
            Done {
                how: "the domain of the names in each VM's hosts file",
                proof: "private-names",
            },
        ),
    ]
}

/// Vagrant's.
pub const VAGRANT: &[(&str, Status)] = &vm("each VM's hosts file carries every name and alias under the zone's domain");

/// Proxmox's.
pub const PROXMOX: &[(&str, Status)] = &vm(
    "each VM's hosts file carries every name and alias under the zone's domain, its search domain (the SDN zone's DNS needs a PowerDNS server outside the environment)",
);

/// The clouds' (claimed through the probe: one machine, which every cloud renders).
pub const CLOUD_VM: &[(&str, Status)] = &[
    (
        "dns.private",
        Done {
            how: "a private zone bound to the environment's network (Route 53 on AWS, Azure private DNS, Cloud DNS, an Oracle Cloud private view), an A record per name and alias; on DigitalOcean and Linode, which have none, each VM's hosts file (not run on a real host)",
            proof: PROBE,
        },
    ),
    (
        "dns.domain",
        Done {
            how: "the private zone's domain",
            proof: PROBE,
        },
    ),
];

/// The existing hosts'.
pub const EXTERNAL: &[(&str, Status)] = &[
    (
        "dns.private",
        Planned {
            note: "hosts file entries over SSH, as for `aliases`: an existing host keeps its own resolver",
        },
    ),
    (
        "dns.domain",
        Planned {
            note: "hosts file entries over SSH, as for `aliases`: an existing host keeps its own resolver",
        },
    ),
];
