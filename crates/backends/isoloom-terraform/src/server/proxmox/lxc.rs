//! LXC containers: a machine with no `vm:` whose `docker:` opts in (`docker.lxc: true`) runs as
//! a container of the server, from its image pulled as an OCI template (Proxmox VE 9.1 or
//! later). Opt-in, because an LXC container is no Docker container: Proxmox runs the image's
//! entrypoint with the machine's `environment`, at its addresses, with the router as its gateway,
//! behind the same firewall as a VM, and nothing more. What it can't carry is refused.
//!
//! A container has no cloud-init to write the environment's names into its hosts file, so the
//! router answers them (dnsmasq, each name at its address on the asking network) and is the
//! container's resolver.

use std::borrow::Cow;

use isoloom_plan::checks::Position;
use isoloom_plan::{Environment, MachinePlan, MachineShape};
use isoloom_render::hcl_builder::{Block, Expression, Module, boolean, list, number, object, reference, string};
use isoloom_render::router;
use isoloom_spec::model::{Arch, Machine};
use isoloom_spec::validate::Cidr;
use isoloom_spec::{DEFAULT_CPUS, DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::firewall::{self, Guest};
use crate::machine::cloud_init::file;
use crate::module::ident;
use crate::module::variable::Variable;
use crate::server::slotted;

/// A machine that runs as an LXC container.
#[derive(Debug, Clone, Copy)]
struct Container<'a> {
    /// Its name.
    name: &'a str,
    /// Its plan.
    plan: &'a MachinePlan,
    /// What the spec says of it.
    machine: &'a Machine,
}

/// The machines that run as LXC containers.
fn containers(env: &Environment) -> impl Iterator<Item = Container<'_>> {
    env.machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Container)
        .filter_map(|(name, plan)| {
            Some(Container {
                name,
                plan,
                machine: env.spec().machines.get(name)?,
            })
        })
}

/// Whether the environment has any.
pub(super) fn needed(env: &Environment) -> bool {
    containers(env).next().is_some()
}

/// What a container can't have, if the machine asks for it.
fn lacking(env: &Environment, c: Container) -> Option<&'static str> {
    let Container { name, plan: mp, machine: m } = c;
    let at = Position::Machine(name.to_owned());
    let checked = env.checks.iter().any(|check| check.position == at && !check.derived);
    let init = m.docker.as_ref().is_some_and(|d| !d.init.is_empty());
    [
        (mp.arch != Arch::Amd64, "an arm64 image (a container runs the server's own architecture)"),
        (!mp.volumes.is_empty(), "`volumes` (a container's mount point goes with it)"),
        (!mp.disks.is_empty(), "data disks (`disks`: nothing formats or mounts them in it)"),
        (
            !mp.files.is_empty() || !mp.secrets.is_empty(),
            "`files` or `secrets` (nothing delivers them into it)",
        ),
        (!mp.inputs.is_empty(), "inputs in its `environment`"),
        (!mp.settings.sysctls.is_empty() || !mp.settings.ulimits.is_empty(), "`sysctls` or `ulimits`"),
        (m.read_only || !m.tmpfs.is_empty() || m.shm_size.is_some(), "`read_only`, `tmpfs` or `shm_size`"),
        (init, "`setup` jobs"),
        (!mp.process.is_empty(), "a `command`, `before` steps or a `healthcheck` of its own"),
        (mp.hardware.gpus.is_some() || !mp.hardware.devices.is_empty(), "`gpus` or `devices`"),
        (!mp.addresses6.is_empty() || !mp.outside.is_empty(), "IPv6 or a network outside the environment"),
        (checked, "checks of the spec run from it (it has no SSH for the check runner)"),
    ]
    .into_iter()
    .find_map(|(asks, what)| asks.then_some(what))
}

/// The environment without the checks Isoloom derives from a container's position (it has no
/// SSH for their runner): the same `reach` is still probed from the VMs. The spec's own checks
/// there are refused instead.
pub(super) fn checked(env: &Environment) -> Cow<'_, Environment> {
    let at = |c: &isoloom_plan::checks::Resolved| matches!(&c.position, Position::Machine(m) if env.machines.get(m).is_some_and(|mp| mp.shape == MachineShape::Container));
    if !env.checks.iter().any(|c| c.derived && at(c)) {
        return Cow::Borrowed(env);
    }
    let mut out = env.clone();
    out.checks.retain(|c| !(c.derived && at(c)));
    Cow::Owned(out)
}

/// Why a container can't run, if one can't.
pub(super) fn refusal(env: &Environment) -> Option<String> {
    containers(env).find_map(|c| {
        lacking(env, c).map(|what| {
            format!(
                "machine `{}` runs as an LXC container (`overrides.proxmox.machines.{}.lxc`), which can't have {what}: run it as a VM (leave `lxc` out)",
                c.name, c.name
            )
        })
    })
}

/// The OCI templates' datastore, when there are containers.
pub(super) fn variables(env: &Environment) -> Vec<Variable> {
    if !needed(env) {
        return Vec::new();
    }
    vec![
        Variable::new("template_datastore", "string")
            .default(string("local"))
            .describe("A datastore with 'vztmpl' content, for the containers' OCI images"),
    ]
}

/// Each container: its image, pulled as a template, then the container and its firewall.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    for c in containers(env) {
        let name = c.name;
        let reference_of = c.machine.docker.as_ref().and_then(|d| d.image.clone()).unwrap_or_default();
        let image = Block::builder("resource")
            .add_label("proxmox_oci_image")
            .add_label(ident(name))
            .add_attribute(("node_name", reference("var.node")))
            .add_attribute(("datastore_id", reference("var.template_datastore")))
            .add_attribute(("reference", string(oci_reference(&reference_of))))
            .add_attribute(("file_name", slotted(&format!("-{name}.tar"))))
            .add_attribute(("overwrite_unmanaged", boolean(true)))
            .build();
        m.commented(&format!("Machine `{name}`: an LXC container (`overrides.proxmox`), from its image."), image);
        m.block(container(env, c));
        firewall::guest(env, name, Guest::Container, m);
    }
}

/// An image as Proxmox pulls it: in full, Docker Hub's registry and `library/` spelled out
/// (`nginx:1.27` is `docker.io/library/nginx:1.27`).
fn oci_reference(image: &str) -> String {
    let first = image.split('/').next().unwrap_or_default();
    let has_registry = image.contains('/') && (first.contains('.') || first.contains(':') || first == "localhost");
    match (has_registry, image.contains('/')) {
        (true, _) => image.to_owned(),
        (false, true) => format!("docker.io/{image}"),
        (false, false) => format!("docker.io/library/{image}"),
    }
}

/// The container: its size, its devices and addresses, its entrypoint and environment.
fn container(env: &Environment, c: Container) -> Block {
    let Container { name, plan: mp, machine: m } = c;
    let mut b = Block::builder("resource")
        .add_label("proxmox_virtual_environment_container")
        .add_label(ident(name))
        .add_attribute(("node_name", reference("var.node")))
        .add_attribute(("tags", list([string("isoloom"), string(env.name.as_str())])))
        .add_attribute(("pool_id", reference(super::base::POOL)))
        .add_attribute(("unprivileged", boolean(!m.privileged)))
        .add_attribute(("start_on_boot", boolean(false)))
        .add_block(
            Block::builder("cpu")
                .add_attribute(("cores", number(mp.size.cpus.unwrap_or(DEFAULT_CPUS).into())))
                .build(),
        )
        .add_block(
            Block::builder("memory")
                .add_attribute(("dedicated", number(mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB).into())))
                .build(),
        )
        .add_block(
            Block::builder("disk")
                .add_attribute(("datastore_id", reference("var.datastore")))
                .add_attribute(("size", number(mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB).into())))
                .build(),
        );
    for (i, net) in mp.addresses.keys().enumerate() {
        let device = Block::builder("network_interface").add_attribute(("name", string(format!("eth{i}"))));
        b = b.add_block(super::links::on(env, net, device, Some(mp)).build());
    }
    if !mp.settings.environment.is_empty() {
        let vars = mp.settings.environment.iter().map(|(k, v)| (k.as_str(), string(v.as_str())));
        b = b.add_attribute(("environment_variables", object(vars)));
    }
    b.add_block(initialization(env, c))
        .add_block(
            Block::builder("operating_system")
                .add_attribute(("template_file_id", reference(&format!("proxmox_oci_image.{}.id", ident(name)))))
                .build(),
        )
        .add_attribute(("depends_on", list(super::machine::depends_on(env, mp))))
        .build()
}

/// Its name, its resolver (the router, for the environment's names), an address per device
/// with the router as the gateway, and an idle image kept running.
fn initialization(env: &Environment, c: Container) -> Block {
    let Container { name, plan: mp, machine: m } = c;
    let spec = env.spec();
    let first = mp.addresses.keys().next().map(|n| router::address(spec, n).to_string()).unwrap_or_default();
    let mut dns = Block::builder("dns").add_attribute(("servers", list([string(first.as_str())])));
    if let Some(domain) = m.dns.as_ref().and_then(|d| d.domain.as_deref()) {
        dns = dns.add_attribute(("domain", string(domain)));
    }
    let mut b = Block::builder("initialization")
        .add_attribute(("hostname", string(name)))
        .add_block(dns.build());
    for (i, (net, addr)) in mp.addresses.iter().enumerate() {
        let len = env.networks.get(net).and_then(|p| Cidr::parse(&p.cidr)).map_or(32, |cidr| cidr.len);
        let mut ipv4 = Block::builder("ipv4").add_attribute(("address", string(format!("{addr}/{len}"))));
        if i == 0 {
            ipv4 = ipv4.add_attribute(("gateway", string(first.as_str())));
        }
        b = b.add_block(Block::builder("ip_config").add_block(ipv4.build()).build());
    }
    if m.docker.as_ref().is_some_and(|d| d.idle) {
        b = b.add_attribute(("entrypoint", string("sleep infinity")));
    }
    b.build()
}

/// The router's name server for the containers: dnsmasq on its network addresses, answering
/// the environment's names from a hosts file, the rest from a public resolver. The package, its
/// files and the command that starts it.
pub(super) fn router_dns(env: &Environment) -> Option<(Vec<Expression>, &'static str)> {
    if !needed(env) {
        return None;
    }
    let spec = env.spec();
    let listen = env
        .networks
        .keys()
        .map(|net| format!("listen-address={}\n", router::address(spec, net)))
        .collect::<Vec<_>>()
        .concat();
    let conf = format!("bind-dynamic\nno-resolv\nserver=1.1.1.1\nlocalise-queries\naddn-hosts=/etc/isoloom/hosts\n{listen}");
    let hosts = env
        .machines
        .values()
        .flat_map(|mp| mp.addresses.values().map(|addr| format!("{addr} {}\n", mp.names.join(" "))))
        .collect::<Vec<_>>()
        .concat();
    let files = vec![
        file("/etc/dnsmasq.d/isoloom.conf", None, string(conf)),
        file("/etc/isoloom/hosts", None, string(hosts)),
    ];
    Some((files, "systemctl restart dnsmasq"))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::oci_reference;

    #[test]
    fn images_are_pulled_by_their_full_reference() {
        assert_eq!(oci_reference("nginx:1.27"), "docker.io/library/nginx:1.27");
        assert_eq!(oci_reference("bitnami/redis"), "docker.io/bitnami/redis");
        assert_eq!(oci_reference("ghcr.io/o/app:1"), "ghcr.io/o/app:1");
        assert_eq!(oci_reference("localhost:5000/app"), "localhost:5000/app");
    }

    proptest! {
        #[test]
        fn a_reference_in_full_stays_as_it_is(image in "[a-z0-9]{1,8}(/[a-z0-9]{1,8}){0,2}(:[a-z0-9.]{1,6})?") {
            let full = oci_reference(&image);
            prop_assert!(full.ends_with(&image), "{full}");
            prop_assert_eq!(oci_reference(&full), full);
        }
    }
}
