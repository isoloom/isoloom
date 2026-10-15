//! The `vagrant` target: one VM per machine, in `.isoloom/vagrant/Vagrantfile`, rendered from
//! the plan ([`Environment`]).
//!
//! - Each network is a private network: a VirtualBox internal network (any address, isolated
//!   from the host's LAN) or a libvirt network, with the machine's fixed address on it.
//! - A VM's set-up is its planned steps (`isoloom_plan::lifecycle`), each rendered once by
//!   `isoloom_render::steps` and run as a `vm.provision` ([`transport`]): the project copied
//!   to `/opt/isoloom` by Isoloom's own provisioner (no shared folders, works on every host
//!   OS), the other machines' names, settings, routes, waits for dependencies, the `provision`
//!   steps (`.sh` with sh, `.yml` / `.yaml` with Ansible inside the VM, `reboot` with Vagrant's
//!   own restart), egress rules, readiness.
//! - Machines start in dependency order; Vagrant boots and provisions them one after the
//!   other, so a machine's services are installed before the machines depending on it start.
//! - Inputs are read from the environment (empty when unset), passed only to the machines
//!   that list them.
//! - On QEMU (vagrant-qemu), a machine boots its libvirt-format box with its own CPU, emulated
//!   when the host's is another (x86 Windows on an Apple Silicon Mac: slow, so longer timeouts).
//!   Without root, two VMs on a network link as a socket listen/connect pair, and any other
//!   number through a hub (a QEMU process with no machine), each VM on one network; the QEMU
//!   provider's refusal ([`provider`]) says when a spec doesn't fit. A disk size grows the
//!   box's disk, never below the box's own.
//! - On UTM (`vagrant_utm`), a VM gets no private network; the UTM provider's refusal
//!   ([`provider`]) refuses a spec whose VMs are on one.
//! - On Hyper-V, Vagrant puts a VM's adapters on one switch and sets no address: each network
//!   is a private switch, each VM an adapter there with a fixed MAC, its address set in the
//!   guest ([`hyperv`]); the Hyper-V provider's refusal ([`provider`]) says when a machine has no
//!   Hyper-V box.

mod boot;
mod bridged;
mod checks;
mod hyperv;
pub(crate) mod limits;
mod machine;
mod nested;
mod own;
pub(crate) mod provider;
mod providers;
mod qemu;
mod transport;
mod windows;

/// The name of a network on the host, `isoloom-<environment>-<network>`: its VirtualBox
/// internal network, its libvirt network and its Hyper-V private switch.
#[must_use]
pub fn network_name(environment: &str, network: &str) -> String {
    format!("isoloom-{environment}-{network}")
}

use indexmap::IndexMap;
use isoloom_plan::{Controller, Environment, MachineShape};
use isoloom_render::backend::Files;
use isoloom_render::{GeneratedFile, OUTPUT_DIR, header, powershell};
use isoloom_spec::model::{Spec, Target};

use crate::backend::capabilities;
use crate::error::{Error, Result};
use crate::ruby::{Call, Ruby, Value};
use crate::vagrantfile;

/// The folder of the target's files, under [`OUTPUT_DIR`].
const DIR: &str = "vagrant";
/// The define names of Isoloom's own VMs (the router's is `router::NAME`).
const CONTROLLER: &str = "isoloom-controller";
const TOOL_SHELL: &str = "isoloom-tool-shell";
/// Whether the host is an ARM one, in Ruby.
const ARM_HOST: &str = "RbConfig::CONFIG[\"host_cpu\"] =~ /arm|aarch64/";

/// The Ruby that copies the project into a VM (`vagrant_project.rb`): a tar.gz built when the
/// step runs, symlinks kept as links, by Isoloom's own provisioner `isoloom_project`; on Windows
/// unpacked by PowerShell (`untar.ps1`, as `IsoloomProject::UNTAR_PS1`). Every Vagrantfile
/// Isoloom writes carries it, after `ROOT`.
pub(crate) fn project_rb() -> String {
    format!(
        "{}\n# Unpacks the project on Windows (no tar.exe before Windows Server 2019).\nIsoloomProject::UNTAR_PS1 = <<'PS1'\n{}PS1\n",
        include_str!("vagrant_project.rb"),
        powershell::UNTAR
    )
}

/// Where a Vagrantfile goes and what it carries besides the environment's VMs: `hybrid` writes
/// its VMs to `.isoloom/hybrid/`, with its Docker host as one more `config.vm.define` block.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Layout<'a> {
    /// The folder under `.isoloom/`.
    pub(crate) dir: &'a str,
    /// Ruby added at the end of the `configure` block (complete statements, indented for it).
    pub(crate) extra: &'a str,
}

/// What rendering the VMs needs at hand.
struct Ctx<'a> {
    env: &'a Environment,
    /// The output folder under `.isoloom/`.
    dir: &'a str,
    /// Each VM's link under QEMU.
    links: &'a IndexMap<String, qemu::Link>,
    /// The checks, by where they run.
    checks: &'a checks::Checks<'a>,
}

/// The Vagrantfile of `env` and the check runners it uploads.
pub(crate) fn render(env: &Environment) -> Result<Files> {
    render_in(env, &Layout { dir: DIR, extra: "" })
}

/// The VMs of `spec` (the `vagrant` target's plan of it) into `layout`: the `hybrid` target's.
pub(crate) fn render_spec_in(spec: &Spec, layout: &Layout) -> Result<Files> {
    render_in(&Environment::build(spec, Target::Vagrant, None), layout)
}

/// [`render`], into `layout`.
fn render_in(env: &Environment, layout: &Layout) -> Result<Files> {
    if let Some(refusal) = isoloom_plan::refusal(&env.requirements, &capabilities()) {
        return Err(Error::Refused {
            target: Target::Vagrant,
            refusal,
        });
    }
    if let Some(reason) = limits::refusal(env) {
        return Err(Error::Unsupported {
            target: Target::Vagrant,
            reason,
        });
    }
    let in_their_place = checks::in_their_place(env);
    let checks = checks::Checks::of(env, &in_their_place);
    let with_controller = with_controller(env, &checks);
    let links = qemu::links(env, with_controller);
    let ctx = Ctx {
        env,
        dir: layout.dir,
        links: &links,
        checks: &checks,
    };
    let mut r = Ruby::new();
    preamble(&mut r, env, layout.dir);
    qemu_preamble(&mut r, env, &links);
    let mut runners = Vec::new();
    r.block("Vagrant.configure(\"2\")", "config", |r| {
        settings(r, env);
        own::router_vm(r, &ctx);
        let vms = env
            .start_order
            .iter()
            .filter(|n| env.machines.get(*n).is_some_and(|m| m.shape == MachineShape::Vm));
        for name in vms {
            runners.extend(machine::vm(r, &ctx, name));
        }
        own::tools(r, &ctx);
        if with_controller {
            runners.extend(own::controller_vm(r, &ctx));
        }
        r.text(layout.extra);
    });
    let mut files = vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{}/Vagrantfile", layout.dir),
        contents: r.finish(),
    }];
    files.extend(runners);
    Ok(files)
}

/// Before `Vagrant.configure`: how to run it, the constants and helpers the VMs use.
fn preamble(r: &mut Ruby, env: &Environment, dir: &str) {
    r.text(&header("#"));
    r.comment(&format!(
        "Start:  cd .isoloom/{dir} && vagrant up\nStop:   cd .isoloom/{dir} && vagrant destroy -f"
    ));
    r.blank();
    // Machines start one after the other, in dependency order: a machine's services are up
    // before the ones depending on it boot, the controller comes last, and on QEMU the VM that
    // listens on a network's link is up before the one that connects.
    r.comment("One machine at a time, in order (QEMU and libvirt would boot them all at once).");
    r.assign("ENV[\"VAGRANT_NO_PARALLEL\"]", Value::str("1"));
    r.assign("ROOT", Value::raw("File.expand_path(\"../..\", __dir__)"));
    r.comment("This host's CPU as QEMU names it: a machine built for another one runs emulated there.");
    r.assign("HOST_ARCH", Value::raw(format!("{ARM_HOST} ? \"aarch64\" : \"x86_64\"")));
    r.text(&project_rb());
    r.text(hyperv::HYPERV_RB);
    r.blank();
    vagrantfile::inputs(r, &env.spec().inputs);
    esxi(r, env);
    r.blank();
}

/// QEMU's helpers ([`qemu::QEMU_RB`]) and its networks' hubs, when a VM needs them.
fn qemu_preamble(r: &mut Ruby, env: &Environment, links: &IndexMap<String, qemu::Link>) {
    let machines = env.machines.values().any(|m| m.shape == MachineShape::Vm && m.size.disk_gb.is_some());
    let controller = links.contains_key(CONTROLLER) && Controller::of(env.spec()).disk_gb.is_some();
    let sized = machines || controller;
    if !qemu::needs_ruby(links, sized) {
        return;
    }
    r.text(qemu::QEMU_RB);
    let hubs = qemu::hubs(links);
    if !hubs.is_empty() {
        r.comment("QEMU: each network's hub port of each of its VMs, in the order they start.");
        r.hash(
            "QEMU_HUBS",
            hubs.iter()
                .map(|(net, vms)| (Value::str(net), Value::hash(vms.iter().map(|(vm, p)| (Value::str(vm), Value::from(*p)))))),
        );
    }
    r.blank();
}

/// ESXi's port groups: `ESXI_VIRTUAL_NETWORK` lists the management network's, then one per
/// network in order.
fn esxi(r: &mut Ruby, env: &Environment) {
    let names = env.networks.keys().cloned().collect::<Vec<_>>().join(", ");
    r.comment(&format!(
        "ESXi (vagrant-vmware-esxi): ESXI_VIRTUAL_NETWORK lists port groups, comma-separated: the\nmanagement network first, then one per network, in order ({names}); a missing one reuses the last."
    ));
    r.assign(
        "ESXI_NETWORKS",
        Value::raw("ENV.fetch(\"ESXI_VIRTUAL_NETWORK\", \"VM Network\").split(\",\").map(&:strip)"),
    );
    r.assign("LAB_NETWORKS", Value::array(env.networks.keys().map(|n| Value::str(n))));
    r.text("def esxi_networks(nets)\n  [ESXI_NETWORKS[0]] + nets.map { |n| ESXI_NETWORKS[1 + LAB_NETWORKS.index(n)] || ESXI_NETWORKS[-1] }\nend\n");
}

/// The settings every VM shares.
fn settings(r: &mut Ruby, env: &Environment) {
    r.call(
        Call::new("config.vm.synced_folder")
            .arg(Value::str("."))
            .arg(Value::str("/vagrant"))
            .kw("disabled", true),
    );
    r.assign("config.vm.boot_timeout", 900u32);
    vagrantfile::ssh_proxy(r, "config");
    vagrantfile::libvirt_prefix(r, "config", &env.name);
}

/// Whether the Vagrantfile has a controller: to run the environment's playbooks, or checks.
fn with_controller(env: &Environment, checks: &checks::Checks<'_>) -> bool {
    env.controller.is_some() || !checks.controller.is_empty()
}

/// Each VM's link under QEMU, the controller included when the environment has one.
fn qemu_links(env: &Environment) -> IndexMap<String, qemu::Link> {
    let in_their_place = checks::in_their_place(env);
    let checks = checks::Checks::of(env, &in_their_place);
    qemu::links(env, with_controller(env, &checks))
}
