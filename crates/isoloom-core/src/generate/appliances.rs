//! Network appliances on Docker (`docker.appliance`): router and switch OSes in containers,
//! wired the way their image expects.
//!
//! Cisco IOL, as vrnetlab packages it (`vrnetlab/cisco_iol`) and containerlab runs it:
//! - `eth0` is the management port (`Ethernet0/0`, in its own VRF), on Isoloom's management
//!   network; the machine's networks follow, in order, as `eth1`, `eth2`... = `Ethernet0/1`,
//!   `Ethernet0/2`, `Ethernet0/3`, `Ethernet1/0` (four ports a slot);
//! - `/iol/NETMAP` and `/iol/iouyap.ini` map those interfaces to IOL's, `IOL_PID` names the
//!   instance;
//! - the startup configuration (`/iol/config.txt`) is Isoloom's (hostname, `admin`/`admin`,
//!   SSH, the management VRF, each interface with its address from the spec) followed by the
//!   machine's own `config`, joined when the container starts;
//! - IOS owns the addresses: the container's own copy of them is removed (its network sidecar),
//!   so only IOS answers for them.
//!
//! The images are the user's to build from software they are licensed for.

use std::fmt::Write;
use std::net::Ipv4Addr;

use crate::model::{Appliance, Machine, Spec};
use crate::validate::Cidr;

/// Isoloom's management network for appliances, outside the environment (no `reach`, no checks).
pub const MGMT_NETWORK: &str = "isoloom-mgmt";
pub const MGMT_CIDR: &str = "10.255.255.0/24";

/// The appliance machines, in spec order.
pub fn appliances(spec: &Spec) -> Vec<(&String, &Machine, Appliance)> {
    spec.machines.iter().filter_map(|(n, m)| Some((n, m, m.docker.as_ref()?.appliance?))).collect()
}

/// An appliance's management address: .10 and up on the management network.
pub fn mgmt_address(spec: &Spec, name: &str) -> Ipv4Addr {
    let i = appliances(spec).iter().position(|(n, _, _)| n.as_str() == name).unwrap_or(0) as u32;
    Ipv4Addr::from(Cidr::parse(MGMT_CIDR).expect("constant").base + 10 + i)
}

fn mgmt_gateway() -> Ipv4Addr {
    Cidr::parse(MGMT_CIDR).expect("constant").gateway()
}

/// The IOL interface of the `k`th data network (from 1): `Ethernet<k / 4>/<k % 4>`.
pub fn iol_interface(k: usize) -> String {
    format!("Ethernet{}/{}", k / 4, k % 4)
}

fn mask(len: u8) -> Ipv4Addr {
    Ipv4Addr::from(if len == 0 { 0 } else { u32::MAX << (32 - len) })
}

/// The files an appliance's container mounts: (name in its folder, path inside, contents).
pub fn files(spec: &Spec, name: &str, m: &Machine, kind: Appliance, address: impl Fn(&str, u8) -> Ipv4Addr) -> Vec<(String, String, String)> {
    match kind {
        Appliance::CiscoIol | Appliance::CiscoIolL2 => iol_files(spec, name, m, kind == Appliance::CiscoIolL2, address),
    }
}

/// IOL's instance number: unique in the environment (1, 2...).
pub fn iol_pid(spec: &Spec, name: &str) -> usize {
    appliances(spec).iter().position(|(n, _, _)| n.as_str() == name).unwrap_or(0) + 1
}

fn iol_files(spec: &Spec, name: &str, m: &Machine, l2: bool, address: impl Fn(&str, u8) -> Ipv4Addr) -> Vec<(String, String, String)> {
    let pid = iol_pid(spec, name);
    let mgmt = Cidr::parse(MGMT_CIDR).expect("constant");

    let mut iouyap = "[default]\nbase_port = 49000\nnetmap = /iol/NETMAP\n[513:0/0]\neth_dev = eth0\n".to_string();
    let mut netmap = format!("{pid}:0/0 513:0/0\n");
    for k in 1..=m.networks.len() {
        let (slot, port) = (k / 4, k % 4);
        let _ = write!(iouyap, "[513:{slot}/{port}]\neth_dev = eth{k}\n");
        let _ = writeln!(netmap, "{pid}:{slot}/{port} 513:{slot}/{port}");
    }

    let mut cfg = format!(
        "hostname {name}\n!\nno aaa new-model\n!\nip domain name lab\nno ip domain lookup\nip cef\n!\nusername admin privilege 15 secret admin\n!\nvrf definition isoloom-mgmt\n description Isoloom management\n address-family ipv4\n exit-address-family\n!\ninterface Ethernet0/0\n{}vrf forwarding isoloom-mgmt\n description Isoloom management\n ip address {} {}\n no shutdown\n!\n",
        if l2 { " no switchport\n " } else { " " },
        mgmt_address(spec, name),
        mask(mgmt.len),
    );
    for (k, (net, octet)) in m.networks.iter().enumerate() {
        let intf = iol_interface(k + 1);
        let _ = write!(cfg, "interface {intf}\n description {net}\n");
        if !l2 {
            let len = Cidr::parse(&spec.networks[net].cidr).expect("validated cidr").len;
            let _ = writeln!(cfg, " ip address {} {}", address(net, *octet), mask(len));
        }
        cfg.push_str(" no shutdown\n!\n");
    }
    let _ = write!(
        cfg,
        "ip route vrf isoloom-mgmt 0.0.0.0 0.0.0.0 Ethernet0/0 {}\n!\nip ssh version 2\ncrypto key generate rsa modulus 2048\n!\nline vty 0 4\n login local\n transport input ssh\n!\n",
        mgmt_gateway()
    );

    vec![
        ("base.txt".into(), "/iol/base.txt".into(), cfg),
        ("NETMAP".into(), "/iol/NETMAP".into(), netmap),
        ("iouyap.ini".into(), "/iol/iouyap.ini".into(), iouyap),
    ]
}

/// The container's start: the startup configuration (Isoloom's, then the machine's own), then
/// the image's entrypoint.
pub fn entrypoint(kind: Appliance) -> Vec<String> {
    match kind {
        Appliance::CiscoIol | Appliance::CiscoIolL2 => vec![
            "/bin/sh".into(),
            "-c".into(),
            "cat /iol/base.txt /iol/own.txt > /iol/config.txt 2>/dev/null; echo end >> /iol/config.txt; exec /entrypoint.sh".into(),
        ],
    }
}

/// What the network sidecar runs: the container lets go of the data addresses (IOS owns them).
pub fn sidecar_commands(m: &Machine) -> Vec<String> {
    (1..=m.networks.len()).map(|k| format!("ip addr flush dev eth{k}")).collect()
}
