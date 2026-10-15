//! Network appliances on Docker (`docker.appliance`): router and switch OSes in containers,
//! wired the way their image expects. The images are the user's to build from software they
//! are licensed for; Isoloom ships none.
//!
//! Every kind: `eth0` is the management port, on Isoloom's management network; the machine's
//! networks follow in order as `eth1`, `eth2`... (Compose `priority`); the startup configuration
//! is Isoloom's (hostname, `admin`/`admin`, each interface with its address from the spec, for a
//! router) followed by the machine's own `config`, joined when the container starts; and IOS
//! owns the addresses (the container's own copy is removed by its network sidecar).
//!
//! - **Cisco IOL** (`vrnetlab/cisco_iol`, as containerlab runs it): `Ethernet0/0` management in
//!   its own VRF, data `Ethernet0/1`, `0/2`, `0/3`, `1/0`... (four a slot); `/iol/NETMAP` and
//!   `/iol/iouyap.ini` map the interfaces, `IOL_PID` names the instance.
//! - **QEMU images** (vrnetlab's `cisco_vios`, `cisco_viosl2`, `cisco_csr1000v`,
//!   `cisco_c8000v`): `launch.py` with containerlab's arguments (`tc` connection mode), the
//!   number of data interfaces in `CLAB_INTFS`, the startup configuration in
//!   `/config/startup-config.cfg` (vrnetlab applies it once booted), privileged for /dev/kvm.
//!   IOSv: data `GigabitEthernet0/1`...; IOS XE: data `GigabitEthernet2`...
//! - **Dynamips** (a 7200): Isoloom builds the container (Ubuntu's `dynamips`), binds each data
//!   interface to a router port (`FastEthernet0/0`, then `1/0`, `1/1`, `2/0`... on PA-2FE-TX
//!   adapters) and boots the IOS `.bin` in `firmware`.
//!
//! A LAN's `switch` (a switch kind, no `networks` of its own) takes its ports from the LAN
//! instead: an access port on each VLAN's network (at the controller address, unused on Docker,
//! as Isoloom's Linux switch had), then a trunk port on each trunk machine's link (see
//! `trunks`). Its configuration declares the VLANs (VTP transparent) and sets each port's mode.

use std::fmt::Write;
use std::net::Ipv4Addr;

use super::trunks;
use crate::model::{Appliance, Machine, Spec};
use crate::validate::Cidr;
use crate::vlans;
pub use isoloom_spec::appliance::{MGMT_CIDR, is_switch};

/// Isoloom's management network for appliances, outside the environment (no `reach`, no checks).
pub const MGMT_NETWORK: &str = "isoloom-mgmt";

/// The appliance machines, in spec order.
pub fn appliances(spec: &Spec) -> Vec<(&String, &Machine, Appliance)> {
    spec.machines.iter().filter_map(|(n, m)| Some((n, m, m.docker.as_ref()?.appliance?))).collect()
}

/// [`MGMT_CIDR`] as a block, written out so reading it can't fail (a test checks they agree).
const MGMT: Cidr = Cidr {
    base: u32::from_be_bytes([10, 255, 255, 0]),
    len: 24,
};

/// An appliance's management address: .10 and up on the management network.
pub fn mgmt_address(spec: &Spec, name: &str) -> Ipv4Addr {
    let i = appliances(spec).iter().position(|(n, _, _)| n.as_str() == name).unwrap_or(0);
    Ipv4Addr::from(MGMT.base + 10 + u32::try_from(i).unwrap_or(0))
}

fn mgmt_gateway() -> Ipv4Addr {
    MGMT.gateway()
}

/// IOL's instance number: unique in the environment (1, 2...).
pub fn iol_pid(spec: &Spec, name: &str) -> usize {
    appliances(spec).iter().position(|(n, _, _)| n.as_str() == name).unwrap_or(0) + 1
}

/// The OS's name for the `k`th data interface (from 1, `eth<k>` in the container).
pub fn interface(kind: Appliance, k: usize) -> String {
    match kind {
        Appliance::CiscoIol | Appliance::CiscoIolL2 => format!("Ethernet{}/{}", k / 4, k % 4),
        Appliance::CiscoVios | Appliance::CiscoViosL2 => format!("GigabitEthernet0/{k}"),
        Appliance::CiscoCsr1000v | Appliance::CiscoC8000v => format!("GigabitEthernet{}", k + 1),
        Appliance::CiscoDynamips => {
            let (slot, port) = dynamips_port(k);
            format!("FastEthernet{slot}/{port}")
        }
    }
}

/// A 7200's port for the `k`th data interface: the I/O card's `0/0`, then two a PA-2FE-TX.
fn dynamips_port(k: usize) -> (usize, usize) {
    if k == 1 { (0, 0) } else { (k / 2, k % 2) }
}

fn mask(len: u8) -> Ipv4Addr {
    Ipv4Addr::from(if len == 0 { 0 } else { u32::MAX << (32 - len) })
}

/// One data interface of an appliance: `eth1`, `eth2`... in order.
pub struct Port {
    /// The Compose network it is on.
    pub network: String,
    /// The container's address there (none on a trunk's link, which has no addresses).
    pub address: Option<Ipv4Addr>,
    /// What the port carries: the network itself, one VLAN, or a trunk.
    pub role: Role,
}

/// What a data interface is for.
pub enum Role {
    /// One of the machine's own networks (a router's interface takes its address).
    Network,
    /// A LAN switch's access port in a VLAN.
    Access(u16),
    /// A LAN switch's trunk to a machine on several of its VLANs: that machine, the VLAN ids.
    Trunk(String, Vec<u16>),
}

/// The data interfaces of appliance `name`, in order: its networks; for a LAN's `switch`, an
/// access port per VLAN of the LAN, then a trunk port per machine on several of them.
pub fn ports(spec: &Spec, name: &str, m: &Machine, address: &impl Fn(&str, u8) -> Ipv4Addr) -> Vec<Port> {
    let Some(lan) = vlans::switched_by(spec, name) else {
        return m
            .networks
            .iter()
            .map(|(net, octet)| Port {
                network: net.clone(),
                address: Some(address(net, *octet)),
                role: Role::Network,
            })
            .collect();
    };
    let mut out: Vec<Port> = vlans::of_lan(spec, lan)
        .map(|(net, id)| Port {
            network: net.clone(),
            address: Some(trunks::switch_address(spec, net)),
            role: Role::Access(id),
        })
        .collect();
    for t in trunks::trunks(spec).iter().filter(|t| t.lan == lan) {
        out.push(Port {
            network: trunks::link_network(t),
            address: None,
            role: Role::Trunk(t.machine.clone(), t.vlans.iter().map(|(_, id, _)| *id).collect()),
        });
    }
    out
}

/// How an appliance's container is set up.
pub struct Wiring {
    /// Its data interfaces, in order (`eth1`, `eth2`...).
    pub ports: Vec<Port>,
    /// Files to mount: (name in its folder, path inside, contents).
    pub files: Vec<(String, String, String)>,
    /// Where the machine's own `config` is mounted (read when the container starts).
    pub own_config: &'static str,
    /// Where the `firmware` file is mounted, for kinds that boot one.
    pub firmware: Option<&'static str>,
    /// The container's environment variables, in order.
    pub environment: Vec<(String, String)>,
    /// What the container runs: joins the startup configuration, then starts the OS.
    pub entrypoint: Vec<String>,
    /// QEMU needs /dev/kvm (and the image sets up its own taps).
    pub privileged: bool,
    /// The container Isoloom builds for the kind (its Dockerfile), if any.
    pub build: Option<String>,
}

/// The startup configuration Isoloom writes: hostname, credentials, each interface's address.
fn base_config(spec: &Spec, name: &str, kind: Appliance, ports: &[Port]) -> String {
    let mut cfg = format!("hostname {name}\n!\nno ip domain lookup\nip domain name lab\n!\nusername admin privilege 15 secret admin\n!\n");
    if matches!(kind, Appliance::CiscoIol | Appliance::CiscoIolL2) {
        let mgmt = MGMT;
        let _ = write!(
            cfg,
            "no aaa new-model\nip cef\n!\nvrf definition isoloom-mgmt\n description Isoloom management\n address-family ipv4\n exit-address-family\n!\ninterface Ethernet0/0\n{}vrf forwarding isoloom-mgmt\n description Isoloom management\n ip address {} {}\n no shutdown\n!\n",
            if is_switch(kind) { " no switchport\n " } else { " " },
            mgmt_address(spec, name),
            mask(mgmt.len),
        );
    }
    // A LAN's switch: its VLANs, kept in the configuration (VTP transparent).
    let access: Vec<(&str, u16)> = ports
        .iter()
        .filter_map(|p| match p.role {
            Role::Access(id) => Some((p.network.as_str(), id)),
            _ => None,
        })
        .collect();
    if !access.is_empty() {
        cfg.push_str("vtp mode transparent\n!\n");
        for (net, id) in &access {
            // IOS takes VLAN names of up to 32 characters.
            let _ = write!(cfg, "vlan {id}\n name {}\n!\n", net.chars().take(32).collect::<String>());
        }
    }
    for (k, port) in ports.iter().enumerate() {
        let _ = writeln!(cfg, "interface {}", interface(kind, k + 1));
        match &port.role {
            Role::Network => {
                let _ = writeln!(cfg, " description {}", port.network);
                // The spec is validated: its blocks parse.
                let block = Cidr::parse(&spec.networks[&port.network].cidr);
                if let (false, Some(addr), Some(block)) = (is_switch(kind), port.address, block) {
                    let _ = writeln!(cfg, " ip address {addr} {}", mask(block.len));
                }
            }
            Role::Access(id) => {
                let _ = write!(cfg, " description {}\n switchport mode access\n switchport access vlan {id}\n", port.network);
            }
            Role::Trunk(machine, ids) => {
                let ids = ids.iter().map(u16::to_string).collect::<Vec<_>>().join(",");
                let _ = write!(
                    cfg,
                    " description trunk to {machine}\n switchport trunk encapsulation dot1q\n switchport mode trunk\n switchport trunk allowed vlan {ids}\n"
                );
            }
        }
        cfg.push_str(" no shutdown\n!\n");
    }
    if matches!(kind, Appliance::CiscoIol | Appliance::CiscoIolL2) {
        let _ = write!(cfg, "ip route vrf isoloom-mgmt 0.0.0.0 0.0.0.0 Ethernet0/0 {}\n!\n", mgmt_gateway());
    }
    cfg.push_str("ip ssh version 2\n!\nline vty 0 4\n login local\n transport input ssh telnet\n!\n");
    cfg
}

/// How the container of appliance `name` is wired.
pub fn wiring(spec: &Spec, name: &str, m: &Machine, kind: Appliance, address: impl Fn(&str, u8) -> Ipv4Addr) -> Wiring {
    let ports = ports(spec, name, m, &address);
    let cfg = base_config(spec, name, kind, &ports);
    match kind {
        Appliance::CiscoIol | Appliance::CiscoIolL2 => iol_wiring(iol_pid(spec, name), ports, cfg),
        Appliance::CiscoVios | Appliance::CiscoViosL2 | Appliance::CiscoCsr1000v | Appliance::CiscoC8000v => vrnetlab_wiring(name, kind, ports, cfg),
        Appliance::CiscoDynamips => dynamips_wiring(ports, cfg),
    }
}

/// IOL: the interfaces mapped by `/iol/NETMAP` and `/iol/iouyap.ini`, the instance in `IOL_PID`.
fn iol_wiring(pid: usize, ports: Vec<Port>, cfg: String) -> Wiring {
    let mut iouyap = "[default]\nbase_port = 49000\nnetmap = /iol/NETMAP\n[513:0/0]\neth_dev = eth0\n".to_string();
    let mut netmap = format!("{pid}:0/0 513:0/0\n");
    for k in 1..=ports.len() {
        let (slot, port) = (k / 4, k % 4);
        let _ = write!(iouyap, "[513:{slot}/{port}]\neth_dev = eth{k}\n");
        let _ = writeln!(netmap, "{pid}:{slot}/{port} 513:{slot}/{port}");
    }
    Wiring {
        ports,
        files: vec![
            ("base.txt".into(), "/iol/base.txt".into(), cfg),
            ("NETMAP".into(), "/iol/NETMAP".into(), netmap),
            ("iouyap.ini".into(), "/iol/iouyap.ini".into(), iouyap),
        ],
        own_config: "/iol/own.txt",
        firmware: None,
        environment: vec![("IOL_PID".into(), pid.to_string())],
        entrypoint: shell("cat /iol/base.txt /iol/own.txt > /iol/config.txt 2>/dev/null; echo end >> /iol/config.txt; exec /entrypoint.sh"),
        privileged: false,
        build: None,
    }
}

/// vrnetlab's QEMU images: `launch.py` with containerlab's arguments, privileged for /dev/kvm.
fn vrnetlab_wiring(name: &str, kind: Appliance, ports: Vec<Port>, cfg: String) -> Wiring {
    let mut environment = vec![("CLAB_INTFS".into(), ports.len().to_string()), ("CONNECTION_MODE".into(), "tc".into())];
    if matches!(kind, Appliance::CiscoVios | Appliance::CiscoViosL2) {
        environment.push(("CLAB_MGMT_PASSTHROUGH".into(), "true".into()));
    }
    Wiring {
        ports,
        files: vec![("base.cfg".into(), "/config/base.cfg".into(), cfg)],
        own_config: "/config/own.cfg",
        firmware: None,
        environment,
        entrypoint: shell(&format!(
            "cat /config/base.cfg /config/own.cfg > /config/startup-config.cfg 2>/dev/null; echo end >> /config/startup-config.cfg; exec uv run /launch.py --username admin --password admin --hostname {name} --connection-mode tc --trace"
        )),
        privileged: true,
        build: None,
    }
}

/// Dynamips: each data interface bound to a router port, the IOS image booted from `firmware`.
fn dynamips_wiring(ports: Vec<Port>, cfg: String) -> Wiring {
    let n = ports.len();
    // PA-2FE-TX adapters in the slots the data interfaces reach past the I/O card's port.
    let slots = (2..=n).map(|k| dynamips_port(k).0).max().unwrap_or(0);
    let mut args = String::from("-P 7200 -r 512 -T 2000 -C /config/startup-config.cfg");
    for slot in 1..=slots {
        let _ = write!(args, " -p {slot}:PA-2FE-TX");
    }
    for k in 1..=n {
        let (slot, port) = dynamips_port(k);
        let _ = write!(args, " -s {slot}:{port}:linux_eth:eth{k}");
    }
    Wiring {
        ports,
        files: vec![("base.cfg".into(), "/config/base.cfg".into(), cfg)],
        own_config: "/config/own.cfg",
        firmware: Some("/firmware/ios.bin"),
        environment: vec![("DYNAMIPS_ARGS".into(), args)],
        entrypoint: shell(
            "cat /config/base.cfg /config/own.cfg > /config/startup-config.cfg 2>/dev/null; echo end >> /config/startup-config.cfg; cd /tmp && exec dynamips $DYNAMIPS_ARGS /firmware/ios.bin",
        ),
        privileged: false,
        build: Some(
            "# Dynamips, the Cisco 7200 emulator (Ubuntu's package, in its default sources); the IOS\n# image is the user's.\nFROM ubuntu:24.04\nRUN apt-get update && apt-get install -y --no-install-recommends dynamips iproute2 && rm -rf /var/lib/apt/lists/*\n".into(),
        ),
    }
}

fn shell(script: &str) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), script.into()]
}

/// What the network sidecar runs: the container lets go of the data addresses on its `ports`
/// data interfaces (IOS owns them).
pub fn sidecar_commands(ports: usize) -> Vec<String> {
    (1..=ports).map(|k| format!("ip addr flush dev eth{k}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_management_block_is_the_shared_constant() {
        assert_eq!(Cidr::parse(MGMT_CIDR), Some(MGMT));
    }
}
