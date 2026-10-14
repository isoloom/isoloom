//! The Proxmox output's cells for the machines' fields and the shared volumes.

use crate::spec::status::Status::{Done, NotApplicable, Partial, Planned};
use crate::spec::status::{NOT_A_CONTAINER, PROBE, Status, VAGRANT_BOX};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "machines.*.networks",
        Done {
            how: "a NIC per network with its fixed address (cloud-init), the router as gateway",
            proof: "segmented",
        },
    ),
    (
        "machines.*.services[].port",
        Done {
            how: "machines depending on it wait until it answers",
            proof: "segmented",
        },
    ),
    (
        "machines.*.services[].publish",
        Done {
            how: "forwarded by the router from its uplink address; the `published` output lists them",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.services[].fixed",
        Done {
            how: "the router forwards `publish` as is",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.services[].dormant",
        Done {
            how: "left out of the dependency waits; no derived check expects it to answer",
            proof: "segmented",
        },
    ),
    (
        "machines.*.services[].protocol",
        Done {
            how: "`udp`: the router forwards a published port as UDP; left out of the dependency waits; `reach` opens TCP and UDP",
            proof: "udp-dns",
        },
    ),
    (
        "machines.*.inputs",
        Done {
            how: "/etc/isoloom/inputs.env (0600), read by that machine's steps only",
            proof: "supplier-portal-api",
        },
    ),
    ("machines.*.secrets", crate::spec::status::SECRETS_ON_VMS),
    (
        "machines.*.environment",
        Done {
            how: "/etc/environment and systemd's default environment, before the steps",
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
        "machines.*.arch",
        Done {
            how: "amd64 VMs on the server's CPU; arm64 ones as aarch64 VMs QEMU emulates (UEFI, Debian's and Ubuntu's arm64 cloud images; `root@pam` sets the architecture; not run on a real host)",
            proof: "arm-vm",
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
        Done {
            how: "cloud-init's resolver",
            proof: PROBE,
        },
    ),
    (
        "machines.*.dns.search",
        Planned {
            note: "cloud-init's search domains",
        },
    ),
    (
        "machines.*.dns.domain",
        Done {
            how: "cloud-init's domain",
            proof: PROBE,
        },
    ),
    (
        "machines.*.dns.options",
        Planned {
            note: "cloud-init's resolver options",
        },
    ),
    (
        "machines.*.resources.cpus",
        Done {
            how: "the VM's cores",
            proof: "segmented",
        },
    ),
    (
        "machines.*.resources.memory_mb",
        Done {
            how: "the VM's memory",
            proof: "segmented",
        },
    ),
    (
        "machines.*.resources.disk_gb",
        Done {
            how: "the VM's disk size",
            proof: "segmented",
        },
    ),
    (
        "machines.*.depends_on",
        Done {
            how: "waits until its dependencies answer (cloud-init)",
            proof: "segmented",
        },
    ),
    (
        "volumes.*.owner",
        Done {
            how: "the owner's VM exports it over NFS; the others mount it",
            proof: "shared-files",
        },
    ),
    (
        "machines.*.volumes",
        Done {
            how: "the VM's own disk keeps the data; the path is created",
            proof: "segmented",
        },
    ),
    (
        "machines.*.access",
        Done {
            how: "checks without `from` run on it",
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
            how: "cloud images for debian-12 and ubuntu-24.04; Windows as a clone of a template you give (cloudbase-init)",
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
            why: "about a Windows Vagrant box: a Windows template's set-up opens WinRM itself",
        },
    ),
    (
        "machines.*.vm.image.winssh",
        NotApplicable {
            why: "about a Windows Vagrant box: a Windows template's set-up opens WinRM itself",
        },
    ),
    (
        "machines.*.vm.provision",
        Done {
            how: "cloud-init writes the project and runs the steps; after a `reboot`, a unit runs the rest at the next boot",
            proof: "existing-hosts",
        },
    ),
    (
        "machines.*.vm.tpm",
        Done {
            how: "a TPM 2.0 (`tpm_state` on the VM datastore)",
            proof: PROBE,
        },
    ),
];
