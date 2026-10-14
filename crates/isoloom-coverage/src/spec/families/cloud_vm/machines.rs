//! The cloud VM output's cells for the machines' fields and the shared volumes.

use crate::spec::status::Status::{Done, NotApplicable, Partial, Planned};
use crate::spec::status::{NOT_A_CONTAINER, PROBE, Status, VAGRANT_BOX};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "machines.*.networks",
        Partial {
            how: "the instance at its fixed private address in its subnet, an interface per network; names in /etc/hosts",
            gap: "Windows on several networks",
            proof: "vlan-office",
        },
    ),
    (
        "machines.*.ports[].port",
        Done {
            how: "machines depending on it wait until it answers; the Windows firewall opened for it",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.ports[].publish",
        Done {
            how: "ingress from `allowed_cidr` on the public address, a differing port redirected inside; the `published` output lists them",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.ports[].fixed",
        Done {
            how: "the public address serves `publish` as is",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.ports[].dormant",
        Done {
            how: "left out of the dependency waits; no derived check expects it to answer",
            proof: "segmented",
        },
    ),
    (
        "machines.*.ports[].protocol",
        Done {
            how: "`udp`: UDP ingress and redirect, left out of the waits; `reach` opens TCP and UDP",
            proof: "udp-dns",
        },
    ),
    (
        "machines.*.inputs",
        Partial {
            how: "an env file read by that machine's steps only",
            gap: "Windows machines",
            proof: "supplier-portal-api",
        },
    ),
    ("machines.*.secrets", crate::spec::status::SECRETS_ON_VMS),
    (
        "machines.*.environment",
        Done {
            how: "/etc/environment and systemd's default environment before the steps (machine variables on Windows)",
            proof: "tuned-web",
        },
    ),
    (
        "machines.*.sysctls",
        Done {
            how: "/etc/sysctl.d, applied before the steps",
            proof: "tuned-web",
        },
    ),
    (
        "machines.*.ulimits",
        Done {
            how: "limits.d and systemd's default limits, before the steps",
            proof: "tuned-web",
        },
    ),
    (
        "machines.*.aliases",
        Done {
            how: "in every VM's hosts file after the machine's name",
            proof: PROBE,
        },
    ),
    (
        "machines.*.os",
        Done {
            how: "the cloud's image for the machine: `vm.os` defaults to it, and a machine with neither `docker:` nor `vm:` is a VM of it",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.arch",
        Partial {
            how: "amd64 instances",
            gap: "arm64 (refused: Graviton instances and arm64 images come later)",
            proof: "segmented",
        },
    ),
    (
        "machines.*.privileged",
        NotApplicable {
            why: "a VM already has kernel access: its workload runs as root",
        },
    ),
    (
        "machines.*.read_only",
        NotApplicable {
            why: "a VM's root filesystem stays writable; use `volumes` for data that must persist",
        },
    ),
    (
        "machines.*.tmpfs",
        NotApplicable {
            why: "a VM mounts tmpfs in its own provisioning (fstab)",
        },
    ),
    (
        "machines.*.shm_size",
        NotApplicable {
            why: "a VM sizes /dev/shm in its own provisioning",
        },
    ),
    (
        "machines.*.dns.servers",
        Planned {
            note: "the resolver in the cloud-init user data; instances use the cloud's resolver today",
        },
    ),
    (
        "machines.*.dns.search",
        Planned {
            note: "the search domains in the cloud-init user data",
        },
    ),
    (
        "machines.*.dns.domain",
        Planned {
            note: "the domain in the cloud-init user data",
        },
    ),
    (
        "machines.*.dns.options",
        Planned {
            note: "the resolver options in the cloud-init user data",
        },
    ),
    (
        "machines.*.resources.cpus",
        Planned {
            note: "the instance type follows memory only today",
        },
    ),
    (
        "machines.*.resources.memory_mb",
        Done {
            how: "picks the instance type",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.resources.disk_gb",
        Done {
            how: "the root disk's size, the filesystem grown into it",
            proof: "segmented",
        },
    ),
    (
        "machines.*.depends_on",
        Done {
            how: "set-up runs after the dependencies' set-up, then waits until they answer",
            proof: "hello-stack",
        },
    ),
    (
        "volumes.*.owner",
        Partial {
            how: "the owner exports it over NFS; the others mount it before their steps",
            gap: "Windows machines",
            proof: "shared-files",
        },
    ),
    (
        "machines.*.volumes",
        Partial {
            how: "the instance's own disk keeps the data; the path is created",
            gap: "Windows machines",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.access",
        Done {
            how: "checks without `from` run on it; the `ip` output points at it",
            proof: "segmented",
        },
    ),
    ("machines.*.docker.image", NOT_A_CONTAINER),
    ("machines.*.docker.build", NOT_A_CONTAINER),
    ("machines.*.docker.dockerfile", NOT_A_CONTAINER),
    ("machines.*.docker.target", NOT_A_CONTAINER),
    ("machines.*.docker.args", NOT_A_CONTAINER),
    ("machines.*.docker.init", NOT_A_CONTAINER),
    ("machines.*.docker.appliance", NOT_A_CONTAINER),
    ("machines.*.docker.firmware", NOT_A_CONTAINER),
    ("machines.*.docker.config", NOT_A_CONTAINER),
    ("machines.*.docker.idle", NOT_A_CONTAINER),
    (
        "machines.*.vm.os",
        Partial {
            how: "the cloud's images for Debian, Ubuntu and Windows Server",
            gap: "Kali",
            proof: "segmented",
        },
    ),
    ("machines.*.vm.image.vagrant", VAGRANT_BOX),
    ("machines.*.vm.image.vagrant_version", VAGRANT_BOX),
    ("machines.*.vm.image.qemu", VAGRANT_BOX),
    ("machines.*.vm.image.hyperv", VAGRANT_BOX),
    (
        "machines.*.vm.image.winrm",
        NotApplicable {
            why: "cloud Windows images get WinRM from their user data",
        },
    ),
    (
        "machines.*.vm.image.winssh",
        NotApplicable {
            why: "cloud Windows images get WinRM from their user data",
        },
    ),
    (
        "machines.*.vm.provision",
        Done {
            how: "steps over SSH (.sh, Ansible on the machine, `reboot` between provisioners); .ps1 over WinRM on Windows",
            proof: "existing-hosts",
        },
    ),
    (
        "machines.*.vm.tpm",
        Partial {
            how: "NitroTPM on AWS (the plan stops if the AMI has none), trusted launch on Azure, a shielded VM on Google Cloud and Oracle Cloud",
            gap: "Windows on AWS (Amazon's NitroTPM Windows images)",
            proof: "tuned-web",
        },
    ),
];
