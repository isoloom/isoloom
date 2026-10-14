//! The Docker Compose output's cells for the machines' fields and the shared volumes.

use crate::spec::status::Status::{Done, NotApplicable};
use crate::spec::status::{NOT_A_VM, Status};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "machines.*.networks",
        Done {
            how: "a fixed address on each network; names across networks",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.services[].port",
        Done {
            how: "a healthcheck, so `depends_on` and `up --wait` wait for it",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.services[].publish",
        Done {
            how: "a published port, on the host's loopback",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.services[].fixed",
        Done {
            how: "the `publish` port itself on the host, not a free one",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.services[].dormant",
        Done {
            how: "left out of the healthcheck (so `depends_on` and `up --wait` don't wait for it) and of the derived checks that expect an answer; still published and opened by `reach`",
            proof: "segmented",
        },
    ),
    (
        "machines.*.services[].protocol",
        Done {
            how: "`udp`: published as `<port>/udp`; left out of the healthcheck and the derived checks (no handshake); opened by `reach` (TCP and UDP alike)",
            proof: "udp-dns",
        },
    ),
    (
        "machines.*.inputs",
        Done {
            how: "environment of that machine and its init jobs only",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.environment",
        Done {
            how: "the container's environment (and its Kubernetes env)",
            proof: "tuned-web",
        },
    ),
    (
        "machines.*.sysctls",
        Done {
            how: "the container's namespaced sysctls (and the Kubernetes pod's)",
            proof: "tuned-web",
        },
    ),
    (
        "machines.*.ulimits",
        Done {
            how: "the container's ulimits (Kubernetes has none per container: refused)",
            proof: "tuned-web",
        },
    ),
    (
        "machines.*.aliases",
        Done {
            how: "network aliases (and extra_hosts across the router)",
            proof: "edge-firewall",
        },
    ),
    (
        "machines.*.arch",
        Done {
            how: "`platform` on the container and a node selector on Kubernetes (amd64/arm64)",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.privileged",
        Done {
            how: "the container runs privileged (and a Kubernetes securityContext)",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.read_only",
        Done {
            how: "a read-only root filesystem on the container (and a Kubernetes securityContext)",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.tmpfs",
        Done {
            how: "memory-backed mounts on the container (and Memory emptyDirs on Kubernetes)",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.shm_size",
        Done {
            how: "the size of /dev/shm on the container (and a Memory emptyDir on Kubernetes)",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.dns.servers",
        Done {
            how: "`dns` on the container (and the Kubernetes dnsConfig nameservers)",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.dns.search",
        Done {
            how: "`dns_search` on the container (and the Kubernetes dnsConfig searches)",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.dns.domain",
        Done {
            how: "`domainname` on the container",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.resources.cpus",
        Done {
            how: "a CPU limit",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.resources.memory_mb",
        Done {
            how: "a memory limit",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.resources.disk_gb",
        NotApplicable {
            why: "containers share the host's disk",
        },
    ),
    (
        "machines.*.depends_on",
        Done {
            how: "starts after its dependencies answer (and their init jobs finish)",
            proof: "hello-stack",
        },
    ),
    (
        "volumes.*.owner",
        Done {
            how: "one named volume (one claim on Kubernetes, its users on the owner's node)",
            proof: "shared-files",
        },
    ),
    (
        "machines.*.volumes",
        Done {
            how: "a named volume per path",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.access",
        Done {
            how: "checks run from its network namespace (a stand-in when the runner supplies it)",
            proof: "segmented",
        },
    ),
    (
        "machines.*.docker.image",
        Done {
            how: "the service's image",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.docker.build",
        Done {
            how: "a build from the project folder",
            proof: "segmented",
        },
    ),
    (
        "machines.*.docker.dockerfile",
        Done {
            how: "the build's `dockerfile`, relative to its context",
            proof: "slow-link",
        },
    ),
    (
        "machines.*.docker.args",
        Done {
            how: "the build's `args`",
            proof: "slow-link",
        },
    ),
    (
        "machines.*.docker.init",
        Done {
            how: "one-shot jobs in its network namespace, after it answers",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.docker.appliance",
        Done {
            how: "Cisco IOL as vrnetlab packages it: the management port first, the networks as Ethernet0/1..., NETMAP and iouyap, a startup configuration with the addresses",
            proof: "cisco-iol",
        },
    ),
    (
        "machines.*.docker.firmware",
        Done {
            how: "the IOS image a Dynamips 7200 boots, in a container Isoloom builds",
            proof: "cisco-dynamips",
        },
    ),
    (
        "machines.*.docker.config",
        Done {
            how: "added to the generated startup configuration when the container starts",
            proof: "cisco-iol",
        },
    ),
    (
        "machines.*.docker.idle",
        Done {
            how: "kept running idle (`entrypoint: [sleep, infinity]`), a machine to work from",
            proof: "workbench",
        },
    ),
    ("machines.*.vm.os", NOT_A_VM),
    ("machines.*.vm.image.vagrant", NOT_A_VM),
    ("machines.*.vm.image.vagrant_version", NOT_A_VM),
    ("machines.*.vm.image.qemu", NOT_A_VM),
    ("machines.*.vm.image.hyperv", NOT_A_VM),
    ("machines.*.vm.image.winrm", NOT_A_VM),
    ("machines.*.vm.provision", NOT_A_VM),
];
