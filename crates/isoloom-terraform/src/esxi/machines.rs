//! The machines and the controller: each a VM on its networks, the router its gateway, then set
//! up by Terraform over SSH through the router with the VM core's transports (a machine's steps
//! from the plan, or the controller's playbooks).

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Expression, Module, reference, string};
use isoloom_render::inventory::Access;
use isoloom_render::router;
use isoloom_spec::validate::Cidr;
use isoloom_spec::{DEFAULT_CPUS, DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::SERVICE_OS;
use super::guest::{Guest, Nic};
use crate::vm::cloud_init::CloudConfig;
use crate::vm::controller::{self, Ssh};
use crate::vm::linux::{self, Hooks};
use crate::vm::{os, res};

/// The controller's place among the VMs (after the router; the machines follow).
const CONTROLLER_INDEX: usize = 1;

/// A public resolver, for a VM whose spec names none: it still installs software.
const RESOLVER: &str = "1.1.1.1";

/// What Terraform's connections go through: the router's uplink address.
fn bastion() -> Expression {
    reference("local.router_address")
}

/// The prefix length of a network's range.
fn len(env: &Environment, net: &str) -> u8 {
    Cidr::parse(&env.networks[net].cidr).map_or(32, |c| c.len)
}

/// Machine `name` (the `i`th VM): its VM and its set-up.
pub fn machine(env: &Environment, name: &str, i: usize, m: &mut Module) {
    let mp = &env.machines[name];
    let id = res(name);
    let nics = mp
        .addresses
        .iter()
        .enumerate()
        .map(|(n, (net, addr))| Nic {
            port_group: reference(&format!("esxi_portgroup.{}.name", res(net))),
            address: Some(format!("{addr}/{}", len(env, net))),
            gateway: (n == 0).then(|| router::address(env.spec(), net).to_string()),
        })
        .collect();
    let dns = env.spec().machines[name].dns.as_ref();
    let servers = match dns.filter(|d| !d.servers.is_empty()) {
        Some(d) => d.servers.iter().map(ToString::to_string).collect(),
        None => vec![RESOLVER.to_string()],
    };
    let guest = Guest {
        id: id.clone(),
        name: name.to_string(),
        index: CONTROLLER_INDEX + 1 + i,
        os: os(mp).unwrap_or_default().to_string(),
        size: (
            mp.size.cpus.unwrap_or(DEFAULT_CPUS),
            mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB),
            mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB),
        ),
        nics,
        dns: (servers, dns.and_then(|d| d.domain.clone())),
        user_data: CloudConfig::new(name, reference("local.users")).document(),
        wait_for_address: None,
        depends_on: vec![reference("esxi_guest.isoloom_router")],
    };
    m.commented(&format!("Machine `{name}`."), guest.block(&env.name));
    let Some(address) = mp.addresses.values().next() else { return };
    let setup = linux::provisioner(linux::Machine {
        env,
        name,
        replace_on: reference(&format!("esxi_guest.{id}.id")),
        host: string(address.to_string()),
        user: "isoloom".into(),
        hooks: Hooks::default(),
    });
    m.commented(
        "Its set-up: its steps from the plan, over SSH through the router.",
        setup.via(bastion()).block(),
    );
}

/// The controller: on every network at the controller address, then its playbooks over SSH
/// through the router once every machine is set up.
pub fn controller(env: &Environment, m: &mut Module) {
    let nics: Vec<Nic> = env
        .networks
        .iter()
        .enumerate()
        .filter_map(|(n, (net, plan))| {
            let c = Cidr::parse(&plan.cidr)?;
            Some(Nic {
                port_group: reference(&format!("esxi_portgroup.{}.name", res(net))),
                address: Some(format!("{}/{}", c.controller(), c.len)),
                gateway: (n == 0).then(|| router::address(env.spec(), net).to_string()),
            })
        })
        .collect();
    let Some(host) = nics.first().and_then(|n| n.address.as_deref()?.split('/').next().map(str::to_string)) else {
        return;
    };
    let (cpus, memory, disk) = controller::size(env);
    let guest = Guest {
        id: "isoloom_controller".into(),
        name: "controller".into(),
        index: CONTROLLER_INDEX,
        os: SERVICE_OS.into(),
        size: (cpus, memory, disk.unwrap_or(10)),
        nics,
        dns: (vec![RESOLVER.to_string()], None),
        user_data: CloudConfig::new("isoloom-controller", reference("local.users")).document(),
        wait_for_address: None,
        depends_on: vec![reference("esxi_guest.isoloom_router")],
    };
    m.commented(
        "The controller: runs the environment's playbooks once every machine is set up.",
        guest.block(&env.name),
    );
    let setup = controller::provisioner(Ssh {
        env,
        replace_on: reference("esxi_guest.isoloom_controller.id"),
        host: string(host),
        user: "isoloom".into(),
        at_boot: Vec::new(),
        inventory: controller::inventory_template(env, Access::proxmox),
    });
    m.block(setup.via(bastion()).block());
}
