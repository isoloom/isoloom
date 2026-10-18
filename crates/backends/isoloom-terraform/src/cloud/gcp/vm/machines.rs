//! Each machine's firewall rules and its instance, set up by its startup script through the VM
//! core; the controller's.

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachinePlan, NetworkPlan};
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, Template, boolean, list, number, object, reference, string};
use isoloom_render::inventory::WinrmAuth;
use isoloom_spec::{DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::network::vpc;
use super::{NESTING, RULES, for_nics, image, machine_type, user};
use crate::cloud::instance::{name_with, tags};
use crate::cloud::vm::{controller_setup, disks, gateways, hooks, interfaces, nested, own_image, reach, seal};
use crate::machine::{controller, user_data};
use crate::module::{ident, resource};

/// A machine's machine type: its GPUs', else a nesting one, else by its memory, with room for an
/// interface per network.
pub(super) fn machine_type_of(mp: &MachinePlan) -> &'static str {
    let memory = mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB);
    let t = RULES.gpus.of(mp).or_else(|| nested::size(mp, memory, NESTING)).unwrap_or(machine_type(memory));
    for_nics(t, mp.addresses.len())
}

/// The MAC address Google Cloud gives an interface: `42:01` then its address's four bytes.
fn mac(a: Ipv4Addr) -> Expression {
    let bytes: Vec<String> = a.octets().iter().map(|b| format!("{b:02x}")).collect();
    string(format!("42:01:{}", bytes.join(":")))
}

/// What every boot brings up: each interface beyond the first, by its MAC (Google's guest agent
/// may have already), and forwarding on a gateway.
fn at_boot(env: &Environment, addresses: &IndexMap<String, Ipv4Addr>, gateway: Option<&MachinePlan>) -> Vec<Template> {
    let mut out = interfaces::up(env, addresses, &|n| mac(addresses[n]));
    out.extend(gateway.map(gateways::at_boot).unwrap_or_default());
    out
}

/// A Linux machine: what may reach it, its instance, set up by its startup script.
pub(super) fn linux(env: &Environment, (name, mp): (&str, &MachinePlan), m: &mut Module) {
    let id = ident(name);
    if mp.addresses.is_empty() {
        return;
    }
    let rules = firewalls(env, (name, mp));
    if let Some((first, rest)) = rules.split_first() {
        m.commented(&format!("Machine `{name}`: what may reach it."), first.clone());
        m.blocks(rest.iter().cloned());
    }
    let os = mp.os.as_deref().unwrap_or_default();
    m.blocks(disks::gcp(name, &mp.disks));
    let setup = hooks::user_data(env, (name, mp), user(os), at_boot(env, &mp.addresses, Some(mp)));
    let mut vm = instance(&Instance {
        env,
        id: &id,
        who: name,
        os,
        own: own_image::of(mp, own_image::gcp),
        machine_type: machine_type_of(mp),
        gpus: RULES.gpus.of(mp).is_some(),
        nested: nested::wanted(mp),
        disk: Some(mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB)),
        on: (&mp.addresses, mp.is_gateway || mp.addresses.len() > 1),
        more: &mp.more_addresses,
        tpm: mp.tpm.is_some(),
        setup,
    });
    vm.body.extend(disks::gcp_attached(name, &mp.disks));
    m.block(vm);
}

/// Who may reach a machine, a rule per source (Google Cloud's firewall rules are a VPC's and
/// match the instance by its tag): each of its networks in that one's VPC, SSH from
/// `allowed_cidr` and its published ports in its first network's (where its public address is),
/// the networks `reach` opens to it (on their ports, TCP and UDP) in the VPC of the network
/// opened.
fn firewalls(env: &Environment, (name, mp): (&str, &MachinePlan)) -> Vec<Block> {
    let id = ident(name);
    let allowed = || list([reference("var.allowed_cidr")]);
    let cidrs = |p: &NetworkPlan| list([string(p.cidr.as_str())]);
    let Some(m) = env.spec().machines.get(name) else { return Vec::new() };
    let Some(first) = m.networks.keys().next() else { return Vec::new() };
    let mut out: Vec<Block> = reach::own(env, m)
        .map(|(n, p)| firewall((&id, name), (&format!("net-{n}"), n), cidrs(p), vec![allow("all", &[])]))
        .collect();
    out.push(firewall((&id, name), ("ssh", first), allowed(), vec![allow("tcp", &[22])]));
    for (r, from) in reach::opening(env, m) {
        let allows = if r.ports.is_empty() {
            vec![allow("all", &[])]
        } else {
            vec![allow("tcp", &r.ports), allow("udp", &r.ports)]
        };
        out.push(firewall((&id, name), (&format!("reach-{}", r.from), &r.to), cidrs(from), allows));
    }
    let ports = &mp.ports;
    let published = |tcp: bool| -> Vec<u16> { ports.iter().filter(|x| x.protocol.is_tcp() == tcp).map(|x| x.publish).collect() };
    let allows: Vec<Block> = [("tcp", published(true)), ("udp", published(false))]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(proto, v)| allow(proto, &v))
        .collect();
    if !allows.is_empty() {
        out.push(firewall((&id, name), ("published", first), allowed(), allows));
    }
    if seal::sealed(env, mp) {
        for net in mp.addresses.keys() {
            out.extend(sealed((&id, name), env, net));
        }
    }
    out
}

/// Once the run is sealed, a machine with no internet goes out to the lab's blocks only, in
/// network `net`'s VPC: everything out denied, the lab's blocks allowed before it. A machine's
/// replies to what came in are let out regardless (Google Cloud's firewall is stateful).
fn sealed((id, who): (&str, &str), env: &Environment, net: &str) -> [Block; 2] {
    let rule = |what: &str, priority: u64, to: Expression, verdict: Block| {
        resource("google_compute_firewall", &format!("{id}_{what}_{}", ident(net)))
            .add_attribute(("count", seal::when_sealed(number(1), number(0))))
            .add_attribute(("project", reference("local.project")))
            .add_attribute(("name", name_with(&format!("-{who}-{what}-{net}"))))
            .add_attribute(("network", vpc(net)))
            .add_attribute(("direction", string("EGRESS")))
            .add_attribute(("priority", number(priority)))
            .add_attribute(("destination_ranges", to))
            .add_attribute(("target_tags", list([name_with(&format!("-{who}"))])))
            .add_block(verdict)
            .build()
    };
    let all = |verdict: &str| Block::builder(verdict).add_attribute(("protocol", string("all"))).build();
    [
        rule("lab", 900, seal::lab_blocks(env), all("allow")),
        rule("sealed", 1000, list([string("0.0.0.0/0")]), all("deny")),
    ]
}

/// A firewall rule in network `net`'s VPC letting `sources` in to the instances tagged
/// `<env>-<who>` (`(id, who)`: the machine's resource name and its name in the cloud), named
/// after `what`.
fn firewall((id, who): (&str, &str), (what, net): (&str, &str), sources: Expression, allows: Vec<Block>) -> Block {
    let mut b = resource("google_compute_firewall", &format!("{id}_{}", ident(what)))
        .add_attribute(("project", reference("local.project")))
        .add_attribute(("name", name_with(&format!("-{who}-{what}"))))
        .add_attribute(("network", vpc(net)))
        .add_attribute(("direction", string("INGRESS")))
        .add_attribute(("source_ranges", sources))
        .add_attribute(("target_tags", list([name_with(&format!("-{who}"))])));
    for a in allows {
        b = b.add_block(a);
    }
    b.build()
}

/// An `allow` block: a protocol, on some ports (all of them when none).
fn allow(protocol: &str, ports: &[u16]) -> Block {
    let b = Block::builder("allow").add_attribute(("protocol", string(protocol)));
    if ports.is_empty() {
        return b.build();
    }
    b.add_attribute(("ports", list(ports.iter().map(|p| string(p.to_string()))))).build()
}

/// An instance, as [`instance`] writes it.
struct Instance<'a> {
    env: &'a Environment,
    /// Its resource name.
    id: &'a str,
    /// Its name in the cloud (`<env>-<who>`), and its tag.
    who: &'a str,
    os: &'a str,
    /// Its own image (`vm.image.gcp`), instead of its OS's.
    own: Option<&'a str>,
    /// Its machine type.
    machine_type: &'a str,
    /// Whether the machine type carries GPUs (which can't move to another host live).
    gpus: bool,
    /// Whether it runs VMs of its own.
    nested: bool,
    /// Its boot disk in GB (the image's size when `None`).
    disk: Option<u32>,
    /// Its networks and its address on each, an interface each (the first with its public
    /// address); and whether IP forwarding is on: it routes a network, or is on several (replies
    /// leave from the right interface).
    on: (&'a IndexMap<String, Ipv4Addr>, bool),
    /// Its further addresses on each network (`lan: [10, 11]`).
    more: &'a IndexMap<String, Vec<Ipv4Addr>>,
    /// A shielded VM with a virtual TPM and secure boot (`vm.tpm`).
    tpm: bool,
    /// Its set-up, as its startup script (run at every boot: the set-up stops once it ended).
    setup: Expression,
}

/// A further address of the instance on its network: an alias range of one address, from the
/// subnet's own range (the network's block, which a secondary range may not overlap), which the
/// VPC then delivers to the instance.
fn alias(address: Ipv4Addr) -> Block {
    Block::builder("alias_ip_range")
        .add_attribute(("ip_cidr_range", string(format!("{address}/32"))))
        .build()
}

/// An instance at its address with a public one; its user from the `ssh-keys` metadata; its
/// set-up as its startup script.
fn instance(i: &Instance) -> Block {
    let (image, _) = image(i.os).unwrap_or_default();
    let user = user(i.os);
    let mut params = Block::builder("initialize_params").add_attribute(("image", string(i.own.unwrap_or(image))));
    if let Some(gb) = i.disk {
        params = params.add_attribute(("size", number(u64::from(gb))));
    }
    let nics = i.on.0.iter().enumerate().map(|(n, (net, address))| {
        let nic = Block::builder("network_interface")
            .add_attribute(("subnetwork", reference(&format!("google_compute_subnetwork.{}.id", ident(net)))))
            .add_attribute(("network_ip", string(address.to_string())))
            .add_blocks(i.more.get(net).into_iter().flatten().copied().map(alias));
        if n == 0 {
            nic.add_block(Block::builder("access_config").build())
        } else {
            nic
        }
        .build()
    });
    let key = Template::text(format!("{user}:")).with_expr(reference("var.ssh_public_key"));
    let b = resource("google_compute_instance", i.id)
        .add_attribute(("project", reference("local.project")))
        .add_attribute(("name", name_with(&format!("-{}", i.who))))
        .add_attribute(("machine_type", string(i.machine_type)))
        .add_attribute(("zone", reference("local.zone")))
        .add_attribute(("tags", list([name_with(&format!("-{}", i.who))])))
        .add_block(Block::builder("boot_disk").add_block(params.build()).build())
        .add_blocks(nics)
        .add_attribute(("metadata", object([("ssh-keys", key.into_expression())])))
        .add_attribute(("metadata_startup_script", i.setup.clone()))
        .add_attribute(("labels", tags(i.env)));
    let b = if i.on.1 { b.add_attribute(("can_ip_forward", boolean(true))) } else { b };
    hardware(b, i).build()
}

/// The instance's shielded VM, nested virtualization and GPU scheduling, as it asks for them.
fn hardware(mut b: BlockBuilder, i: &Instance) -> BlockBuilder {
    if i.tpm {
        // Google's public images are all shielded-VM images: a vTPM and secure boot.
        b = b.add_block(
            Block::builder("shielded_instance_config")
                .add_attribute(("enable_secure_boot", boolean(true)))
                .add_attribute(("enable_vtpm", boolean(true)))
                .build(),
        );
    }
    if i.nested {
        let features = Block::builder("advanced_machine_features").add_attribute(("enable_nested_virtualization", boolean(true)));
        b = b.add_block(features.build());
    }
    if i.gpus {
        // A GPU stays with its host: maintenance stops the VM instead of moving it.
        b = b.add_block(Block::builder("scheduling").add_attribute(("on_host_maintenance", string("TERMINATE"))).build());
    }
    b
}

/// The controller: what may reach it (every network, SSH from `allowed_cidr`), its instance
/// with an interface on every network at the controller address, set up by its startup script
/// (it waits for every machine's own set-up before the playbooks).
pub(super) fn controller(env: &Environment, m: &mut Module) {
    let id = "isoloom_controller";
    let Some(first) = env.networks.keys().next() else { return };
    let addresses = controller::addresses(env);
    let who = (id, "controller");
    let mut rules = env
        .networks
        .iter()
        .map(|(n, p)| firewall(who, (&format!("net-{n}"), n), list([string(p.cidr.as_str())]), vec![allow("all", &[])]));
    if let Some(rule) = rules.next() {
        m.commented("The controller (Ansible): every network may reach it, and SSH from allowed_cidr.", rule);
    }
    m.blocks(rules);
    m.block(firewall(who, ("ssh", first), list([reference("var.allowed_cidr")]), vec![allow("tcp", &[22])]));
    let (_, memory, disk) = controller::size(env);
    let inventory = controller::cloud_inventory(env, |os| user(os).to_owned(), WinrmAuth::Basic);
    let who_logs_in = |mp: &MachinePlan| user(mp.os.as_deref().unwrap_or_default()).to_owned();
    let setup = controller_setup::set_up(env, &who_logs_in, inventory, &at_boot(env, &addresses, None));
    m.block(instance(&Instance {
        env,
        id,
        who: "controller",
        os: controller::OS,
        own: None,
        machine_type: for_nics(machine_type(memory), addresses.len()),
        gpus: false,
        nested: false,
        disk,
        on: (&addresses, addresses.len() > 1),
        more: &IndexMap::new(),
        tpm: false,
        setup: user_data::expression(&setup),
    }));
}
