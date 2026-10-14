//! The external output's cells for the machines' fields and the shared volumes.

use crate::spec::status::Status::{Done, NotApplicable, Partial, Planned};
use crate::spec::status::{EXISTING_HOST, PROBE, Status};

/// The cells of this section.
pub const CELLS: &[(&str, Status)] = &[
    (
        "machines.*.networks",
        Done {
            how: "the machine's address on each network, which the other runners probe",
            proof: "existing-hosts",
        },
    ),
    (
        "machines.*.services[].port",
        Done {
            how: "derived checks from the other hosts expect it to answer",
            proof: "existing-hosts",
        },
    ),
    (
        "machines.*.services[].publish",
        NotApplicable {
            why: "an existing host is reached at its own address: nothing to forward",
        },
    ),
    (
        "machines.*.services[].fixed",
        NotApplicable {
            why: "an existing host is reached at its own address: nothing to forward",
        },
    ),
    (
        "machines.*.services[].dormant",
        Done {
            how: "left out of the derived checks that expect an answer",
            proof: PROBE,
        },
    ),
    (
        "machines.*.services[].protocol",
        Done {
            how: "`udp`: left out of the derived checks; `reach` checks TCP",
            proof: PROBE,
        },
    ),
    (
        "machines.*.inputs",
        Planned {
            note: "passed to the machine's steps",
        },
    ),
    (
        "machines.*.environment",
        Planned {
            note: "/etc/environment over SSH",
        },
    ),
    (
        "machines.*.sysctls",
        Planned {
            note: "/etc/sysctl.d over SSH",
        },
    ),
    ("machines.*.ulimits", Planned { note: "limits.d over SSH" }),
    (
        "machines.*.aliases",
        Planned {
            note: "/etc/hosts entries over SSH",
        },
    ),
    (
        "machines.*.arch",
        NotApplicable {
            why: "an existing host has its own architecture",
        },
    ),
    (
        "machines.*.privileged",
        NotApplicable {
            why: "steps run with sudo as the SSH user: nothing to grant",
        },
    ),
    (
        "machines.*.read_only",
        NotApplicable {
            why: "an existing host's root filesystem is what it is",
        },
    ),
    (
        "machines.*.tmpfs",
        NotApplicable {
            why: "the host mounts its own",
        },
    ),
    (
        "machines.*.shm_size",
        NotApplicable {
            why: "the host sizes its own /dev/shm",
        },
    ),
    (
        "machines.*.dns.servers",
        NotApplicable {
            why: "an existing host keeps its resolver",
        },
    ),
    (
        "machines.*.dns.search",
        NotApplicable {
            why: "an existing host keeps its resolver",
        },
    ),
    (
        "machines.*.dns.domain",
        NotApplicable {
            why: "an existing host keeps its resolver",
        },
    ),
    (
        "machines.*.resources.cpus",
        NotApplicable {
            why: "an existing host has its own size",
        },
    ),
    (
        "machines.*.resources.memory_mb",
        NotApplicable {
            why: "an existing host has its own size",
        },
    ),
    (
        "machines.*.resources.disk_gb",
        NotApplicable {
            why: "an existing host has its own size",
        },
    ),
    (
        "machines.*.depends_on",
        Partial {
            how: "provisioned in start order, dependencies first",
            gap: "no wait until the dependencies answer",
            proof: PROBE,
        },
    ),
    (
        "volumes.*.owner",
        Planned {
            note: "an NFS export on the owner, mounts on the others",
        },
    ),
    (
        "machines.*.volumes",
        Planned {
            note: "the path created on the host",
        },
    ),
    (
        "machines.*.access",
        Done {
            how: "checks without `from` run from its runner",
            proof: PROBE,
        },
    ),
    ("machines.*.docker.image", EXISTING_HOST),
    ("machines.*.docker.build", EXISTING_HOST),
    ("machines.*.docker.dockerfile", EXISTING_HOST),
    ("machines.*.docker.args", EXISTING_HOST),
    ("machines.*.docker.init", EXISTING_HOST),
    ("machines.*.docker.appliance", EXISTING_HOST),
    ("machines.*.docker.firmware", EXISTING_HOST),
    ("machines.*.docker.config", EXISTING_HOST),
    ("machines.*.docker.idle", EXISTING_HOST),
    (
        "machines.*.vm.os",
        Partial {
            how: "the inventory's `linux` or `windows` group",
            gap: "Windows hosts skip the machine's own steps",
            proof: "existing-hosts",
        },
    ),
    ("machines.*.vm.image.vagrant", EXISTING_HOST),
    ("machines.*.vm.image.vagrant_version", EXISTING_HOST),
    ("machines.*.vm.image.qemu", EXISTING_HOST),
    ("machines.*.vm.image.hyperv", EXISTING_HOST),
    (
        "machines.*.vm.image.winrm",
        NotApplicable {
            why: "Windows hosts are reached over SSH",
        },
    ),
    (
        "machines.*.vm.provision",
        Partial {
            how: "the project copied to /opt/isoloom over SSH; `.sh` steps run there with sudo, playbooks run from here, `reboot` restarts the host",
            gap: "Windows hosts",
            proof: "existing-hosts",
        },
    ),
    (
        "machines.*.external.address",
        Done {
            how: "`ansible_host` in the inventory and machines.json; SSH for run, test, connect and exec",
            proof: "existing-hosts",
        },
    ),
    (
        "machines.*.external.user",
        Done {
            how: "`ansible_user` and machines.json (default `root`)",
            proof: "existing-hosts",
        },
    ),
    (
        "machines.*.external.port",
        Done {
            how: "`ansible_port` and machines.json (default 22)",
            proof: PROBE,
        },
    ),
    (
        "machines.*.external.key",
        Done {
            how: "`ansible_ssh_private_key_file`, machines.json, `ssh -i`",
            proof: "existing-hosts",
        },
    ),
];
