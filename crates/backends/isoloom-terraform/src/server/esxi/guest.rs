//! A VM of the environment (a machine, the router or the controller) as an `esxi_guest`: its
//! OVA, its size, its firmware and guest OS type when set, a network interface per port group
//! with a fixed MAC, its data disks by slot, and the guest's documents in `guestinfo`.
//! cloud-init's VMware data source reads them on Linux, cloudbase-init's VMware guestinfo
//! service on Windows: the metadata holds the VM's name and its network configuration (each
//! interface found by its MAC), the user data its set-up.

use isoloom_render::hcl_builder::{Block, Expression, boolean, call, list, number, object, reference, string};

use crate::module::resource;
use crate::server::slotted;

/// One network interface.
#[derive(Debug, Clone)]
pub(super) struct Nic {
    /// The port group's name.
    pub(super) port_group: Expression,
    /// Its addresses with their prefix length (`10.0.0.10/24`), or DHCP when there is none.
    pub(super) addresses: Vec<String>,
    /// The default route through this interface.
    pub(super) gateway: Option<String>,
    /// The network's MTU, when it sets one.
    pub(super) mtu: Option<u16>,
    /// With DHCP: whether the LAN's routes are taken (its default route among them).
    pub(super) dhcp_routes: bool,
}

impl Nic {
    /// An interface on `port_group` at `address` (DHCP when `None`), with no route of its own
    /// and the default MTU.
    pub(super) fn at(port_group: Expression, address: Option<String>) -> Nic {
        Nic {
            port_group,
            addresses: address.into_iter().collect(),
            gateway: None,
            mtu: None,
            dhcp_routes: true,
        }
    }
}

/// A VM, as the module writes it.
#[derive(Debug, Clone)]
pub(super) struct Guest {
    /// Its resource name.
    pub(super) id: String,
    /// Its name on the host, after `iso<slot>-`, and its host name.
    pub(super) name: String,
    /// Its position among the environment's VMs: the fifth byte of its MACs.
    pub(super) index: u64,
    /// Its OS (the key of its OVA in `images`).
    pub(super) os: String,
    /// CPUs, memory in MB, disk in GB.
    pub(super) size: (u32, u32, u32),
    /// Its interfaces, in order.
    pub(super) nics: Vec<Nic>,
    /// Its resolvers and search domain, on its first interface.
    pub(super) dns: (Vec<String>, Option<String>),
    /// Its user data: a `#cloud-config` document, or a cloudbase-init script on Windows.
    pub(super) user_data: Expression,
    /// How long the provider waits for VMware Tools to report an address, when it should.
    pub(super) wait_for_address: Option<u64>,
    /// What it waits for.
    pub(super) depends_on: Vec<Expression>,
    /// Its firmware (`bios` or `efi`), when the machine sets it; else its OVA's.
    pub(super) firmware: Option<&'static str>,
    /// Its guest OS type, when its OVA's isn't kept (Windows).
    pub(super) guestos: Option<&'static str>,
    /// Its data disks' `esxi_virtual_disk` ids, attached in order from slot 0:1.
    pub(super) disks: Vec<Expression>,
    /// Whether it runs Windows: cloudbase-init reads its network configuration.
    pub(super) windows: bool,
    /// Its `lifecycle` block, when the apply checks what it reports (the router's uplink).
    pub(super) lifecycle: Option<Block>,
}

/// The SCSI slots a data disk can take: 0:1 to 0:15 but 0:7, the controller's own.
fn slots() -> impl Iterator<Item = String> {
    (1..=15).filter(|i| *i != 7).map(|i| format!("0:{i}"))
}

/// The most data disks a VM takes: one per free slot.
pub(super) const MAX_DISKS: usize = 14;

/// The MAC of interface `nic` of VM `index`: VMware's range for addresses set by hand, unique
/// per slot.
fn mac(index: u64, nic: u64) -> Expression {
    call("format", [string("00:50:56:%02x:%02x:%02x"), reference("var.slot"), number(index), number(nic)])
}

impl Guest {
    /// The `esxi_guest` block.
    pub(super) fn block(self, env_name: &str) -> Block {
        let (cpus, memory, disk) = self.size;
        let encoded = |e: Expression| call("base64encode", [e]);
        let guestinfo = object([
            ("metadata", encoded(call("yamlencode", [self.metadata()]))),
            ("metadata.encoding", string("base64")),
            ("userdata", encoded(self.user_data.clone())),
            ("userdata.encoding", string("base64")),
        ]);
        let mut b = resource("esxi_guest", &self.id)
            .add_attribute(("guest_name", slotted(&format!("-{}", self.name))))
            .add_attribute(("notes", string(format!("isoloom: {env_name}"))))
            .add_attribute(("disk_store", reference("var.disk_store")))
            .add_attribute(("ovf_source", call("lookup", [reference("var.images"), string(self.os.as_str())])))
            .add_attribute(("numvcpus", string(cpus.to_string())))
            .add_attribute(("memsize", string(memory.to_string())))
            .add_attribute(("boot_disk_size", string(disk.to_string())))
            .add_attribute(("power", string("on")));
        if let Some(f) = self.firmware {
            b = b.add_attribute(("boot_firmware", string(f)));
        }
        if let Some(g) = self.guestos {
            b = b.add_attribute(("guestos", string(g)));
        }
        if let Some(secs) = self.wait_for_address {
            b = b.add_attribute(("guest_startup_timeout", number(secs)));
        }
        for (i, nic) in (0..).zip(&self.nics) {
            b = b.add_block(
                Block::builder("network_interfaces")
                    .add_attribute(("virtual_network", nic.port_group.clone()))
                    .add_attribute(("mac_address", mac(self.index, i)))
                    .add_attribute(("nic_type", string("vmxnet3")))
                    .build(),
            );
        }
        for (id, slot) in self.disks.iter().zip(slots()) {
            b = b.add_block(
                Block::builder("virtual_disks")
                    .add_attribute(("virtual_disk_id", id.clone()))
                    .add_attribute(("slot", string(slot)))
                    .build(),
            );
        }
        b = b.add_attribute(("guestinfo", guestinfo));
        if let Some(lifecycle) = self.lifecycle {
            b = b.add_block(lifecycle);
        }
        b.add_attribute(("depends_on", list(self.depends_on))).build()
    }

    /// The metadata: its instance id and host name, and its network configuration (version 2),
    /// each interface matched by its MAC.
    fn metadata(&self) -> Expression {
        let ethernets = (0..).zip(&self.nics).map(|(i, nic)| (format!("nic{i}"), self.ethernet(i, nic)));
        let network = object([("version", number(2)), ("ethernets", object(ethernets))]);
        object([
            ("instance-id", slotted(&format!("-{}", self.name))),
            ("local-hostname", string(self.name.as_str())),
            ("network", network),
        ])
    }

    /// Interface `i`'s network configuration: its addresses (or DHCP), its default route (as
    /// `gateway4` on Windows, the form cloudbase-init reads), its MTU, and the resolvers on the
    /// first.
    fn ethernet(&self, i: u64, nic: &Nic) -> Expression {
        let mut items = vec![("match", object([("macaddress", mac(self.index, i))]))];
        if nic.addresses.is_empty() {
            items.push(("dhcp4", boolean(true)));
            if !nic.dhcp_routes {
                items.push(("dhcp4-overrides", object([("use-routes", boolean(false))])));
            }
        } else {
            items.push(("addresses", list(nic.addresses.iter().map(|a| string(a.as_str())))));
        }
        match (&nic.gateway, self.windows) {
            (Some(g), true) => items.push(("gateway4", string(g.as_str()))),
            (Some(g), false) => {
                let route = object([("to", string("0.0.0.0/0")), ("via", string(g.as_str()))]);
                items.push(("routes", list([route])));
            }
            (None, _) => {}
        }
        if let Some(mtu) = nic.mtu {
            items.push(("mtu", number(mtu.into())));
        }
        if i == 0 && !self.dns.0.is_empty() {
            let mut ns = vec![("addresses", list(self.dns.0.iter().map(|s| string(s.as_str()))))];
            ns.extend(self.dns.1.as_ref().map(|d| ("search", list([string(d.as_str())]))));
            items.push(("nameservers", object(ns)));
        }
        object(items)
    }
}
