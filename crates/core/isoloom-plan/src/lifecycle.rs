//! A VM's set-up, as an ordered list of steps every VM backend runs the same way. The plan says
//! what each step does; `isoloom-render`'s `steps` module writes each one as Linux shell (and
//! PowerShell where Windows has it); a backend only picks the transport (a Vagrant provisioner,
//! cloud-init, `remote-exec`, WinRM).

use std::net::{Ipv4Addr, Ipv6Addr};

use isoloom_spec::model::{Machine, MachineFile, Spec, is_reboot};

use crate::addressing::address_for;
use crate::environment::Route;
use crate::ipv6::{self, Route6};
use crate::routing;
use crate::storage::DataDisk;

/// A machine's set-up steps, in order.
pub type MachineLifecycle = Vec<Step>;

/// One set-up step of a VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The spec's constraints: proxy, package mirrors, NTP (before anything downloads).
    Prelude,
    /// Grows the root file system to the disk the machine was given.
    DiskGrow,
    /// Formats its data disks (once, when blank) and mounts them, at every boot.
    Disks {
        /// The disks, in spec order (a raw one is left as it is).
        disks: Vec<DataDisk>,
    },
    /// The machine's environment, sysctls and ulimits.
    Settings,
    /// The other machines by name, at the address this one reaches each at.
    Hosts {
        /// One `(address, names)` per other machine, its own name first.
        entries: Vec<(Ipv4Addr, Vec<String>)>,
    },
    /// Routes to the networks it isn't on, then its default route through a gateway.
    Routes {
        /// The routes.
        routes: Vec<Route>,
        /// The default route's next hop.
        default: Option<Ipv4Addr>,
    },
    /// Its IPv6 addresses, each on the interface holding its IPv4 address on that network,
    /// then its IPv6 routes through the router.
    Ipv6 {
        /// One `(IPv4 address, IPv6 address)` per IPv6 network: the IPv4 one finds the interface.
        addresses: Vec<(Ipv4Addr, Ipv6Addr)>,
        /// The routes.
        routes: Vec<Route6>,
    },
    /// 802.1Q interfaces for the VLANs it reaches through a trunk.
    Trunks,
    /// Its further addresses on a network (`lan: [10, 11]`), each on the interface holding its
    /// primary address there.
    MoreAddresses {
        /// One entry per network where it has several.
        addresses: Vec<crate::more_addresses::MoreAddresses>,
    },
    /// The directories of its own volumes.
    Volumes {
        /// The mount paths.
        paths: Vec<String>,
    },
    /// A shared volume: served over NFS by its owner, mounted by the others.
    SharedVolume {
        /// The volume's name.
        volume: String,
        /// Where this machine has it.
        path: String,
        /// This machine's part.
        role: SharedRole,
    },
    /// The project copied to `/opt/isoloom`.
    Project,
    /// The machine's `files`, each placed at its path with its owner and mode (a `source` read
    /// from the unpacked project).
    Files {
        /// The files, in spec order.
        files: Vec<MachineFile>,
    },
    /// The inputs made available to the steps.
    Inputs {
        /// Kept on disk (a reboot comes and `/tmp` is emptied at boot).
        persist: bool,
    },
    /// Waits for `isoloom run` to deliver its secrets, then puts each file in place, now and at
    /// every boot (see [`crate::SecretFile`]). Before anything that may read one: the
    /// app's steps, its command and its `setup` scripts.
    Secrets,
    /// Waits until a machine it depends on answers.
    WaitFor {
        /// The machine.
        machine: String,
        /// The ports it must answer on.
        ports: Vec<u16>,
    },
    /// One of its `provision` steps.
    Run {
        /// The script or playbook, from the project folder.
        path: String,
        /// How it runs.
        kind: RunKind,
    },
    /// Restarts the machine; the steps after it run once it is back.
    Reboot,
    /// Its own command, run as a service from now on (see [`crate::Process`]).
    Command,
    /// Its `setup` scripts, run once with its service up, as root from the project folder.
    Setup {
        /// The scripts, from the project folder.
        paths: Vec<String>,
    },
    /// Offline: no new connection leaves the environment.
    Egress,
    /// Link impairment on its own interfaces.
    Tc,
    /// Marks the machine ready, once its healthcheck passes when it has one.
    Ready,
}

/// A machine's part in a shared volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedRole {
    /// It holds the data and serves it to these addresses.
    Serve {
        /// The addresses of the machines mounting it.
        clients: Vec<Ipv4Addr>,
        /// The folders other machines take of it (`subpath`), created when missing.
        folders: Vec<String>,
    },
    /// It mounts the owner's export.
    Mount {
        /// `<owner address>:<owner path>`, then `/<subpath>` when it takes one folder.
        source: String,
        /// The owner, named in errors.
        owner: String,
    },
    /// It mounts a folder of the server the target shares into the VM (virtiofs on Proxmox),
    /// as every other machine using the volume does: no owner serves it. The plan never makes
    /// this role; a target that shares the volume so puts it in place of the other two.
    HostFolder {
        /// A file the target writes in the machine, holding the folder's mount tag (the tag is
        /// only known once the target's own names are, a slot in them).
        tag_file: String,
        /// The folder of the volume it takes (`subpath`), if not all of it.
        folder: Option<String>,
    },
}

/// How a `provision` step runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    /// A shell script.
    Shell,
    /// An Ansible playbook, run locally.
    Playbook,
    /// A PowerShell script (a Windows machine's).
    PowerShell,
}

/// The set-up steps of VM `name`.
#[must_use]
pub fn of(spec: &Spec, name: &str, m: &Machine) -> MachineLifecycle {
    let mut steps = vec![Step::Prelude];
    if m.resources.and_then(|r| r.disk_gb).is_some() {
        steps.push(Step::DiskGrow);
    }
    if !m.disks.is_empty() {
        steps.push(Step::Disks {
            disks: crate::storage::disks(name, m),
        });
    }
    steps.push(Step::Project);
    steps.push(hosts(spec, name));
    steps.push(Step::Settings);
    steps.extend(shared_volumes(spec, name, m));
    steps.push(Step::Trunks);
    steps.extend(crate::more_addresses::step(spec, m));
    steps.push(Step::Routes {
        routes: routing::routes(spec, name, m).into_iter().map(|(cidr, via)| Route { cidr, via }).collect(),
        default: routing::default_gateway(spec, name, m),
    });
    steps.extend(ipv6_step(spec, m));
    let own: Vec<String> = m
        .volumes
        .iter()
        .filter(|(v, _)| !spec.volumes.contains_key(*v))
        .map(|(_, p)| p.path().to_owned())
        .collect();
    if !own.is_empty() {
        steps.push(Step::Volumes { paths: own });
    }
    steps.extend(crate::machine_files::step(m));
    if !m.secrets.is_empty() {
        steps.push(Step::Secrets);
    }
    steps.extend(m.depends_on.iter().filter_map(|dep| {
        let ports = spec.machines.get(dep)?.ready_ports();
        (!ports.is_empty()).then(|| Step::WaitFor { machine: dep.clone(), ports })
    }));
    steps.extend(runs(m));
    if m.command.is_some() {
        steps.push(Step::Command);
    }
    if let Some(vm) = m.vm.as_ref().filter(|vm| !vm.setup.is_empty()) {
        steps.push(Step::Setup { paths: vm.setup.clone() });
    }
    if routing::offline(spec, name, m) {
        steps.push(Step::Egress);
    }
    if m.networks.keys().any(|n| spec.networks.get(n).is_some_and(|net| net.tc.is_some())) {
        steps.push(Step::Tc);
    }
    steps.push(Step::Ready);
    steps
}

/// Its IPv6 addresses and routes, when it has any.
fn ipv6_step(spec: &Spec, m: &Machine) -> Option<Step> {
    let addresses: Vec<(Ipv4Addr, Ipv6Addr)> = m
        .networks
        .iter()
        .filter_map(|(n, o)| Some((crate::addressing::address(spec, n, *o), ipv6::address(spec, n, *o)?)))
        .collect();
    let routes = ipv6::routes(spec, m);
    (!addresses.is_empty() || !routes.is_empty()).then_some(Step::Ipv6 { addresses, routes })
}

/// The other machines by name (and under the private zone's domain, with `dns.private`). A Linux machine leaves the Windows ones out: in a domain they are
/// reached through AD DNS, and a short name in /etc/hosts shadows it (a realm join then fails
/// the Kerberos SPN lookup, "Server not found").
fn hosts(spec: &Spec, name: &str) -> Step {
    let linux = !spec.machines.get(name).is_some_and(windows);
    let entries = spec
        .machines
        .iter()
        .filter(|(o, om)| o.as_str() != name && !om.networks.is_empty() && !(linux && windows(om)))
        .map(|(o, om)| {
            let names = std::iter::once(o.clone()).chain(om.aliases.iter().cloned()).collect();
            (address_for(spec, name, o), crate::private_dns::with_domain(spec, names))
        })
        .collect();
    Step::Hosts {
        entries: [entries, more_addresses(spec, name, linux)].concat(),
    }
}

/// The other machines' further addresses (`lan: [10, 11]`), after every primary one, under
/// their machine's own name: a reverse lookup names the machine, and a lookup by name still
/// finds its primary address first.
fn more_addresses(spec: &Spec, name: &str, linux: bool) -> Vec<(Ipv4Addr, Vec<String>)> {
    spec.machines
        .iter()
        .filter(|(o, om)| o.as_str() != name && !(linux && windows(om)))
        .flat_map(|(o, om)| {
            let names = crate::private_dns::with_domain(spec, vec![o.clone()]);
            crate::more_addresses::of(spec, om).into_values().flatten().map(move |a| (a, names.clone()))
        })
        .collect()
}

/// The machine's part in each shared volume it mounts: its owner serves it, the others mount
/// the owner's export (validated: the owner mounts the volume too).
fn shared_volumes(spec: &Spec, name: &str, m: &Machine) -> Vec<Step> {
    let mount_of = |machine: &str, v: &str| spec.machines.get(machine).and_then(|om| om.volumes.get(v));
    m.volumes
        .iter()
        .filter(|(v, _)| spec.volumes.contains_key(*v))
        .filter_map(|(v, mount)| {
            let owner = spec.volume_owner(v)?;
            let role = if owner == name {
                let others: Vec<&str> = spec.volume_users(v).into_iter().filter(|u| *u != name).collect();
                let mut folders: Vec<String> = others.iter().filter_map(|u| mount_of(u, v)?.subpath().map(String::from)).collect();
                folders.dedup();
                SharedRole::Serve {
                    clients: others.iter().map(|u| address_for(spec, name, u)).collect(),
                    folders,
                }
            } else {
                let at = mount_of(owner, v)?.path().trim_end_matches('/');
                let folder = mount.subpath().map(|f| format!("/{f}")).unwrap_or_default();
                SharedRole::Mount {
                    source: format!("{}:{at}{folder}", address_for(spec, name, owner)),
                    owner: owner.to_owned(),
                }
            };
            Some(Step::SharedVolume {
                volume: v.clone(),
                path: mount.path().to_owned(),
                role,
            })
        })
        .collect()
}

fn runs(m: &Machine) -> Vec<Step> {
    let Some(vm) = &m.vm else { return Vec::new() };
    // Kept on disk when a reboot comes, and for the command, which reads
    // them again at every boot.
    let persist = vm.provision.iter().any(|s| is_reboot(s)) || m.command.is_some();
    let mut steps = Vec::new();
    if !m.inputs.is_empty() {
        steps.push(Step::Inputs { persist });
    }
    steps.extend(vm.provision.iter().map(|s| {
        if is_reboot(s) {
            Step::Reboot
        } else {
            let kind = if has_extension(s, "sh") {
                RunKind::Shell
            } else if has_extension(s, "ps1") {
                RunKind::PowerShell
            } else {
                RunKind::Playbook
            };
            Step::Run { path: s.clone(), kind }
        }
    }));
    steps
}

/// Whether a provisioning step's file has this extension, in any case (`setup.SH` is a shell
/// script): how the plan and the backends tell a step's kind.
#[must_use]
pub fn has_extension(path: &str, extension: &str) -> bool {
    std::path::Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case(extension))
}

/// Whether a machine is a Windows VM.
fn windows(m: &Machine) -> bool {
    m.vm.as_ref().is_some_and(|vm| isoloom_spec::images::is_windows(&vm.os))
}

#[cfg(test)]
mod tests;
