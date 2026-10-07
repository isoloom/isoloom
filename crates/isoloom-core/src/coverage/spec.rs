//! The spec's side of coverage: for every field of an Isoloom spec, what each output does
//! with it. Its tests keep it honest: every field of the format has a row, and every "done"
//! claim names an example that uses the field and generates for that output.

use std::fmt::Write;

use serde_yaml_ng::Value;

use crate::model::Target;

/// The files Isoloom writes, one column each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// `.isoloom/docker/compose.yml` (targets docker, hosted, cloud-docker).
    Compose,
    /// `.isoloom/vagrant/Vagrantfile` (target vagrant).
    Vagrant,
    /// Terraform for one VM per machine (targets proxmox, cloud-vm).
    Terraform,
}

impl Output {
    pub const ALL: [Output; 3] = [Output::Compose, Output::Vagrant, Output::Terraform];

    pub fn label(self) -> &'static str {
        match self {
            Output::Compose => "Compose",
            Output::Vagrant => "Vagrant",
            Output::Terraform => "Terraform",
        }
    }

    /// The target whose generator writes this output, when one exists.
    pub fn target(self) -> Target {
        match self {
            Output::Compose => Target::Docker,
            Output::Vagrant => Target::Vagrant,
            Output::Terraform => Target::Proxmox,
        }
    }
}

/// What an output does with a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Produced; `proof` is an example that uses the field and generates for this output.
    Done { how: &'static str, proof: &'static str },
    /// Produced, with a known gap.
    Partial {
        how: &'static str,
        gap: &'static str,
        proof: &'static str,
    },
    /// Not produced yet.
    Planned { note: &'static str },
    /// Means nothing for this output (e.g. `vm:` in a Compose file).
    NotApplicable { why: &'static str },
    /// Describes the environment for people and runners; no output uses it.
    Descriptive { note: &'static str },
    /// Read by Isoloom itself (validation, target selection), not by an output.
    Core { note: &'static str },
}

impl Status {
    pub fn symbol(self) -> &'static str {
        match self {
            Status::Done { .. } => "✓",
            Status::Partial { .. } => "◐",
            Status::Planned { .. } => "planned",
            Status::NotApplicable { .. } => "n/a",
            Status::Descriptive { .. } => "info",
            Status::Core { .. } => "core",
        }
    }

    /// Whether the output can be expected to handle the field (done, partial or planned).
    pub fn applies(self) -> bool {
        matches!(self, Status::Done { .. } | Status::Partial { .. } | Status::Planned { .. })
    }

    pub fn note(self) -> String {
        match self {
            Status::Done { how, .. } => how.to_string(),
            Status::Partial { how, gap, .. } => format!("{how}; not yet: {gap}"),
            Status::Planned { note } | Status::NotApplicable { why: note } | Status::Descriptive { note } | Status::Core { note } => note.to_string(),
        }
    }
}

/// One field of the spec (as a path, `*` for names you choose, `[]` for list items) and its
/// status in each output, in [`Output::ALL`] order.
#[derive(Debug, Clone, Copy)]
pub struct Row {
    pub path: &'static str,
    pub outputs: [Status; 3],
}

impl Row {
    pub fn status(&self, o: Output) -> Status {
        self.outputs[Output::ALL.iter().position(|x| *x == o).expect("every output has a column")]
    }
}

const fn done(how: &'static str, proof: &'static str) -> Status {
    Status::Done { how, proof }
}
const fn planned(note: &'static str) -> Status {
    Status::Planned { note }
}
const fn na(why: &'static str) -> Status {
    Status::NotApplicable { why }
}
const VM_PLANNED: Status = planned("with the Proxmox generator");
const NOT_A_CONTAINER: Status = na("VM outputs build machines from `vm:`");
const NOT_A_VM: Status = na("container outputs build machines from `docker:`");

fn all(s: Status) -> [Status; 3] {
    [s; 3]
}

/// The coverage table, in spec-reference order.
pub fn table() -> Vec<Row> {
    let mut rows = base_table();
    // What the Proxmox generator produces (Terraform), each proven by an example.
    let proxmox: &[(&str, Status)] = &[
        ("name", done("tags and VM names (with the slot)", "segmented")),
        ("networks.*.cidr", done("an SDN VNet per network in the environment's own zone", "segmented")),
        (
            "networks.*.internet",
            done("new connections leaving the environment blocked once provisioned", "segmented"),
        ),
        ("reach[].from", done("a router VM filtering with nftables", "segmented")),
        ("reach[].to", done("the router's rules", "segmented")),
        ("reach[].ports", done("the router's rules, per port", "segmented")),
        (
            "machines.*.networks",
            done("a NIC per network with its fixed address (cloud-init), the router as gateway", "segmented"),
        ),
        (
            "machines.*.services[].port",
            done("machines depending on it wait until it answers", "segmented"),
        ),
        ("machines.*.resources.cpus", done("the VM's cores", "segmented")),
        ("machines.*.resources.memory_mb", done("the VM's memory", "segmented")),
        ("machines.*.resources.disk_gb", done("the VM's disk size", "segmented")),
        ("machines.*.depends_on", done("waits until its dependencies answer (cloud-init)", "segmented")),
        (
            "machines.*.vm.os",
            Status::Partial {
                how: "cloud images for debian-12 and ubuntu-24.04",
                gap: "Kali and Windows",
                proof: "segmented",
            },
        ),
        ("machines.*.vm.provision", done("cloud-init writes the project and runs the steps", "segmented")),
        ("machines.*.volumes", done("the VM's own disk keeps the data; the path is created", "segmented")),
    ];
    for (path, status) in proxmox {
        if let Some(r) = rows.iter_mut().find(|r| r.path == *path) {
            r.outputs[2] = *status;
        }
    }
    rows
}

fn base_table() -> Vec<Row> {
    let row = |path, compose, vagrant, terraform| Row {
        path,
        outputs: [compose, vagrant, terraform],
    };
    let vm_field = |path, vagrant| row(path, NOT_A_VM, vagrant, VM_PLANNED);
    let common = |path, compose, vagrant| row(path, compose, vagrant, VM_PLANNED);
    vec![
        Row {
            path: "version",
            outputs: all(Status::Core {
                note: "selects the format version",
            }),
        },
        common(
            "name",
            done("the Compose project name", "hello-stack"),
            done("VM labels and network names", "hello-stack"),
        ),
        common(
            "networks.*.cidr",
            done("a Compose network with that subnet", "hello-stack"),
            done("a private network (VirtualBox internal network, libvirt network)", "hello-stack"),
        ),
        common(
            "networks.*.tc.delay",
            done(
                "netem on the router's interface into the network and each machine's own (its network sidecar)",
                "slow-link",
            ),
            done("netem on the router VM and each Linux VM's interface, re-applied at boot", "slow-link"),
        ),
        common(
            "networks.*.tc.jitter",
            done(
                "netem on the router's interface into the network and each machine's own (its network sidecar)",
                "slow-link",
            ),
            done("netem on the router VM and each Linux VM's interface, re-applied at boot", "slow-link"),
        ),
        common(
            "networks.*.tc.loss",
            done(
                "netem on the router's interface into the network and each machine's own (its network sidecar)",
                "slow-link",
            ),
            done("netem on the router VM and each Linux VM's interface, re-applied at boot", "slow-link"),
        ),
        common(
            "networks.*.tc.rate",
            done(
                "netem on the router's interface into the network and each machine's own (its network sidecar)",
                "slow-link",
            ),
            done("netem on the router VM and each Linux VM's interface, re-applied at boot", "slow-link"),
        ),
        row(
            "networks.*.docker.cidr",
            done(
                "the network's block on Docker (others outside 10.0.0.0/8 move there automatically)",
                "air-gapped",
            ),
            na("Docker only: VMs use `cidr` as written"),
            na("Docker only: VMs use `cidr` as written"),
        ),
        row(
            "networks.*.vlans.*.cidr",
            done(
                "a Compose network per VLAN (`<lan>-vlan<id>`); a machine on several VLANs of the LAN gets one 802.1Q trunk (`<lan>.<id>` subinterfaces) through a switch container",
                "vlan-office",
            ),
            done("a private network per VLAN", "vlan-office"),
            done("an SDN VNet per VLAN", "vlan-office"),
        ),
        row(
            "networks.*.vlans.*.internet",
            done("as `networks.*.internet`, per VLAN (the LAN's by default)", "vlan-office"),
            done("as `networks.*.internet`, per VLAN (the LAN's by default)", "vlan-office"),
            done("as `networks.*.internet`, per VLAN (the LAN's by default)", "vlan-office"),
        ),
        common(
            "networks.*.internet",
            done(
                "no default route for offline machines (Docker's internal network when nothing routes)",
                "segmented",
            ),
            done("new connections out through the NAT interface blocked after provisioning", "segmented"),
        ),
        common(
            "networks.*.gateway",
            done(
                "the gateway machine at .1 with forwarding; machines behind it route through it",
                "edge-firewall",
            ),
            done("forwarding on the gateway; machines move behind it after provisioning", "edge-firewall"),
        ),
        common(
            "reach[].from",
            done("a router container filtering with nftables", "segmented"),
            done("a router VM filtering with nftables", "segmented"),
        ),
        common("reach[].to", done("the router's rules", "segmented"), done("the router's rules", "segmented")),
        common(
            "reach[].ports",
            done("the router's rules, per port", "segmented"),
            done("the router's rules, per port", "segmented"),
        ),
        common(
            "inputs",
            done("variables read from the shell at start", "supplier-portal-api"),
            done("values read from the environment at start", "supplier-portal-api"),
        ),
        common(
            "machines.*.networks",
            done("a fixed address on each network; names across networks", "hello-stack"),
            done("a fixed address on each network; names in /etc/hosts", "hello-stack"),
        ),
        common(
            "machines.*.services[].port",
            done("a healthcheck, so `depends_on` and `up --wait` wait for it", "hello-stack"),
            done("machines depending on it wait until it answers", "hello-stack"),
        ),
        Row {
            path: "machines.*.services[].name",
            outputs: all(Status::Descriptive {
                note: "names the service for people and runners",
            }),
        },
        common(
            "machines.*.services[].publish",
            done("a published port, on the host's loopback", "supplier-portal-api"),
            done("a forwarded port, on the host's loopback", "supplier-portal-api"),
        ),
        Row {
            path: "machines.*.services[].http",
            outputs: all(Status::Descriptive {
                note: "tells runners they can open it in a browser",
            }),
        },
        common(
            "machines.*.inputs",
            done("environment of that machine and its init jobs only", "supplier-portal-api"),
            done("environment of that machine's provisioning only", "supplier-portal-api"),
        ),
        common(
            "machines.*.arch",
            done("`platform` on the container and a node selector on Kubernetes (amd64/arm64)", "arm-lab"),
            done("`box_architecture` on the VM", "segmented"),
        ),
        common(
            "machines.*.privileged",
            done("the container runs privileged (and a Kubernetes securityContext)", "arm-lab"),
            na("a VM already has kernel access: its workload runs as root"),
        ),
        common(
            "machines.*.read_only",
            done("a read-only root filesystem on the container (and a Kubernetes securityContext)", "arm-lab"),
            na("a VM's root filesystem stays writable; use `volumes` for data that must persist"),
        ),
        common(
            "machines.*.tmpfs",
            done("memory-backed mounts on the container (and Memory emptyDirs on Kubernetes)", "arm-lab"),
            na("a VM mounts tmpfs in its own provisioning (fstab)"),
        ),
        common(
            "machines.*.shm_size",
            done("the size of /dev/shm on the container (and a Memory emptyDir on Kubernetes)", "arm-lab"),
            na("a VM sizes /dev/shm in its own provisioning"),
        ),
        common(
            "machines.*.dns.servers",
            done("`dns` on the container (and the Kubernetes dnsConfig nameservers)", "arm-lab"),
            na("a VM sets its resolver in its own provisioning (resolv.conf)"),
        ),
        common(
            "machines.*.dns.search",
            done("`dns_search` on the container (and the Kubernetes dnsConfig searches)", "arm-lab"),
            na("a VM sets its search domains in its own provisioning"),
        ),
        common(
            "machines.*.dns.domain",
            done("`domainname` on the container", "arm-lab"),
            na("a VM sets its domain in its own provisioning"),
        ),
        common(
            "machines.*.resources.cpus",
            done("a CPU limit", "supplier-portal-api"),
            done("the VM's CPUs on every provider", "supplier-portal-api"),
        ),
        common(
            "machines.*.resources.memory_mb",
            done("a memory limit", "supplier-portal-api"),
            done("the VM's memory on every provider", "supplier-portal-api"),
        ),
        common(
            "machines.*.resources.disk_gb",
            na("containers share the host's disk"),
            planned("the box's disk size is used as is"),
        ),
        common(
            "machines.*.depends_on",
            done("starts after its dependencies answer (and their init jobs finish)", "hello-stack"),
            done("boots after its dependencies, then waits until they answer", "hello-stack"),
        ),
        common(
            "machines.*.volumes",
            done("a named volume per path", "hello-stack"),
            done("the VM's own disk keeps the data; the path is created", "hello-stack"),
        ),
        common(
            "machines.*.access",
            done("checks run from its network namespace (a stand-in when the runner supplies it)", "segmented"),
            Status::Partial {
                how: "a VM like the others when it has `vm:`",
                gap: "checks don't run from it",
                proof: "segmented",
            },
        ),
        row(
            "machines.*.docker.image",
            done("the service's image", "hello-stack"),
            NOT_A_CONTAINER,
            NOT_A_CONTAINER,
        ),
        row(
            "machines.*.docker.build",
            done("a build from the project folder", "segmented"),
            NOT_A_CONTAINER,
            NOT_A_CONTAINER,
        ),
        row(
            "machines.*.docker.init",
            done("one-shot jobs in its network namespace, after it answers", "hello-stack"),
            NOT_A_CONTAINER,
            NOT_A_CONTAINER,
        ),
        row(
            "machines.*.docker.idle",
            done("kept running idle (`entrypoint: [sleep, infinity]`), a machine to work from", "workbench"),
            NOT_A_CONTAINER,
            NOT_A_CONTAINER,
        ),
        vm_field(
            "machines.*.vm.os",
            Status::Partial {
                how: "Vagrant boxes for debian-12, ubuntu-24.04 and kali",
                gap: "Windows",
                proof: "hello-stack",
            },
        ),
        vm_field(
            "machines.*.vm.image.vagrant",
            done("the machine's own Vagrant box instead of the built-in one", "windows-hello"),
        ),
        vm_field("machines.*.vm.image.vagrant_version", done("the box version, pinned", "windows-hello")),
        vm_field(
            "machines.*.vm.image.winrm",
            done(
                "how the Windows box answers WinRM: `plaintext` (HTTP 5985, the default) or `ssl` (HTTPS 5986)",
                "windows-hello",
            ),
        ),
        vm_field("machines.*.vm.provision", done("`.sh` steps, and Ansible run inside the VM", "hello-stack")),
        common(
            "provision[].ansible",
            planned("environment-level provisioning on containers (a controller container)"),
            done("run from a controller VM on every network, once every machine is up", "ansible-pair"),
        ),
        common(
            "provision[].inventory",
            planned("with environment-level provisioning on containers"),
            done("more inventory files, next to the one Isoloom writes", "ansible-pair"),
        ),
        common(
            "provision[].groups",
            planned("with environment-level provisioning on containers"),
            done("groups in the inventory Isoloom writes", "ansible-pair"),
        ),
        common(
            "provision[].vars",
            planned("with environment-level provisioning on containers"),
            done("extra variables (`-e`)", "ansible-pair"),
        ),
        common(
            "provision[].requirements",
            planned("with environment-level provisioning on containers"),
            done("Galaxy collections and roles installed first", "ansible-pair"),
        ),
        common(
            "checks[]",
            done(
                "a `check` profile: a runner per position (a sh script next to the Compose file), scripts from the project mounted read-only",
                "hello-stack",
            ),
            done(
                "on demand (`vagrant provision --provision-with checks`): a runner script per machine, the controller's for the rest and for Ansible checks",
                "hello-stack",
            ),
        ),
        common(
            "checks[].name",
            done("the runner's PASS/FAIL line", "hello-stack"),
            done("the runner's PASS/FAIL line", "hello-stack"),
        ),
        common(
            "checks[].from",
            done(
                "the runner in that machine's network namespace (a stand-in's when the runner supplies it)",
                "segmented",
            ),
            done("the runner on that machine", "segmented"),
        ),
        common(
            "checks[].http",
            done("curl (else wget, else bash) from the position, retried for `wait`", "hello-stack"),
            done("curl (else wget, else bash) from the machine, retried for `wait`", "hello-stack"),
        ),
        common(
            "checks[].tcp",
            done("nc (else bash) from the position", "segmented"),
            done("nc (else bash) from the machine", "segmented"),
        ),
        row(
            "checks[].exec",
            planned("the runner shares the machine's network, not its filesystem"),
            done("the command in the machine's own shell", "arm-vm"),
            VM_PLANNED,
        ),
        common(
            "checks[].script",
            done("sh, from the project mounted read-only", "edge-firewall"),
            done("sh, from /opt/isoloom", "edge-firewall"),
        ),
        common(
            "checks[].expect",
            done(
                "a status code, `any` or `blocked` (http); `open` or `blocked` (tcp); text (exec)",
                "hello-stack",
            ),
            done(
                "a status code, `any` or `blocked` (http); `open` or `blocked` (tcp); text (exec)",
                "hello-stack",
            ),
        ),
        common(
            "checks[].wait",
            done("retried every 2s until it passes or the time is up", "segmented"),
            done("retried every 2s until it passes or the time is up", "segmented"),
        ),
        Row {
            path: "targets",
            outputs: all(Status::Core {
                note: "narrows the targets Isoloom generates",
            }),
        },
        common(
            "tools",
            done(
                "a container per tool on every network at the reserved addresses (`shell`: netshoot)",
                "slow-link",
            ),
            done(
                "the `shell` tool as a Debian VM with the usual tools; image tools are container-only",
                "slow-link",
            ),
        ),
        Row {
            path: "machines.*.external.address",
            outputs: all(Status::Core {
                note: "the `external` target's SSH endpoint (inventory, checks, connect)",
            }),
        },
        Row {
            path: "machines.*.external.user",
            outputs: all(Status::Core {
                note: "the `external` target's SSH user",
            }),
        },
        Row {
            path: "machines.*.external.port",
            outputs: all(Status::Core {
                note: "the `external` target's SSH port",
            }),
        },
        Row {
            path: "machines.*.external.key",
            outputs: all(Status::Core {
                note: "the `external` target's SSH key",
            }),
        },
        Row {
            path: "message",
            outputs: all(Status::Descriptive {
                note: "printed by `isoloom run` and `isoloom message`, placeholders filled from the snapshot",
            }),
        },
        Row {
            path: "machines.*.count",
            outputs: all(Status::Core {
                note: "expanded into clones before generation",
            }),
        },
        Row {
            path: "common",
            outputs: all(Status::Core {
                note: "folded into every machine before generation",
            }),
        },
        Row {
            path: "groups",
            outputs: [
                Status::Core {
                    note: "folded into their members before generation",
                },
                done("folded into their members; Ansible inventory groups", "ansible-pair"),
                done("folded into their members; Ansible inventory groups", "ansible-pair"),
            ],
        },
    ]
}

/// Fields counted as one feature whatever they hold.
const WHOLE: &[&str] = &["common", "groups", "tools"];

/// Where in the spec names are chosen by the author (map keys become `*`).
const NAMED: &[&str] = &[
    "networks",
    "networks.*.vlans",
    "machines",
    "machines.*.networks",
    "machines.*.volumes",
    "provision[].groups",
    "provision[].vars",
];

/// The field paths present in a spec document (`machines.*.docker.image`, `reach[].ports`).
/// A map of plain values under author-chosen names (`machines.*.networks`) counts as one field.
pub fn paths(doc: &Value) -> Vec<String> {
    fn walk(v: &Value, at: &str, out: &mut Vec<String>) {
        // Shared machine fields are one feature, however many fields they carry.
        if WHOLE.contains(&at) {
            out.push(at.to_string());
            return;
        }
        match v {
            Value::Mapping(m) if NAMED.contains(&at) => {
                if m.values().all(|c| !matches!(c, Value::Mapping(_))) {
                    out.push(at.to_string());
                } else {
                    for c in m.values() {
                        walk(c, &format!("{at}.*"), out);
                    }
                }
            }
            Value::Mapping(m) => {
                for (k, c) in m {
                    let k = k.as_str().unwrap_or_default();
                    walk(c, &if at.is_empty() { k.to_string() } else { format!("{at}.{k}") }, out);
                }
            }
            Value::Sequence(items) if items.iter().any(|i| matches!(i, Value::Mapping(_))) => {
                for i in items {
                    walk(i, &format!("{at}[]"), out);
                }
            }
            _ => out.push(at.to_string()),
        }
    }
    let mut out = Vec::new();
    walk(doc, "", &mut out);
    out.sort();
    out.dedup();
    out
}

/// The table as a Markdown section: a grid, then what each generated output does, field by field.
pub fn section() -> String {
    let rows = table();
    let mut md = String::from(
        "## From the spec's side\n\nThe other direction: every field of an Isoloom spec, and what each output does with it.\n\n✓ done · ◐ partial · planned · n/a: means nothing for this output · info: descriptive only · core: read by Isoloom itself\n\n",
    );
    md.push_str("| Field |");
    for o in Output::ALL {
        let _ = write!(md, " {} |", o.label());
    }
    md.push_str("\n| --- |");
    for _ in Output::ALL {
        md.push_str(" :---: |");
    }
    md.push('\n');
    for r in &rows {
        let _ = write!(md, "| `{}` |", r.path);
        for o in Output::ALL {
            let _ = write!(md, " {} |", r.status(o).symbol());
        }
        md.push('\n');
    }
    md.push_str("\n| Output | Fields handled |\n| --- | --- |\n");
    for o in Output::ALL {
        let _ = writeln!(md, "| {} | {} |", o.label(), score(&rows, o));
    }
    md
}

/// "done of applicable" for an output, partial counting as half.
pub fn score(rows: &[Row], o: Output) -> String {
    let applicable = rows.iter().filter(|r| r.status(o).applies()).count();
    let done = rows.iter().filter(|r| matches!(r.status(o), Status::Done { .. })).count();
    let partial = rows.iter().filter(|r| matches!(r.status(o), Status::Partial { .. })).count();
    if partial == 0 {
        format!("{done}/{applicable}")
    } else {
        format!("{done}/{applicable} (+{partial} partial)")
    }
}
