//! The machines and the controller: each a VM on its networks, the router its gateway. A Linux
//! machine is then set up by Terraform over SSH through the router with the VM core's
//! transports (its steps from the plan), a Windows one by cloudbase-init at its first boot; the
//! controller runs the playbooks once every machine is set up.

use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{Expression, Module, reference, string};
use isoloom_render::inventory::Access;
use isoloom_render::router;
use isoloom_spec::validate::Cidr;
use isoloom_spec::{DEFAULT_CPUS, DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::SERVICE_OS;
use super::guest::{Guest, Nic};
use crate::machine::cloud_init::CloudConfig;
use crate::machine::controller::{self, Ssh};
use crate::machine::linux::{self, Hooks};
use crate::machine::{Remote, cloudbase_init, vms};
use crate::module::ident;
use crate::server::router::is_default_gateway;

/// The controller's place among the VMs (after the router; the machines follow).
const CONTROLLER_INDEX: u64 = 1;

/// A public resolver, for a VM whose spec names none: it still installs software.
const RESOLVER: &str = "1.1.1.1";

/// What Terraform's connections go through: the router's uplink address.
fn bastion() -> Expression {
    reference("local.router_address")
}

/// The prefix length of a network's range.
fn len(env: &Environment, net: &str) -> u8 {
    env.networks.get(net).and_then(|n| Cidr::parse(&n.cidr)).map_or(32, |c| c.len)
}

/// An interface on network `net` at `address`: its MTU, and the default route through the
/// router at `gateway`.
pub(super) fn nic(env: &Environment, net: &str, address: String, gateway: bool) -> Nic {
    let mut nic = Nic::at(reference(&format!("esxi_portgroup.{}.name", ident(net))), Some(address));
    nic.gateway = gateway.then(|| router::address(env.spec(), net).to_string());
    nic.mtu = env.networks.get(net).and_then(|n| n.mtu);
    nic
}

/// The machine's interfaces: one per network (the router its default route on the first,
/// unless a bridged LAN's DHCP gives it), then one per outside network.
fn nics(env: &Environment, mp: &MachinePlan) -> Vec<Nic> {
    let routed = is_default_gateway(env, mp);
    let mut out: Vec<Nic> = mp
        .addresses
        .iter()
        .enumerate()
        .map(|(n, (net, addr))| nic(env, net, format!("{addr}/{}", len(env, net)), n == 0 && routed))
        .collect();
    out.extend(super::outside::nics(env, mp));
    out
}

/// Machine `name` (the `i`th VM): its data disks, its VM and its set-up.
pub(super) fn machine(env: &Environment, (name, mp): (&str, &MachinePlan), i: u64, m: &mut Module) {
    let id = ident(name);
    let dns = env.spec().machines.get(name).and_then(|sm| sm.dns.as_ref());
    let servers = match dns.filter(|d| !d.servers.is_empty()) {
        Some(d) => d.servers.iter().map(ToString::to_string).collect(),
        None => vec![RESOLVER.to_owned()],
    };
    let windows = mp.is_windows();
    let os = mp.os.as_deref().unwrap_or_default();
    let guest = Guest {
        id: id.clone(),
        name: name.to_owned(),
        index: CONTROLLER_INDEX + 1 + i,
        os: os.to_owned(),
        size: (
            mp.size.cpus.unwrap_or(DEFAULT_CPUS),
            mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB),
            mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB),
        ),
        nics: nics(env, mp),
        // Its own domain, else the private zone's (`dns.private`), whose names its hosts file
        // carries: ESXi has no DNS of its own to serve them.
        dns: (
            servers,
            dns.and_then(|d| d.domain.clone())
                .or_else(|| env.private_dns.as_ref().map(|z| z.domain.clone())),
        ),
        user_data: if windows {
            cloudbase_init::user_data(env, (name, mp))
        } else {
            CloudConfig::new(name, reference("local.users")).document()
        },
        wait_for_address: None,
        depends_on: vec![reference("esxi_guest.isoloom_router")],
        firmware: super::boot::firmware(mp),
        guestos: windows.then(|| super::windows::guestos(os)),
        disks: super::disks::blocks(name, &mp.disks, m),
        windows,
    };
    let what = if windows { ": set up by cloudbase-init at its first boot" } else { "" };
    m.commented(&format!("Machine `{name}`{what}."), guest.block(&env.name));
    let Some(address) = mp.addresses.values().next().filter(|_| !windows) else {
        return;
    };
    let setup = linux::provisioner(linux::Machine {
        remote: Remote {
            env,
            name,
            plan: mp,
            replace_on: reference(&format!("esxi_guest.{id}.id")),
            host: string(address.to_string()),
        },
        user: "isoloom".to_owned(),
        hooks: Hooks::default(),
    });
    m.commented(
        "Its set-up: its steps from the plan, over SSH through the router.",
        setup.via(bastion()).block(),
    );
}

/// The controller: on every network at the controller address, then its playbooks over SSH
/// through the router once every machine is set up (a Linux one's set-up done, a Windows one
/// answering WinRM with its ready marker).
pub(super) fn controller(env: &Environment, m: &mut Module) {
    let nics: Vec<Nic> = env
        .networks
        .iter()
        .enumerate()
        .filter_map(|(n, (net, plan))| {
            let c = Cidr::parse(&plan.cidr)?;
            Some(nic(env, net, format!("{}/{}", c.controller(), c.len), n == 0))
        })
        .collect();
    let Some(host) = nics.first().and_then(|n| n.addresses.first()?.split('/').next().map(str::to_owned)) else {
        return;
    };
    let (cpus, memory, disk) = controller::size(env);
    let guest = Guest {
        id: "isoloom_controller".to_owned(),
        name: "controller".to_owned(),
        index: CONTROLLER_INDEX,
        os: SERVICE_OS.to_owned(),
        size: (cpus, memory, disk.unwrap_or(10)),
        nics,
        dns: (vec![RESOLVER.to_owned()], None),
        user_data: CloudConfig::new("isoloom-controller", reference("local.users")).document(),
        wait_for_address: None,
        depends_on: vec![reference("esxi_guest.isoloom_router")],
        firmware: None,
        guestos: None,
        disks: Vec::new(),
        windows: false,
    };
    m.commented(
        "The controller: runs the environment's playbooks once every machine is set up.",
        guest.block(&env.name),
    );
    let ssh = Ssh {
        env,
        replace_on: reference("esxi_guest.isoloom_controller.id"),
        host: string(host),
        user: "isoloom".to_owned(),
        at_boot: Vec::new(),
        inventory: controller::inventory_template(env, Access::proxmox),
    };
    let mut setup = controller::provisioner_waiting(ssh, &controller::windows_waits(env));
    // A Windows machine has no Terraform set-up: its VM, and the wait above, stand for it.
    setup.depends_on = vms(env)
        .map(|(n, mp)| {
            let kind = if mp.is_windows() { "esxi_guest" } else { "terraform_data" };
            reference(&format!("{kind}.{}", ident(n)))
        })
        .collect();
    m.block(setup.via(bastion()).block());
}
