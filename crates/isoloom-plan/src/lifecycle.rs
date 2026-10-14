//! A VM's set-up, as an ordered list of steps every VM backend runs the same way. The plan says
//! what each step does; `isoloom-render`'s `steps` module writes each one as Linux shell (and
//! PowerShell where Windows has it); a backend only picks the transport (a Vagrant provisioner,
//! cloud-init, `remote-exec`, WinRM).

use std::net::Ipv4Addr;

use isoloom_spec::model::{Machine, Spec, is_reboot};

use crate::addressing::address_for;
use crate::environment::Route;
use crate::routing;

/// A machine's set-up steps, in order.
pub type MachineLifecycle = Vec<Step>;

/// One set-up step of a VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The spec's constraints: proxy, package mirrors, NTP (before anything downloads).
    Prelude,
    /// Grows the root file system to the disk the machine was given.
    DiskGrow,
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
    /// 802.1Q interfaces for the VLANs it reaches through a trunk.
    Trunks,
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
    /// The inputs made available to the steps.
    Inputs {
        /// Kept on disk (a reboot comes and `/tmp` is emptied at boot).
        persist: bool,
    },
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
    /// Offline: no new connection leaves the environment.
    Egress,
    /// Link impairment on its own interfaces.
    Tc,
    /// Marks the machine ready.
    Ready,
}

/// A machine's part in a shared volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedRole {
    /// It holds the data and serves it to these addresses.
    Serve {
        /// The addresses of the machines mounting it.
        clients: Vec<Ipv4Addr>,
    },
    /// It mounts the owner's export.
    Mount {
        /// `<owner address>:<owner path>`.
        source: String,
        /// The owner, named in errors.
        owner: String,
    },
}

/// How a `provision` step runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    /// A shell script.
    Shell,
    /// An Ansible playbook, run locally.
    Playbook,
}

/// The set-up steps of VM `name`.
pub fn of(spec: &Spec, name: &str, m: &Machine) -> MachineLifecycle {
    let mut steps = vec![Step::Prelude];
    if m.resources.and_then(|r| r.disk_gb).is_some() {
        steps.push(Step::DiskGrow);
    }
    steps.push(Step::Project);
    steps.push(hosts(spec, name));
    steps.push(Step::Settings);
    steps.extend(shared_volumes(spec, name, m));
    steps.push(Step::Trunks);
    steps.push(Step::Routes {
        routes: routing::routes(spec, name, m).into_iter().map(|(cidr, via)| Route { cidr, via }).collect(),
        default: routing::default_gateway(spec, name, m),
    });
    let own: Vec<String> = m
        .volumes
        .iter()
        .filter(|(v, _)| !spec.volumes.contains_key(*v))
        .map(|(_, p)| p.clone())
        .collect();
    if !own.is_empty() {
        steps.push(Step::Volumes { paths: own });
    }
    steps.extend(m.depends_on.iter().filter_map(|dep| {
        let ports = spec.machines[dep].ready_ports();
        (!ports.is_empty()).then(|| Step::WaitFor { machine: dep.clone(), ports })
    }));
    steps.extend(runs(m));
    if routing::offline(spec, name, m) {
        steps.push(Step::Egress);
    }
    if m.networks.keys().any(|n| spec.networks[n].tc.is_some()) {
        steps.push(Step::Tc);
    }
    steps.push(Step::Ready);
    steps
}

fn hosts(spec: &Spec, name: &str) -> Step {
    let entries = spec
        .machines
        .iter()
        .filter(|(o, om)| o.as_str() != name && !om.networks.is_empty())
        .map(|(o, om)| {
            let names = std::iter::once(o.clone()).chain(om.aliases.iter().cloned()).collect();
            (address_for(spec, name, o), names)
        })
        .collect();
    Step::Hosts { entries }
}

fn shared_volumes(spec: &Spec, name: &str, m: &Machine) -> Vec<Step> {
    m.volumes
        .iter()
        .filter(|(v, _)| spec.volumes.contains_key(*v))
        .filter_map(|(v, path)| {
            let owner = spec.volume_owner(v)?;
            let role = if owner == name {
                let clients = spec
                    .volume_users(v)
                    .into_iter()
                    .filter(|u| *u != name)
                    .map(|u| address_for(spec, name, u))
                    .collect();
                SharedRole::Serve { clients }
            } else {
                SharedRole::Mount {
                    source: format!("{}:{}", address_for(spec, name, owner), spec.machines[owner].volumes[v]),
                    owner: owner.to_string(),
                }
            };
            Some(Step::SharedVolume {
                volume: v.clone(),
                path: path.clone(),
                role,
            })
        })
        .collect()
}

fn runs(m: &Machine) -> Vec<Step> {
    let Some(vm) = &m.vm else { return Vec::new() };
    let reboots = vm.provision.iter().any(|s| is_reboot(s));
    let mut steps = Vec::new();
    if !m.inputs.is_empty() {
        steps.push(Step::Inputs { persist: reboots });
    }
    steps.extend(vm.provision.iter().map(|s| {
        if is_reboot(s) {
            Step::Reboot
        } else {
            let kind = if s.ends_with(".sh") { RunKind::Shell } else { RunKind::Playbook };
            Step::Run { path: s.clone(), kind }
        }
    }));
    steps
}
