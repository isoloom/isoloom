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

/// IOL's instance number: unique in the environment (1, 2...).
pub fn iol_pid(spec: &Spec, name: &str) -> usize {
    appliances(spec).iter().position(|(n, _, _)| n.as_str() == name).unwrap_or(0) + 1
}

/// Whether the appliance is a switch (its interfaces take no address).
pub fn is_switch(kind: Appliance) -> bool {
    matches!(kind, Appliance::CiscoIolL2 | Appliance::CiscoViosL2)
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

/// How an appliance's container is set up.
pub struct Wiring {
    /// Files to mount: (name in its folder, path inside, contents).
    pub files: Vec<(String, String, String)>,
    /// Where the machine's own `config` is mounted (read when the container starts).
    pub own_config: &'static str,
    /// Where the `firmware` file is mounted, for kinds that boot one.
    pub firmware: Option<&'static str>,
    pub environment: Vec<(String, String)>,
    pub entrypoint: Vec<String>,
    /// QEMU needs /dev/kvm (and the image sets up its own taps).
    pub privileged: bool,
    /// The container Isoloom builds for the kind (its Dockerfile), if any.
    pub build: Option<String>,
}

/// The startup configuration Isoloom writes: hostname, credentials, each interface's address.
fn base_config(spec: &Spec, name: &str, m: &Machine, kind: Appliance, address: &impl Fn(&str, u8) -> Ipv4Addr) -> String {
    let mut cfg = format!("hostname {name}\n!\nno ip domain lookup\nip domain name lab\n!\nusername admin privilege 15 secret admin\n!\n");
    if matches!(kind, Appliance::CiscoIol | Appliance::CiscoIolL2) {
        let mgmt = Cidr::parse(MGMT_CIDR).expect("constant");
        let _ = write!(
            cfg,
            "no aaa new-model\nip cef\n!\nvrf definition isoloom-mgmt\n description Isoloom management\n address-family ipv4\n exit-address-family\n!\ninterface Ethernet0/0\n{}vrf forwarding isoloom-mgmt\n description Isoloom management\n ip address {} {}\n no shutdown\n!\n",
            if is_switch(kind) { " no switchport\n " } else { " " },
            mgmt_address(spec, name),
            mask(mgmt.len),
        );
    }
    for (k, (net, octet)) in m.networks.iter().enumerate() {
        let _ = write!(cfg, "interface {}\n description {net}\n", interface(kind, k + 1));
        if !is_switch(kind) {
            let len = Cidr::parse(&spec.networks[net].cidr).expect("validated cidr").len;
            let _ = writeln!(cfg, " ip address {} {}", address(net, *octet), mask(len));
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
    let cfg = base_config(spec, name, m, kind, &address);
    let n = m.networks.len();
    match kind {
        Appliance::CiscoIol | Appliance::CiscoIolL2 => {
            let pid = iol_pid(spec, name);
            let mut iouyap = "[default]\nbase_port = 49000\nnetmap = /iol/NETMAP\n[513:0/0]\neth_dev = eth0\n".to_string();
            let mut netmap = format!("{pid}:0/0 513:0/0\n");
            for k in 1..=n {
                let (slot, port) = (k / 4, k % 4);
                let _ = write!(iouyap, "[513:{slot}/{port}]\neth_dev = eth{k}\n");
                let _ = writeln!(netmap, "{pid}:{slot}/{port} 513:{slot}/{port}");
            }
            Wiring {
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
        Appliance::CiscoVios | Appliance::CiscoViosL2 | Appliance::CiscoCsr1000v | Appliance::CiscoC8000v => {
            let mut environment = vec![("CLAB_INTFS".into(), n.to_string()), ("CONNECTION_MODE".into(), "tc".into())];
            if matches!(kind, Appliance::CiscoVios | Appliance::CiscoViosL2) {
                environment.push(("CLAB_MGMT_PASSTHROUGH".into(), "true".into()));
            }
            Wiring {
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
        Appliance::CiscoDynamips => {
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
    }
}

fn shell(script: &str) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), script.into()]
}

/// What the network sidecar runs: the container lets go of the data addresses (IOS owns them).
pub fn sidecar_commands(m: &Machine) -> Vec<String> {
    (1..=m.networks.len()).map(|k| format!("ip addr flush dev eth{k}")).collect()
}
