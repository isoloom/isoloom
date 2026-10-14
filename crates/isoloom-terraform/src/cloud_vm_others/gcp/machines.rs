//! Each machine's firewall rules, its instance and its set-up through the VM core; the
//! controller's.

use std::net::Ipv4Addr;

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, boolean, list, null, number, object, reference, string};
use isoloom_spec::validate::Cidr;
use isoloom_spec::{DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::{GPUS, NESTING, image, machine_type, public_ip, user, vpc_does};
use crate::vm::cloud::{self, name_with, resource};
use crate::vm::linux::{self, Hooks};
use crate::vm::{controller, nested, os, own_image, res};

/// A Linux machine: what may reach it, its instance, its set-up over SSH.
pub fn linux(env: &Environment, name: &str, m: &mut Module) {
    let mp = &env.machines[name];
    let id = res(name);
    let Some((net, address)) = mp.addresses.first() else { return };
    let rules = firewalls(env, name);
    if let Some((first, rest)) = rules.split_first() {
        m.commented(&format!("Machine `{name}`: what may reach it."), first.clone());
        m.blocks(rest.iter().cloned());
    }
    let os = os(mp).unwrap_or_default();
    let memory = mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB);
    m.block(instance(&Instance {
        env,
        id: &id,
        who: name,
        os,
        own: own_image::of(mp, own_image::gcp),
        machine_type: GPUS.of(mp).or_else(|| nested::size(mp, memory, NESTING)).unwrap_or(machine_type(memory)),
        gpus: GPUS.of(mp).is_some(),
        nested: nested::wanted(mp),
        disk: Some(mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB)),
        on: (net, *address),
        tpm: mp.tpm.is_some(),
    }));
    let hooks = Hooks {
        at_boot: Vec::new(),
        after_reboot: vec![cloud::reschedule_auto_stop()],
        before_egress: cloud::redirects(mp),
        skip: vpc_does,
    };
    let machine = linux::Machine {
        env,
        name,
        replace_on: reference(&format!("google_compute_instance.{id}.id")),
        host: public_ip(&id),
        user: user(os).to_string(),
        hooks,
    };
    m.block(linux::provisioner(machine).block());
}

/// Who may reach a machine, a rule per source (Google Cloud's firewall rules are the VPC's and
/// match the instance by its tag): its network, SSH from `allowed_cidr`, the networks `reach`
/// opens to it (on their ports, TCP and UDP), its published ports from `allowed_cidr`.
fn firewalls(env: &Environment, name: &str) -> Vec<Block> {
    let spec = env.spec();
    let m = &spec.machines[name];
    let id = res(name);
    let allowed = || list([reference("var.allowed_cidr")]);
    let cidrs = |n: &str| list([string(env.networks[n].cidr.as_str())]);
    let mut out: Vec<Block> = m
        .networks
        .keys()
        .map(|n| firewall((&id, name), "net", cidrs(n), vec![allow("all", &[])]))
        .collect();
    out.push(firewall((&id, name), "ssh", allowed(), vec![allow("tcp", &[22])]));
    for r in spec
        .reach
        .iter()
        .filter(|r| m.networks.contains_key(&r.to) && !m.networks.contains_key(&r.from))
    {
        let allows = if r.ports.is_empty() {
            vec![allow("all", &[])]
        } else {
            vec![allow("tcp", &r.ports), allow("udp", &r.ports)]
        };
        out.push(firewall((&id, name), &format!("reach-{}", r.from), cidrs(&r.from), allows));
    }
    let ports = &env.machines[name].ports;
    let published = |tcp: bool| -> Vec<u16> { ports.iter().filter(|x| x.protocol.is_tcp() == tcp).map(|x| x.publish).collect() };
    let allows: Vec<Block> = [("tcp", published(true)), ("udp", published(false))]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(proto, v)| allow(proto, &v))
        .collect();
    if !allows.is_empty() {
        out.push(firewall((&id, name), "published", allowed(), allows));
    }
    out
}

/// A firewall rule letting `sources` in to the instances tagged `<env>-<who>` (`(id, who)`:
/// the machine's resource name and its name in the cloud), named after `what`.
fn firewall((id, who): (&str, &str), what: &str, sources: Expression, allows: Vec<Block>) -> Block {
    let mut b = resource("google_compute_firewall", &format!("{id}_{}", res(what)))
        .add_attribute(("project", reference("local.project")))
        .add_attribute(("name", name_with(&format!("-{who}-{what}"))))
        .add_attribute(("network", reference("google_compute_network.env.id")))
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
    /// Its network and address there.
    on: (&'a str, Ipv4Addr),
    /// A shielded VM with a virtual TPM and secure boot (`vm.tpm`).
    tpm: bool,
}

/// An instance at its address with a public one; its user from the `ssh-keys` metadata; the
/// auto-stop scheduled at first boot.
fn instance(i: &Instance) -> Block {
    let (image, _) = image(i.os).unwrap_or_default();
    let user = user(i.os);
    let mut params = Block::builder("initialize_params").add_attribute(("image", string(i.own.unwrap_or(image))));
    if let Some(gb) = i.disk {
        params = params.add_attribute(("size", number(u64::from(gb))));
    }
    let nic = Block::builder("network_interface")
        .add_attribute(("subnetwork", reference(&format!("google_compute_subnetwork.{}.id", res(i.on.0)))))
        .add_attribute(("network_ip", string(i.on.1.to_string())))
        .add_block(Block::builder("access_config").build())
        .build();
    let key = isoloom_render::hcl_builder::Template::text(format!("{user}:")).with_expr(reference("var.ssh_public_key"));
    let b = resource("google_compute_instance", i.id)
        .add_attribute(("project", reference("local.project")))
        .add_attribute(("name", name_with(&format!("-{}", i.who))))
        .add_attribute(("machine_type", string(i.machine_type)))
        .add_attribute(("zone", reference("local.zone")))
        .add_attribute(("tags", list([name_with(&format!("-{}", i.who))])))
        .add_block(Block::builder("boot_disk").add_block(params.build()).build())
        .add_block(nic)
        .add_attribute(("metadata", object([("ssh-keys", key.into_expression())])))
        .add_attribute(("metadata_startup_script", cloud::auto_stop(cloud::shutdown_script(), null())))
        .add_attribute(("labels", cloud::tags(i.env)));
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
        let features = Block::builder("advanced_machine_features").add_attribute(("enable_nested_virtualization", isoloom_render::hcl_builder::boolean(true)));
        b = b.add_block(features.build());
    }
    if i.gpus {
        // A GPU stays with its host: maintenance stops the VM instead of moving it.
        b = b.add_block(Block::builder("scheduling").add_attribute(("on_host_maintenance", string("TERMINATE"))).build());
    }
    b
}

/// The controller: what may reach it (its network, SSH from `allowed_cidr`), its instance at the
/// controller address on the one network (several are refused), its set-up over SSH once every
/// machine is set up.
pub fn controller(env: &Environment, m: &mut Module) {
    let id = "isoloom_controller";
    let Some((net, p)) = env.networks.first() else { return };
    let address = Cidr::parse(&p.cidr).map_or(Ipv4Addr::UNSPECIFIED, Cidr::controller);
    let who = (id, "controller");
    m.commented(
        "The controller (Ansible): its network may reach it, and SSH from allowed_cidr.",
        firewall(who, "net", list([string(p.cidr.as_str())]), vec![allow("all", &[])]),
    );
    m.block(firewall(who, "ssh", list([reference("var.allowed_cidr")]), vec![allow("tcp", &[22])]));
    let (_, memory, disk) = controller::size(env);
    m.block(instance(&Instance {
        env,
        id,
        who: "controller",
        os: controller::OS,
        own: None,
        machine_type: machine_type(memory),
        gpus: false,
        nested: false,
        disk,
        on: (net, address),
        tpm: false,
    }));
    let ssh = controller::Ssh {
        env,
        replace_on: reference(&format!("google_compute_instance.{id}.id")),
        host: public_ip(id),
        user: user(controller::OS).to_string(),
        at_boot: Vec::new(),
        inventory: controller::cloud_inventory(env, |os| user(os).to_string(), isoloom_render::inventory::WinrmAuth::Basic),
    };
    m.block(controller::provisioner(ssh).block());
}
