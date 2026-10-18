//! A machine: its cloud-init snippet (its steps, through the core's cloud-init transport), or a
//! Windows machine's cloudbase-init script, and its VM; and the pieces the router and the
//! controller are made of too.

use isoloom_plan::checks::{Position, Probe, by_position};
use isoloom_plan::{Environment, MachinePlan, MachineShape};
use isoloom_render::hcl_builder::{Block, Expression, Module, boolean, list, number, reference, string};
use isoloom_render::router;
use isoloom_spec::validate::Cidr;
use isoloom_spec::{DEFAULT_CPUS, DEFAULT_DISK_GB, DEFAULT_MEMORY_MB};

use super::windows;
use crate::machine::cloud_init::{self, CloudConfig};
use crate::machine::cloudbase_init;
use crate::module::{ident, resource};
use crate::server::{router as lab_router, slotted};

/// A machine's snippet and VM: a cloud-init document on a cloud image, or on Windows a
/// cloudbase-init script on a clone of the OS's template (see `windows`).
pub(super) fn blocks(env: &Environment, name: &str, mp: &MachinePlan, m: &mut Module) {
    let boot = if mp.is_windows() {
        m.commented(
            &format!("Machine `{name}`: its set-up, run by cloudbase-init."),
            snippet_file(&ident(name), &format!("{name}.ps1"), cloudbase_init::user_data(env, (name, mp))),
        );
        windows::boot(mp)
    } else {
        let mut config = CloudConfig::new(name, reference("local.users"));
        let domain = env.spec().machines.get(name).and_then(|machine| machine.dns.as_ref()?.domain.as_deref());
        config.fqdn = domain.map(|d| format!("{name}.{d}"));
        config.packages = ["nftables", "curl", "netcat-openbsd"].map(String::from).to_vec();
        if needs_project(env, (name, mp)) {
            config.file_lists.push(reference("local.project_files"));
        }
        config.files.extend(super::virtiofs::tag_files(env, name));
        cloud_init::machine(env, (name, mp), &mut config);
        m.commented(&format!("Machine `{name}`."), snippet(&ident(name), name, config.document()));
        super::image::boot(name, mp).unwrap_or_else(|| Boot::Image(super::arch::image_res(mp)))
    };
    m.block(vm(env, name, mp, boot).block(env));
    super::firewall::guest(env, name, super::firewall::Guest::Vm, m);
}

/// The VM of machine `name`, booting from `boot`: an interface and an address per network, the
/// router as its gateway, after the router and the VMs it depends on.
fn vm(env: &Environment, name: &str, mp: &MachinePlan, boot: Boot) -> Vm {
    let mut nics = Vec::new();
    let mut ips = Vec::new();
    // The default route is the router's on the first network, unless the LAN's DHCP gives it.
    let routed = lab_router::is_default_gateway(env, mp);
    for (i, (net, addr)) in mp.addresses.iter().enumerate() {
        nics.push(super::links::device(env, net, None, Some(mp)));
        let len = env.networks.get(net).and_then(|n| Cidr::parse(&n.cidr)).map_or(32, |c| c.len);
        let gateway = (i == 0 && routed).then(|| router::address(env.spec(), net).to_string());
        ips.push(ip_config(&format!("{addr}/{len}"), gateway));
    }
    for (device, ip) in super::outside::interfaces(env, mp) {
        nics.push(device);
        ips.push(ip);
    }
    Vm {
        id: ident(name),
        snippet: ident(name),
        name: name.to_owned(),
        size: (
            mp.size.cpus.unwrap_or(DEFAULT_CPUS),
            mp.size.memory_mb.unwrap_or(DEFAULT_MEMORY_MB),
            mp.size.disk_gb.unwrap_or(DEFAULT_DISK_GB),
        ),
        boot,
        tpm: mp.tpm.is_some(),
        own_boot: mp.boot.clone(),
        arm: super::arch::is_arm(mp),
        agent: None,
        nics,
        pci: super::gpus::hostpci(mp.hardware.gpus.as_ref()),
        dns: Some(dns(env, name)),
        ips,
        depends_on: depends_on(env, mp),
        data_disks: mp.disks.clone(),
        folders: super::virtiofs::devices(env, name),
    }
}

/// What a machine waits for: the router, then the machines it depends on (VMs and containers).
pub(super) fn depends_on(env: &Environment, mp: &MachinePlan) -> Vec<Expression> {
    let mut out = vec![reference("proxmox_virtual_environment_vm.isoloom_router")];
    out.extend(mp.depends_on.iter().filter_map(|d| {
        let kind = match env.machines.get(d)?.shape {
            MachineShape::Vm => "vm",
            MachineShape::Container => "container",
            _ => return None,
        };
        Some(reference(&format!("proxmox_virtual_environment_{kind}.{}", ident(d))))
    }));
    out
}

/// Whether the machine gets the project: its steps read it, or a check runner runs a script there.
fn needs_project(env: &Environment, (name, mp): (&str, &MachinePlan)) -> bool {
    let runs = isoloom_plan::machine_files::reads_project(&mp.lifecycle);
    let scripts = by_position(env.spec(), &env.checks)
        .into_iter()
        .any(|(p, g)| p == Position::Machine(name.to_owned()) && g.iter().any(|c| matches!(c.probe, Probe::Script { .. })));
    runs || scripts
}

/// The machine's resolver when it sets one (an AD member points at the domain controller),
/// else a public one so it can still install software. Its search domain is its own, else the
/// private zone's (`dns.private`), whose names its hosts file carries.
fn dns(env: &Environment, name: &str) -> Block {
    let d = env.spec().machines.get(name).and_then(|m| m.dns.as_ref());
    let servers: Vec<Expression> = match d.filter(|d| !d.servers.is_empty()) {
        Some(d) => d.servers.iter().map(|s| string(s.as_str())).collect(),
        None => vec![string("1.1.1.1")],
    };
    let mut b = Block::builder("dns").add_attribute(("servers", list(servers)));
    let zone = env.private_dns.as_ref().map(|z| z.domain.as_str());
    if let Some(domain) = d.and_then(|d| d.domain.as_deref()).or(zone) {
        b = b.add_attribute(("domain", string(domain)));
    }
    b.build()
}

/// A cloud-init snippet, `iso<slot>-<file>.yaml` on the snippets datastore.
pub(super) fn snippet(id: &str, file: &str, data: Expression) -> Block {
    snippet_file(id, &format!("{file}.yaml"), data)
}

/// A snippet named `iso<slot>-<file_name>` on the snippets datastore.
pub(super) fn snippet_file(id: &str, file_name: &str, data: Expression) -> Block {
    let source = Block::builder("source_raw")
        .add_attribute(("file_name", slotted(&format!("-{file_name}"))))
        .add_attribute(("data", data))
        .build();
    resource("proxmox_virtual_environment_file", id)
        .add_attribute(("node_name", reference("var.node")))
        .add_attribute(("datastore_id", reference("var.snippets_datastore")))
        .add_attribute(("content_type", string("snippets")))
        .add_block(source)
        .build()
}

/// A network device on `bridge`, with a fixed MAC address when given.
pub(super) fn nic(bridge: &str, mac: Option<Expression>) -> Block {
    let mut b = Block::builder("network_device").add_attribute(("bridge", reference(bridge)));
    if let Some(mac) = mac {
        b = b.add_attribute(("mac_address", mac));
    }
    b.build()
}

/// An address (`dhcp`, or `<address>/<len>`), with a gateway on the first network.
pub(super) fn ip_config(address: &str, gateway: Option<String>) -> Block {
    let mut ipv4 = Block::builder("ipv4").add_attribute(("address", string(address)));
    if let Some(g) = gateway {
        ipv4 = ipv4.add_attribute(("gateway", string(g)));
    }
    Block::builder("ip_config").add_block(ipv4.build()).build()
}

/// What a VM boots from.
#[derive(Debug, Clone)]
pub(super) enum Boot {
    /// A cloud image (its `proxmox_download_file` resource's name), as a new disk.
    Image(String),
    /// A full clone of a Linux template on the node, by id (`vm.image.proxmox`): its disk grown
    /// to the machine's size, cloud-init as on a cloud image.
    Clone(u32),
    /// A full clone of a Windows template (`var.<variable>`): UEFI, cloudbase-init reading its
    /// configuration drive. The template's disk is kept, grown to `disk` GB when the spec sizes it.
    Template {
        /// The variable holding the template's VM id.
        variable: String,
        /// Its OS type, as Proxmox names it (`win11`).
        ostype: &'static str,
        /// The disk size the spec asks for.
        disk: Option<u32>,
    },
}

/// A data disk: an empty one after the VM's own (`virtio1` on), its serial its name, which the
/// machine's cloud-init finds it by.
fn data_disk(i: usize, d: &isoloom_plan::DataDisk) -> Block {
    Block::builder("disk")
        .add_attribute(("datastore_id", reference("var.datastore")))
        .add_attribute(("interface", string(format!("virtio{}", i + 1))))
        .add_attribute(("size", number(d.size_gb.into())))
        .add_attribute(("serial", string(d.name.as_str())))
        .add_attribute(("file_format", string("raw")))
        .build()
}

/// A VM of the environment: a machine, the router or the controller.
#[derive(Debug, Clone)]
pub(super) struct Vm {
    /// Its resource name.
    pub(super) id: String,
    /// Its snippet's resource name.
    pub(super) snippet: String,
    /// Its name on the server, after `iso<slot>-`.
    pub(super) name: String,
    /// CPUs, memory in MB, disk in GB (a cloud image's disk).
    pub(super) size: (u32, u32, u32),
    /// What it boots from.
    pub(super) boot: Boot,
    /// Whether it has a virtual TPM 2.0 (its state on the VM datastore).
    pub(super) tpm: bool,
    /// Its own firmware and boot order (`vm.boot`).
    pub(super) own_boot: Option<isoloom_spec::model::Boot>,
    /// Whether it is an aarch64 VM, emulated (`arch: arm64`).
    pub(super) arm: bool,
    /// When the guest agent reports its uplink address (the router): its lab addresses, which
    /// don't count as one.
    pub(super) agent: Option<Vec<String>>,
    /// Its network devices.
    pub(super) nics: Vec<Block>,
    /// The server's PCI devices passed through to it (its GPUs), on a q35 machine.
    pub(super) pci: Vec<Block>,
    /// Its resolver.
    pub(super) dns: Option<Block>,
    /// Its addresses, one per device.
    pub(super) ips: Vec<Block>,
    /// What it waits for.
    pub(super) depends_on: Vec<Expression>,
    /// Its data disks, after its own.
    pub(super) data_disks: Vec<isoloom_plan::DataDisk>,
    /// The server's folders shared into it (`virtiofs` devices).
    pub(super) folders: Vec<Block>,
}

impl Vm {
    /// The `proxmox_virtual_environment_vm` block.
    pub(super) fn block(mut self, env: &Environment) -> Block {
        let (cores, memory, _) = self.size;
        let mut b = resource("proxmox_virtual_environment_vm", &self.id)
            .add_attribute(("name", slotted(&format!("-{}", self.name))))
            .add_attribute(("node_name", reference("var.node")))
            .add_attribute(("tags", list([string("isoloom"), string(env.name.as_str())])))
            .add_attribute(("pool_id", reference(super::base::POOL)))
            .add_attribute(("on_boot", boolean(false)));
        let template = matches!(self.boot, Boot::Template { .. });
        if template {
            // Windows 11 and its TPM need UEFI on a Q35 machine; the template is made so.
            b = b.add_attribute(("bios", string("ovmf")));
        } else if self.arm {
            b = super::arch::arm(b);
        }
        if template || !self.pci.is_empty() {
            // Q35 also gives a passed-through device its PCIe slot (i440fx has none).
            b = b.add_attribute(("machine", string("q35")));
        }
        b = super::boot::add(b, self.own_boot.as_ref());
        if let Some(own) = &self.agent {
            // A missing agent must not hold a destroy either: the VM is stopped, not shut down.
            b = b
                .add_block(super::uplink::agent())
                .add_attribute(("stop_on_destroy", boolean(true)))
                .add_block(super::uplink::checked(&format!("VM `{}`", self.name), own));
        }
        b = b
            .add_block(super::arch::cpu(cores, self.arm))
            .add_block(Block::builder("memory").add_attribute(("dedicated", number(memory.into()))).build())
            .add_blocks(self.disks())
            .add_blocks(self.data_disks.iter().enumerate().map(|(i, d)| data_disk(i, d)))
            .add_blocks(std::mem::take(&mut self.nics))
            .add_blocks(std::mem::take(&mut self.pci))
            .add_blocks(std::mem::take(&mut self.folders))
            .add_block(self.initialization());
        let ostype = match self.boot {
            Boot::Image(_) | Boot::Clone(_) => "l26",
            Boot::Template { ostype, .. } => ostype,
        };
        b = b.add_block(Block::builder("operating_system").add_attribute(("type", string(ostype))).build());
        if !template {
            b = b.add_block(Block::builder("serial_device").build());
        }
        b.add_attribute(("depends_on", list(self.depends_on))).build()
    }

    /// Its cloud-init drive: the snippet, the resolver, an address per device.
    fn initialization(&mut self) -> Block {
        let mut init = Block::builder("initialization").add_attribute(("datastore_id", reference("var.datastore")));
        if let Boot::Template { .. } = self.boot {
            // cloudbase-init reads an OpenStack configuration drive.
            init = init.add_attribute(("type", string("configdrive2")));
        }
        if self.arm {
            init = init.add_attribute(("interface", string("scsi1")));
        }
        init = init.add_attribute(("user_data_file_id", reference(&format!("proxmox_virtual_environment_file.{}.id", self.snippet))));
        if let Some(d) = self.dns.take() {
            init = init.add_block(d);
        }
        init.add_blocks(std::mem::take(&mut self.ips)).build()
    }

    /// Its disks: a cloud image's, or a template's clone (with its EFI variables), then its TPM.
    fn disks(&self) -> Vec<Block> {
        let datastore = || reference("var.datastore");
        // A cloud image's disk is made from its file; a clone's is the template's, sized.
        let disk = |file: Option<Expression>, gb: u32| {
            let mut b = Block::builder("disk").add_attribute(("datastore_id", datastore()));
            if let Some(f) = file {
                b = b.add_attribute(("file_id", f));
            }
            b.add_attribute(("interface", string("virtio0")))
                .add_attribute(("size", number(gb.into())))
                .build()
        };
        let mut out = Vec::new();
        match &self.boot {
            Boot::Image(image) => {
                out.push(disk(Some(reference(&format!("proxmox_download_file.{image}.id"))), self.size.2));
                out.extend(self.arm.then(super::arch::efi_disk));
            }
            Boot::Clone(id) => {
                out.push(super::image::clone(number((*id).into())));
                out.push(disk(None, self.size.2));
            }
            Boot::Template { variable, disk: size, .. } => {
                out.push(
                    Block::builder("clone")
                        .add_attribute(("vm_id", reference(&format!("var.{variable}"))))
                        .add_attribute(("full", boolean(true)))
                        .build(),
                );
                out.extend(size.map(|gb| disk(None, gb)));
                out.push(super::arch::efi_disk());
            }
        }
        if self.tpm {
            out.push(
                Block::builder("tpm_state")
                    .add_attribute(("datastore_id", datastore()))
                    .add_attribute(("version", string("v2.0")))
                    .build(),
            );
        }
        out
    }
}
