//! The Kubernetes output's cells for the machines' fields and the shared volumes.

use crate::spec::status::Status::{Done, NotApplicable, Partial, Planned};
use crate::spec::status::{NOT_A_VM, PROBE, Status};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "machines.*.networks",
        Partial {
            how: "a label per network the policies select; reached by name through its Service",
            gap: "the fixed address: Kubernetes picks pod addresses",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.ports[].port",
        Done {
            how: "container ports, a Service named after the machine, a readiness probe dependents wait for",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.ports[].publish",
        Done {
            how: "a LoadBalancer Service `<machine>-published` on the published port and a policy letting anyone in on it",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.ports[].fixed",
        Done {
            how: "the LoadBalancer answers on `publish` itself",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.ports[].dormant",
        Done {
            how: "left out of the readiness probe and the dependents' waits",
            proof: "segmented",
        },
    ),
    (
        "machines.*.ports[].protocol",
        Done {
            how: "`udp`: UDP container, Service and LoadBalancer ports; left out of the probe and waits",
            proof: "udp-dns",
        },
    ),
    (
        "machines.*.inputs",
        Done {
            how: "env vars from the `isoloom-inputs` Secret on the machine's container and its `init` container",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.secrets",
        Done {
            how: "a Secret volume, each key mounted at its path with its mode (the `init` container's too); one with a `uid` or `gid` copied into a memory emptyDir with its owner by an init container",
            proof: PROBE,
        },
    ),
    (
        "machines.*.environment",
        Done {
            how: "the container's env (and its `init` container's)",
            proof: PROBE,
        },
    ),
    (
        "machines.*.sysctls",
        Done {
            how: "the pod's securityContext sysctls (unsafe ones need the kubelet to allow them)",
            proof: PROBE,
        },
    ),
    (
        "machines.*.ulimits",
        NotApplicable {
            why: "Kubernetes has no per-container process limits",
        },
    ),
    (
        "machines.*.aliases",
        NotApplicable {
            why: "dotted DNS names aren't Kubernetes Service names",
        },
    ),
    (
        "machines.*.arch",
        Done {
            how: "a node selector `kubernetes.io/arch`",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.privileged",
        Done {
            how: "a privileged container securityContext",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.read_only",
        Done {
            how: "`readOnlyRootFilesystem` in the container's securityContext",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.tmpfs",
        Done {
            how: "a Memory emptyDir per path",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.shm_size",
        Done {
            how: "a Memory emptyDir at /dev/shm, size-limited",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.dns.servers",
        Partial {
            how: "dnsPolicy `None` with dnsConfig nameservers",
            gap: "the cluster DNS is dropped: Service names resolve only if the server forwards them",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.dns.search",
        Done {
            how: "dnsConfig searches (added to the cluster's resolver unless the machine has servers of its own)",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.dns.domain",
        Partial {
            how: "the pod's `subdomain` (the domain as one label): its full name in its hosts file",
            gap: "Kubernetes names pods under the cluster's domain: `<machine>.<subdomain>.<namespace>.svc.<cluster domain>`, not `dns.domain`",
            proof: "arm-lab",
        },
    ),
    (
        "machines.*.dns.options",
        Done {
            how: "dnsConfig options (with the cluster's resolver unless the machine has servers of its own)",
            proof: PROBE,
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
            why: "containers share the node's disk",
        },
    ),
    (
        "machines.*.depends_on",
        Partial {
            how: "init containers that wait until the dependencies' services answer",
            gap: "the dependencies' `init:` jobs finishing",
            proof: "hello-stack",
        },
    ),
    (
        "volumes.*.owner",
        Done {
            how: "one claim, held by the owner; its other users get pod affinity to the owner's node",
            proof: "shared-files",
        },
    ),
    (
        "machines.*.volumes",
        Done {
            how: "a ReadWriteOnce claim per path, seeded from the image as Docker seeds a named volume",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.access",
        Done {
            how: "checks from it run as a Job carrying its networks' labels",
            proof: "segmented",
        },
    ),
    (
        "machines.*.docker.image",
        Done {
            how: "the container's image through the mirrors (Helm: an `images.<machine>` value)",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.docker.build",
        Partial {
            how: "the image tag the pod runs, with the `docker build` line in the manifests' comment",
            gap: "the build itself: the image is built and pushed by hand",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.docker.dockerfile",
        Partial {
            how: "`-f` in the `docker build` line of the comment",
            gap: "the build itself",
            proof: PROBE,
        },
    ),
    (
        "machines.*.docker.target",
        Partial {
            how: "`--target` in the `docker build` line of the comment",
            gap: "the build itself",
            proof: PROBE,
        },
    ),
    (
        "machines.*.docker.args",
        Partial {
            how: "`--build-arg` in the `docker build` line of the comment",
            gap: "the build itself",
            proof: PROBE,
        },
    ),
    (
        "machines.*.docker.init",
        Done {
            how: "a second container in the pod that runs the scripts once the machine answers (Kustomize: a configMapGenerator; Helm: the chart's copies)",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.docker.appliance",
        NotApplicable {
            why: "network appliances run on the Docker targets only",
        },
    ),
    (
        "machines.*.docker.firmware",
        NotApplicable {
            why: "network appliances run on the Docker targets only",
        },
    ),
    (
        "machines.*.docker.config",
        NotApplicable {
            why: "network appliances run on the Docker targets only",
        },
    ),
    (
        "machines.*.docker.idle",
        Done {
            how: "`command: [sleep, infinity]`",
            proof: "workbench",
        },
    ),
    (
        "machines.*.os",
        Planned {
            note: "a container of the OS family, from the service catalogue; today `os` names the machine's VM only",
        },
    ),
    ("machines.*.vm.os", NOT_A_VM),
    ("machines.*.vm.image.vagrant", NOT_A_VM),
    ("machines.*.vm.image.vagrant_version", NOT_A_VM),
    ("machines.*.vm.image.qemu", NOT_A_VM),
    ("machines.*.vm.image.hyperv", NOT_A_VM),
    ("machines.*.vm.image.winrm", NOT_A_VM),
    ("machines.*.vm.image.winssh", NOT_A_VM),
    ("machines.*.vm.provision", NOT_A_VM),
    ("machines.*.vm.tpm", NOT_A_VM),
];
