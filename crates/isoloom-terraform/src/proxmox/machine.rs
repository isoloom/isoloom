//! A machine: its cloud-init snippet (its steps, through the core's cloud-init transport) and
//! its VM; and the pieces the router and the controller are made of too.

use isoloom_plan::Environment;
use isoloom_plan::checks::{Position, Probe, by_position};
use isoloom_render::hcl_builder::{Block, Expression, Module, Template, boolean, list, number, reference, string};
use isoloom_render::router;
use isoloom_spec::validate::Cidr;
use isoloom_spec::{DEFAULT_CPUS, DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use crate::vm::cloud_init::{self, CloudConfig};
use crate::vm::{os, res};

/// A machine's snippet and VM.
pub fn blocks(env: &Environment, name: &str, m: &mut Module) {
    let mp = &env.machines[name];
    let mut config = CloudConfig::new(name, reference("local.users"));
    config.packages = ["nftables", "curl", "netcat-openbsd"].map(String::from).to_vec();
    if needs_project(env, name) {
        config.file_lists.push(reference("local.project_files"));
    }
    cloud_init::machine(env, name, &mut config);
    m.commented(&format!("Machine `{name}`."), snippet(&res(name), name, config.document()));
    let mut nics = Vec::new();
    let mut ips = Vec::new();
    for (i, (net, addr)) in mp.addresses.iter().enumerate() {
        nics.push(nic(&format!("proxmox_sdn_vnet.{}.id", res(net)), None));
        let len = Cidr::parse(&env.networks[net].cidr).map_or(32, |c| c.len);
        let gateway = (i == 0).then(|| router::address(env.spec(), net).to_string());
        ips.push(ip_config(&format!("{addr}/{len}"), gateway));
    }
    let mut depends_on = vec![reference("proxmox_virtual_environment_vm.isoloom_router")];
    depends_on.extend(
        mp.depends_on
            .iter()
            .filter(|d| env.machines.get(*d).is_some_and(|dp| dp.shape == isoloom_plan::MachineShape::Vm))
            .map(|d| reference(&format!("proxmox_virtual_environment_vm.{}", res(d)))),
    );
    let vm = Vm {
        id: res(name),
        snippet: res(name),
        name: name.to_string(),
        size: (
            mp.size.cpus.unwrap_or(DEFAULT_CPUS),
            mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB),
            mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB),
        ),
        image: res(os(mp).unwrap_or_default()),
        agent: false,
        nics,
        dns: Some(dns(env, name)),
        ips,
        depends_on,
    };
    m.block(vm.block(env));
}

/// Whether the machine gets the project: it has steps, or a check runner runs a script there.
fn needs_project(env: &Environment, name: &str) -> bool {
    let runs = env.machines[name].lifecycle.iter().any(|s| matches!(s, isoloom_plan::Step::Run { .. }));
    let scripts = by_position(env.spec(), &env.checks)
        .into_iter()
        .any(|(p, g)| p == Position::Machine(name.to_string()) && g.iter().any(|c| matches!(c.probe, Probe::Script { .. })));
    runs || scripts
}

/// The machine's resolver when it sets one (an AD member points at the domain controller),
/// else a public one so it can still install software.
fn dns(env: &Environment, name: &str) -> Block {
    let d = env.spec().machines[name].dns.as_ref();
    let servers: Vec<Expression> = match d.filter(|d| !d.servers.is_empty()) {
        Some(d) => d.servers.iter().map(|s| string(s.as_str())).collect(),
        None => vec![string("1.1.1.1")],
    };
    let mut b = Block::builder("dns").add_attribute(("servers", list(servers)));
    if let Some(domain) = d.and_then(|d| d.domain.as_deref()) {
        b = b.add_attribute(("domain", string(domain)));
    }
    b.build()
}

/// A cloud-init snippet, `iso<slot>-<file>.yaml` on the snippets datastore.
pub fn snippet(id: &str, file: &str, data: Expression) -> Block {
    let source = Block::builder("source_raw")
        .add_attribute((
            "file_name",
            Template::text("iso")
                .with_expr(reference("var.slot"))
                .with_text(format!("-{file}.yaml"))
                .into_expression(),
        ))
        .add_attribute(("data", data))
        .build();
    Block::builder("resource")
        .add_label("proxmox_virtual_environment_file")
        .add_label(id)
        .add_attribute(("node_name", reference("var.node")))
        .add_attribute(("datastore_id", reference("var.snippets_datastore")))
        .add_attribute(("content_type", string("snippets")))
        .add_block(source)
        .build()
}

/// A network device on `bridge`, with a fixed MAC address when given.
pub fn nic(bridge: &str, mac: Option<Expression>) -> Block {
    let mut b = Block::builder("network_device").add_attribute(("bridge", reference(bridge)));
    if let Some(mac) = mac {
        b = b.add_attribute(("mac_address", mac));
    }
    b.build()
}

/// An address (`dhcp`, or `<address>/<len>`), with a gateway on the first network.
pub fn ip_config(address: &str, gateway: Option<String>) -> Block {
    let mut ipv4 = Block::builder("ipv4").add_attribute(("address", string(address)));
    if let Some(g) = gateway {
        ipv4 = ipv4.add_attribute(("gateway", string(g)));
    }
    Block::builder("ip_config").add_block(ipv4.build()).build()
}

/// A VM of the environment: a machine, the router or the controller.
#[derive(Debug, Clone)]
pub struct Vm {
    /// Its resource name.
    pub id: String,
    /// Its snippet's resource name.
    pub snippet: String,
    /// Its name on the server, after `iso<slot>-`.
    pub name: String,
    /// CPUs, memory in MB, disk in GB.
    pub size: (u32, u32, u32),
    /// Its cloud image's resource name.
    pub image: String,
    /// Whether the guest agent reports its addresses.
    pub agent: bool,
    /// Its network devices.
    pub nics: Vec<Block>,
    /// Its resolver.
    pub dns: Option<Block>,
    /// Its addresses, one per device.
    pub ips: Vec<Block>,
    /// What it waits for.
    pub depends_on: Vec<Expression>,
}

impl Vm {
    /// The `proxmox_virtual_environment_vm` block.
    pub fn block(self, env: &Environment) -> Block {
        let (cores, memory, disk) = self.size;
        let mut b = Block::builder("resource")
            .add_label("proxmox_virtual_environment_vm")
            .add_label(self.id.as_str())
            .add_attribute((
                "name",
                Template::text("iso")
                    .with_expr(reference("var.slot"))
                    .with_text(format!("-{}", self.name))
                    .into_expression(),
            ))
            .add_attribute(("node_name", reference("var.node")))
            .add_attribute(("tags", list([string("isoloom"), string(env.name.as_str())])))
            .add_attribute(("on_boot", boolean(false)));
        if self.agent {
            b = b.add_block(Block::builder("agent").add_attribute(("enabled", boolean(true))).build());
        }
        b = b
            .add_block(
                Block::builder("cpu")
                    .add_attribute(("cores", number(cores.into())))
                    .add_attribute(("type", string("host")))
                    .build(),
            )
            .add_block(Block::builder("memory").add_attribute(("dedicated", number(memory.into()))).build())
            .add_block(
                Block::builder("disk")
                    .add_attribute(("datastore_id", reference("var.datastore")))
                    .add_attribute(("file_id", reference(&format!("proxmox_download_file.{}.id", self.image))))
                    .add_attribute(("interface", string("virtio0")))
                    .add_attribute(("size", number(disk.into())))
                    .build(),
            )
            .add_blocks(self.nics);
        let mut init = Block::builder("initialization")
            .add_attribute(("datastore_id", reference("var.datastore")))
            .add_attribute(("user_data_file_id", reference(&format!("proxmox_virtual_environment_file.{}.id", self.snippet))));
        if let Some(d) = self.dns {
            init = init.add_block(d);
        }
        b.add_block(init.add_blocks(self.ips).build())
            .add_block(Block::builder("operating_system").add_attribute(("type", string("l26"))).build())
            .add_block(Block::builder("serial_device").build())
            .add_attribute(("depends_on", list(self.depends_on)))
            .build()
    }
}
