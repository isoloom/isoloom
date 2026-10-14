//! The Vagrant output's cells for the machines' fields and the shared volumes.

use crate::spec::status::Status::{Done, NotApplicable, Partial};
use crate::spec::status::{NOT_A_CONTAINER, PROBE, Status};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "machines.*.networks",
        Done {
            how: "a fixed address on each network; names in /etc/hosts",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.services[].port",
        Done {
            how: "machines depending on it wait until it answers",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.services[].publish",
        Done {
            how: "a forwarded port, on the host's loopback",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.services[].fixed",
        Done {
            how: "Vagrant forwards `publish` as is",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.services[].dormant",
        Done {
            how: "left out of the dependency waits and of the derived checks that expect an answer; still forwarded and opened by `reach`",
            proof: "segmented",
        },
    ),
    (
        "machines.*.services[].protocol",
        Partial {
            how: "`udp`: forwarded as UDP; left out of the dependency waits and the derived checks; opened by `reach`",
            gap: "vagrant-libvirt skips UDP forwarded ports (`isoloom run` warns): reach the machine's address instead",
            proof: "udp-dns",
        },
    ),
    (
        "machines.*.inputs",
        Done {
            how: "environment of that machine's provisioning only",
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
            how: "in every Linux VM's /etc/hosts, after the machine's name",
            proof: "edge-firewall",
        },
    ),
    (
        "machines.*.arch",
        Done {
            how: "`box_architecture` on the VM",
            proof: "segmented",
        },
    ),
    (
        "machines.*.privileged",
        Done {
            how: "nested virtualization on Hyper-V (`enable_virtualization_extensions`); elsewhere a VM already has kernel access",
            proof: "existing-hosts",
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
        NotApplicable {
            why: "a VM sets its resolver in its own provisioning (resolv.conf)",
        },
    ),
    (
        "machines.*.dns.search",
        NotApplicable {
            why: "a VM sets its search domains in its own provisioning",
        },
    ),
    (
        "machines.*.dns.domain",
        NotApplicable {
            why: "a VM sets its domain in its own provisioning",
        },
    ),
    (
        "machines.*.dns.options",
        NotApplicable {
            why: "a VM sets its resolver options in its own provisioning",
        },
    ),
    (
        "machines.*.resources.cpus",
        Done {
            how: "the VM's CPUs on every provider",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.resources.memory_mb",
        Done {
            how: "the VM's memory on every provider",
            proof: "supplier-portal-api",
        },
    ),
    (
        "machines.*.resources.disk_gb",
        Done {
            how: "the VM's disk on every provider (`vm.disk`, or the provider's own disk size), the root filesystem grown into it",
            proof: "segmented",
        },
    ),
    (
        "machines.*.depends_on",
        Done {
            how: "boots after its dependencies, then waits until they answer",
            proof: "hello-stack",
        },
    ),
    (
        "volumes.*.owner",
        Done {
            how: "the owner's directory served over NFS, mounted by the others before their steps",
            proof: "shared-files",
        },
    ),
    (
        "machines.*.volumes",
        Done {
            how: "the VM's own disk keeps the data; the path is created",
            proof: "hello-stack",
        },
    ),
    (
        "machines.*.access",
        Partial {
            how: "a VM like the others when it has `vm:`",
            gap: "checks don't run from it",
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
        Done {
            how: "a Vagrant box for every OS name, Windows among them (over WinRM, or OpenSSH with `vm.image.winssh`)",
            proof: "windows-hello",
        },
    ),
    (
        "machines.*.vm.image.vagrant",
        Done {
            how: "the machine's own Vagrant box instead of the built-in one",
            proof: "windows-hello",
        },
    ),
    (
        "machines.*.vm.image.vagrant_version",
        Done {
            how: "the box version, pinned",
            proof: "windows-hello",
        },
    ),
    (
        "machines.*.vm.image.qemu",
        Done {
            how: "the box Vagrant's QEMU provider boots instead (libvirt format), when the main one has none",
            proof: "windows-hello",
        },
    ),
    (
        "machines.*.vm.image.hyperv",
        Done {
            how: "the box Vagrant's Hyper-V provider boots instead (its latest version with a Hyper-V build)",
            proof: "windows-hello",
        },
    ),
    (
        "machines.*.vm.image.winrm",
        Done {
            how: "how the Windows box answers WinRM: `plaintext` (HTTP 5985, the default) or `ssl` (HTTPS 5986)",
            proof: "windows-hello",
        },
    ),
    (
        "machines.*.vm.image.winssh",
        Done {
            how: "a Windows box reached over OpenSSH (`config.winssh`, PowerShell), the playbooks over SSH",
            proof: PROBE,
        },
    ),
    (
        "machines.*.vm.provision",
        Done {
            how: "`.sh` steps, Ansible run inside the VM, and `reboot` (Vagrant restarts the VM)",
            proof: "existing-hosts",
        },
    ),
    (
        "machines.*.vm.tpm",
        Done {
            how: "a TPM 2.0 on VirtualBox (`--tpm-type`), libvirt (emulated) and Hyper-V (a trigger); `tpm: true` is refused on the other providers",
            proof: PROBE,
        },
    ),
];
