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
//!   Without root, QEMU links exactly two VMs per network (a socket listen/connect pair), each
//!   on one network; [`qemu_refusal`] says when a spec doesn't fit.

mod checks;
mod helpers;
mod limits;
mod machine;
mod providers;
mod qemu;
pub mod transport;

use indexmap::IndexMap;
use isoloom_plan::{Capabilities, Environment, Feature, MachineShape};
use isoloom_render::backend::{self, Backend, Files, RenderError};
use isoloom_render::{GenerateError, GeneratedFile, OUTPUT_DIR, header};
use isoloom_spec::model::{Spec, Target};

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
pub const PROJECT_RB: &str = concat!(
    include_str!("vagrant_project.rb"),
    "\n# Unpacks the project on Windows (no tar.exe before Windows Server 2019).\nIsoloomProject::UNTAR_PS1 = <<'PS1'\n",
    include_str!("../../isoloom-render/src/untar.ps1"),
    "PS1\n"
);

/// What Vagrant can render: a controller VM runs the environment's playbooks and the Ansible
/// checks, every check runs where its machine stands (a VM), and a machine can route a network
/// (its `gateway`). No network appliances (they are wired the way Docker attaches networks).
pub fn capabilities() -> Capabilities {
    Capabilities::of([
        Feature::EnvironmentProvisioning,
        Feature::PlaybookChecks,
        Feature::ExecWithoutContainer,
        Feature::Gateways,
        Feature::MachineAliases,
        Feature::Ulimits,
        Feature::TrafficControl,
    ])
}

/// The `vagrant` target's backend.
#[derive(Debug, Clone, Copy)]
pub struct VagrantBackend;

impl Backend for VagrantBackend {
    fn target(&self) -> Target {
        Target::Vagrant
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        render(env)
    }

    /// The plan's requirements against the capabilities, then the machines Vagrant can't build,
    /// without rendering.
    fn refusal(&self, env: &Environment) -> Option<String> {
        isoloom_plan::refusal(&env.requirements, &capabilities())
            .map(|r| r.to_string())
            .or_else(|| limits::refusal(env))
    }
}

/// Where a Vagrantfile goes and what it carries besides the environment's VMs: `hybrid` writes
/// its VMs to `.isoloom/hybrid/`, with its Docker host as one more `config.vm.define` block.
#[derive(Debug, Clone, Copy)]
pub struct Layout<'a> {
    /// The folder under `.isoloom/`.
    pub dir: &'a str,
    /// Ruby added at the end of the `configure` block (complete statements, indented for it).
    pub extra: &'a str,
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
pub fn render(env: &Environment) -> Result<Files, RenderError> {
    render_in(env, &Layout { dir: DIR, extra: "" })
}

/// [`render`], into `layout`.
pub fn render_in(env: &Environment, layout: &Layout) -> Result<Files, RenderError> {
    backend::check(Target::Vagrant, env, &capabilities())?;
    if let Some(what) = limits::refusal(env) {
        return Err(GenerateError::Unsupported { target: Target::Vagrant, what }.into());
    }
    let in_their_place = checks::in_their_place(env);
    let checks = checks::Checks::of(env, &in_their_place);
    let with_controller = env.controller.is_some() || !checks.controller.is_empty();
    let links = qemu::links(env, with_controller);
    let ctx = Ctx {
        env,
        dir: layout.dir,
        links: &links,
        checks: &checks,
    };
    let mut r = Ruby::new();
    preamble(&mut r, env, layout.dir);
    let mut runners = Vec::new();
    r.block("Vagrant.configure(\"2\")", "config", |r| {
        settings(r, env);
        helpers::router_vm(r, &ctx);
        for name in env.start_order.iter().filter(|n| env.machines[*n].shape == MachineShape::Vm) {
            runners.extend(machine::vm(r, &ctx, name));
        }
        helpers::tools(r, &ctx);
        if with_controller {
            runners.extend(helpers::controller_vm(r, &ctx));
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
    r.text(PROJECT_RB);
    r.blank();
    vagrantfile::inputs(r, &env.spec().inputs);
    esxi(r, env);
    r.blank();
}

/// ESXi's port groups: ESXI_VIRTUAL_NETWORK lists the management network's, then one per
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

/// The files of the `vagrant` target for `spec`, through its plan, into `layout` (the hybrid
/// target renders its VMs with it).
pub fn generate_in(spec: &Spec, layout: &Layout) -> Result<Vec<GeneratedFile>, GenerateError> {
    render_in(&Environment::build(spec, Target::Vagrant, None), layout).map_err(GenerateError::from)
}

/// Why this environment's VMs can't all have their networks under Vagrant's QEMU provider, if
/// they can't (`refusal_on` asks it for `qemu`).
pub fn qemu_refusal(spec: &Spec) -> Option<String> {
    let env = Environment::build(spec, Target::Vagrant, None);
    let in_their_place = checks::in_their_place(&env);
    let checks = checks::Checks::of(&env, &in_their_place);
    let with_controller = env.controller.is_some() || !checks.controller.is_empty();
    qemu::refusal(qemu::links(&env, with_controller))
}

#[cfg(test)]
mod tests;
