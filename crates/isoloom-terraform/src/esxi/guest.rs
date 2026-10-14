//! A VM of the environment (a machine, the router or the controller) as an `esxi_guest`: its
//! OVA, its size, a network interface per port group with a fixed MAC, and cloud-init's
//! documents in `guestinfo`. The VMware data source reads them: the metadata holds the VM's
//! name and its network configuration (each interface found by its MAC), the user data its
//! user, packages, files and first commands.

use isoloom_render::hcl_builder::{Block, Expression, Template, call, list, number, object, reference, string};

/// One network interface.
#[derive(Debug, Clone)]
pub struct Nic {
    /// The port group's name.
    pub port_group: Expression,
    /// Its address with the prefix length (`10.0.0.10/24`), or DHCP when `None`.
    pub address: Option<String>,
    /// The default route through this interface.
    pub gateway: Option<String>,
}

/// A VM, as the module writes it.
#[derive(Debug, Clone)]
pub struct Guest {
    /// Its resource name.
    pub id: String,
    /// Its name on the host, after `iso<slot>-`, and its host name.
    pub name: String,
    /// Its position among the environment's VMs: the fifth byte of its MACs.
    pub index: usize,
    /// Its OS (the key of its OVA in `images`).
    pub os: String,
    /// CPUs, memory in MB, disk in GB.
    pub size: (u32, u32, u32),
    /// Its interfaces, in order.
    pub nics: Vec<Nic>,
    /// Its resolvers and search domain, on its first interface.
    pub dns: (Vec<String>, Option<String>),
    /// Its cloud-init user data (`#cloud-config`).
    pub user_data: Expression,
    /// How long the provider waits for VMware Tools to report an address, when it should.
    pub wait_for_address: Option<u64>,
    /// What it waits for.
    pub depends_on: Vec<Expression>,
}

/// The MAC of interface `nic` of VM `index`: VMware's range for addresses set by hand, unique
/// per slot.
pub fn mac(index: usize, nic: usize) -> Expression {
    call(
        "format",
        [
            string("00:50:56:%02x:%02x:%02x"),
            reference("var.slot"),
            number(index as u64),
            number(nic as u64),
        ],
    )
}

/// `iso<slot><suffix>`.
pub fn slotted(suffix: &str) -> Expression {
    Template::text("iso").with_expr(reference("var.slot")).with_text(suffix).into_expression()
}

impl Guest {
    /// The `esxi_guest` block.
    pub fn block(self, env_name: &str) -> Block {
        let (cpus, memory, disk) = self.size;
        let encoded = |e: Expression| call("base64encode", [e]);
        let guestinfo = object([
            ("metadata", encoded(call("yamlencode", [self.metadata()]))),
            ("metadata.encoding", string("base64")),
            ("userdata", encoded(self.user_data.clone())),
            ("userdata.encoding", string("base64")),
        ]);
        let mut b = Block::builder("resource")
            .add_label("esxi_guest")
            .add_label(self.id.as_str())
            .add_attribute(("guest_name", slotted(&format!("-{}", self.name))))
            .add_attribute(("notes", string(format!("isoloom: {env_name}"))))
            .add_attribute(("disk_store", reference("var.disk_store")))
            .add_attribute(("ovf_source", call("lookup", [reference("var.images"), string(self.os.as_str())])))
            .add_attribute(("numvcpus", string(cpus.to_string())))
            .add_attribute(("memsize", string(memory.to_string())))
            .add_attribute(("boot_disk_size", string(disk.to_string())))
            .add_attribute(("power", string("on")));
        if let Some(secs) = self.wait_for_address {
            b = b.add_attribute(("guest_startup_timeout", number(secs)));
        }
        for (i, nic) in self.nics.iter().enumerate() {
            b = b.add_block(
                Block::builder("network_interfaces")
                    .add_attribute(("virtual_network", nic.port_group.clone()))
                    .add_attribute(("mac_address", mac(self.index, i)))
                    .add_attribute(("nic_type", string("vmxnet3")))
                    .build(),
            );
        }
        b.add_attribute(("guestinfo", guestinfo))
            .add_attribute(("depends_on", list(self.depends_on)))
            .build()
    }

    /// The metadata: its instance id and host name, and its network configuration (version 2),
    /// each interface matched by its MAC.
    fn metadata(&self) -> Expression {
        let ethernets = self.nics.iter().enumerate().map(|(i, nic)| {
            let mut items = vec![("match", object([("macaddress", mac(self.index, i))]))];
            match &nic.address {
                Some(a) => items.push(("addresses", list([string(a.as_str())]))),
                None => items.push(("dhcp4", isoloom_render::hcl_builder::boolean(true))),
            }
            if let Some(g) = &nic.gateway {
                let route = object([("to", string("0.0.0.0/0")), ("via", string(g.as_str()))]);
                items.push(("routes", list([route])));
            }
            if i == 0 && !self.dns.0.is_empty() {
                let mut ns = vec![("addresses", list(self.dns.0.iter().map(|s| string(s.as_str()))))];
                ns.extend(self.dns.1.as_ref().map(|d| ("search", list([string(d.as_str())]))));
                items.push(("nameservers", object(ns)));
            }
            (format!("nic{i}"), object(items))
        });
        let network = object([("version", number(2)), ("ethernets", object(ethernets))]);
        object([
            ("instance-id", slotted(&format!("-{}", self.name))),
            ("local-hostname", string(self.name.as_str())),
            ("network", network),
        ])
    }
}
