//! Isoloom's own VMs: the router (when `reach` rules need one), the `shell` tool, and the
//! controller that runs the environment's playbooks and the checks standing on every network.
//! Their set-up goes through the same step renderings and transport as the lab's machines.

use std::net::Ipv4Addr;

use isoloom_plan::checks::Position;
use isoloom_plan::lifecycle::Step;
use isoloom_plan::{Controller, Environment};
use isoloom_render::boot::{self, Trigger};
use isoloom_render::inventory::{self, Access};
use isoloom_render::router;
use isoloom_render::{GeneratedFile, controller};
use isoloom_spec::images::HyperVBox;
use isoloom_spec::model::Spec;
use isoloom_spec::validate::Cidr;

use super::hyperv::HyperV;
use super::machine::{define, private_network};
use super::providers::{self, Guest, Qemu, Size};
use super::qemu::Link;
use super::transport::{Os, shell, step_provisioners};
use super::{CONTROLLER, Ctx, TOOL_SHELL, checks};
use crate::ruby::{Ruby, Value};

/// The Debian box of Isoloom's own VMs; libvirt can't run it, and uses [`HELPER_LIBVIRT_BOX`].
const HELPER_BOX: &str = "bento/debian-12";

/// The controller's box by default: Alpine, about 100 MB (Python, ansible-core and pywinrm run
/// fine on it), for VirtualBox, libvirt, VMware and Parallels alike.
const CONTROLLER_BOX: &str = "generic/alpine319";

/// The controller's box and its version: `controller.image.vagrant`, else [`CONTROLLER_BOX`].
fn controller_box(spec: &Spec) -> (&str, Option<&str>) {
    let image = spec.controller.as_ref().and_then(|c| c.image.as_ref());
    match image.and_then(|i| i.vagrant.as_deref()) {
        Some(b) => (b, image.and_then(|i| i.vagrant_version.as_deref())),
        None => (CONTROLLER_BOX, None),
    }
}
/// The libvirt box of Isoloom's own Debian VMs (router, controller, tool shell).
const HELPER_LIBVIRT_BOX: &str = "generic/debian12";

/// The provisioners of planned steps on one of Isoloom's own (Linux) VMs.
fn steps_of(r: &mut Ruby, env: &Environment, vm: &str, steps: &[Step]) {
    for s in steps {
        step_provisioners(r, env, vm, s, Os::Linux);
    }
}

/// Every machine by name, at its address on its first network (Isoloom's own VMs are on all).
fn every_machine(env: &Environment) -> Step {
    let entries = env
        .machines
        .values()
        .filter_map(|mp| mp.addresses.first().map(|(_, ip)| (*ip, mp.names.clone())))
        .collect();
    Step::Hosts { entries }
}

/// One of Isoloom's own VMs, as its providers see it: on each network of `at`, at its address
/// there.
fn helper<'a>(env: &'a Environment, what: &str, size: Size, at: Vec<(&'a str, Ipv4Addr)>, link: &'a Link) -> Guest<'a> {
    Guest {
        label: format!("{} · {what}", env.name),
        vm_name: format!("{}-{}", env.name, what.replace(' ', "-")),
        size,
        nets: at.iter().map(|(net, _)| *net).collect(),
        libvirt_box: Some(HELPER_LIBVIRT_BOX),
        boot: None,
        qemu: Qemu::helper(link),
        hyperv: HyperV::helper(env, at),
        tpm: None,
        esxi_guestos: None,
        nested: false,
        disks: Vec::new(),
    }
}

/// Each network with the VM's address on it (`at`), in order.
fn addresses(env: &Environment, at: impl Fn(Cidr) -> Ipv4Addr) -> Vec<(&str, Ipv4Addr)> {
    env.networks
        .iter()
        .filter_map(|(net, n)| Cidr::parse(&n.cidr).map(|c| (net.as_str(), at(c))))
        .collect()
}

/// Each network at the VM's address on it (`at`), as private networks.
fn networks(r: &mut Ruby, env: &Environment, at: impl Fn(Cidr) -> Ipv4Addr) {
    for (net, ip) in addresses(env, at) {
        private_network(r, env, net, ip);
    }
}

/// The router VM: on every network it routes, at its last address, forwarding with the
/// `reach` rules (and impairing links with `tc`).
pub(crate) fn router_vm(r: &mut Ruby, ctx: &Ctx) {
    let env = ctx.env;
    let Some(routed) = &env.router else { return };
    let link = ctx.links.get(router::NAME).unwrap_or(&Link::None);
    let spec = env.spec();
    let tc = router::tc_script(spec)
        .map(|t| {
            boot::at_boot(
                "tc",
                "Link impairment (networks.*.tc) on the isoloom router's interfaces",
                &t,
                Trigger::Boot,
                &[],
            )
        })
        .unwrap_or_default();
    let pairs: Vec<_> = env.router6.iter().filter_map(|(n, v6)| Some((*routed.get(n)?, *v6))).collect();
    let v6 = isoloom_render::ipv6::router_linux(&pairs)
        .map(|s| boot::at_boot("ipv6", "IPv6 addresses of the isoloom router", &s, Trigger::BootAndLinkUp, &[]))
        .unwrap_or_default();
    let script = format!(
        "set -e\nexport DEBIAN_FRONTEND=noninteractive\napt-get update -qq\napt-get install -y -qq nftables >/dev/null\necho 'net.ipv4.ip_forward=1' > /etc/sysctl.d/90-isoloom.conf\nsysctl -q -p /etc/sysctl.d/90-isoloom.conf\ncat > /etc/nftables.conf <<'NFT'\nflush ruleset\n{}NFT\nsystemctl enable nftables\nnft -f /etc/nftables.conf\n{v6}{tc}",
        router::nftables(spec),
    );
    define(r, router::NAME, |r| {
        r.assign("m.vm.box", Value::str(HELPER_BOX));
        r.assign("m.vm.hostname", Value::str(router::NAME));
        for (net, ip) in routed {
            private_network(r, env, net, *ip);
        }
        let at = routed.iter().map(|(net, ip)| (net.as_str(), *ip)).collect();
        providers::blocks(r, &helper(env, "router", Size::HELPER, at, link));
        steps_of(r, env, router::NAME, &[Step::Prelude]);
        shell(r, "router", &script);
    });
}

/// The tools: `shell` is a Debian VM on every network at the tool's address, with the usual
/// observation tools, outside the environment's contract; the others have no VM form.
pub(crate) fn tools(r: &mut Ruby, ctx: &Ctx) {
    for (i, (name, tool)) in ctx.env.spec().tools.iter().enumerate() {
        r.blank();
        if name == "shell" {
            r.comment("Tool `shell`: a toolbox on every network (tcpdump, nmap, curl, dig, netcat).");
            tool_shell(r, ctx, i);
        } else {
            r.comment(&format!(
                "Tool `{name}` ({}) runs on the container targets; no VM form.",
                tool.image.as_deref().unwrap_or("image")
            ));
        }
    }
}

fn tool_shell(r: &mut Ruby, ctx: &Ctx, index: usize) {
    let env = ctx.env;
    let link = ctx.links.get(TOOL_SHELL).unwrap_or(&Link::None);
    let define = crate::ruby::Call::new("config.vm.define").arg(Value::str(TOOL_SHELL));
    r.block(define, "m", |r| {
        r.assign("m.vm.box", Value::str(HELPER_BOX));
        r.assign("m.vm.hostname", Value::str("shell"));
        networks(r, env, |c| c.tool(index));
        let at = addresses(env, |c| c.tool(index));
        providers::blocks(r, &helper(env, "tool shell", Size::HELPER, at, link));
        steps_of(r, env, TOOL_SHELL, &[Step::Prelude, every_machine(env)]);
        shell(
            r,
            "toolbox",
            "export DEBIAN_FRONTEND=noninteractive; apt-get update -qq && apt-get install -y -qq tcpdump nmap curl dnsutils netcat-openbsd iproute2 >/dev/null",
        );
    });
}

/// What the controller installs before Ansible: Python's venv, sshpass (password SSH), curl and
/// netcat (checks). Alpine's packages (the default box), or Debian's (a `controller.image`).
const CONTROLLER_PACKAGES: &str = "if command -v apk >/dev/null; then\n  apk add -q --no-cache python3 sshpass curl netcat-openbsd\nelse\n  export DEBIAN_FRONTEND=noninteractive\n  apt-get update -qq\n  apt-get install -y -qq python3-venv sshpass curl netcat-openbsd >/dev/null\nfi\n";

/// The controller: a small VM (Alpine by default) on every network at its controller address,
/// started after every machine. It writes the inventory, runs the environment's playbooks, then
/// halts unless the spec keeps it running; it runs the checks standing on every network on
/// demand (their runner is returned).
pub(crate) fn controller_vm(r: &mut Ruby, ctx: &Ctx) -> Option<GeneratedFile> {
    let env = ctx.env;
    let (bx, version) = controller_box(env.spec());
    let controller = Controller::of(env.spec());
    let size = Size {
        cpus: controller.cpus,
        mem: controller.memory_mb,
        disk: controller.disk_gb,
    };
    let link = ctx.links.get(CONTROLLER).unwrap_or(&Link::None);
    let mut guest = helper(env, "controller", size, addresses(env, Cidr::controller), link);
    // The default box (Alpine) has arm64 only for libvirt and QEMU: an arm64 host on the other
    // providers gets Debian's instead.
    guest.libvirt_box = (bx == CONTROLLER_BOX).then_some(CONTROLLER_BOX);
    // Its box (Alpine's, or the spec's own) boots on Hyper-V as it is.
    guest.hyperv.boxed = HyperVBox::Main;
    let mut steps = vec![Step::Prelude];
    steps.extend(size.disk.map(|_| Step::DiskGrow));
    steps.extend([every_machine(env), Step::Project]);
    let on_controller = &ctx.checks.controller;
    define(r, CONTROLLER, |r| {
        controller_box_line(r, bx, version);
        r.assign("m.vm.hostname", Value::str(CONTROLLER));
        networks(r, env, Cidr::controller);
        providers::blocks(r, &guest);
        steps_of(r, env, CONTROLLER, &steps);
        controller_steps(r, env, !on_controller.is_empty());
        if !on_controller.is_empty() {
            checks::provisioner(r, "controller", on_controller);
        }
    });
    (!on_controller.is_empty()).then(|| checks::runner(env, ctx.dir, "controller", &Position::Networks, on_controller))
}

/// The default box (Alpine) unless the spec picks one; Debian's on an arm64 host.
fn controller_box_line(r: &mut Ruby, bx: &str, version: Option<&str>) {
    if bx == CONTROLLER_BOX && version.is_none() {
        let debian = Value::str(HELPER_BOX);
        r.assign("m.vm.box", Value::raw(format!("{} ? {debian} : {}", super::ARM_HOST, Value::str(bx))));
    } else {
        r.assign("m.vm.box", Value::str(bx));
    }
    if let Some(v) = version {
        r.assign("m.vm.box_version", Value::str(v));
    }
}

/// Ansible and the inventory, the environment's playbooks, going offline, halting.
fn controller_steps(r: &mut Ruby, env: &Environment, with_checks: bool) {
    let spec = env.spec();
    let setup = format!(
        "set -e\n{CONTROLLER_PACKAGES}[ -x /opt/ansible/bin/ansible-playbook ] || {{ python3 -m venv /opt/ansible && /opt/ansible/bin/pip install -q 'ansible-core>=2.15,<2.17' pywinrm; }}\nmkdir -p /etc/isoloom\ncat > /etc/isoloom/inventory.ini <<'INV'\n{}INV\n",
        inventory::render(env, &Access::vagrant())
    );
    shell(r, "controller", &setup);
    if !spec.provision.is_empty() {
        shell(r, "ansible", &controller::provision_script(spec, &[]));
    }
    // Standing in for a user on offline networks, the controller goes offline too once it's
    // done installing (its checks would otherwise see its own NAT internet).
    if with_checks && !env.networks.values().any(|n| n.internet) {
        step_provisioners(r, env, CONTROLLER, &Step::Egress, Os::Linux);
    }
    // Done: it halts (its disk kept) unless the spec keeps it running. Detached, so that the
    // provisioner returns before the VM goes; `isoloom test` boots it again for the checks.
    if !spec.controller.as_ref().is_some_and(|c| c.keep_running) {
        shell(r, "halt", "touch /run/isoloom-halting; (sleep 5; poweroff) >/dev/null 2>&1 &");
    }
}
