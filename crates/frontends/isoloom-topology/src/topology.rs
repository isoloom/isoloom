//! The drawing a topology editor sends: machines (the topology's nodes), the LANs drawn around them, and
//! the cables between their ports. Read once into types that hold only what Isoloom can take: an
//! OS Isoloom knows, a port's protocol, an appliance kind.

use std::num::NonZeroU16;

use indexmap::IndexMap;
use isoloom_spec::syntax::{OsRef, PortProtocol};
use isoloom_spec::{Appliance, Tc};
use serde::{Deserialize, Serialize};

/// A project as drawn: its machines, LANs and cables.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Topology {
    /// Kebab-case id of the project, the spec's `name`.
    pub name: String,
    /// The nodes, by name: machines, and automatic routers.
    pub nodes: IndexMap<String, Node>,
    /// The LANs: name → its block. Machines join one through [`Node::networks`].
    #[serde(default)]
    pub networks: IndexMap<String, Lan>,
    /// The cables between two nodes' ports.
    #[serde(default)]
    pub links: Vec<Link>,
}

impl Topology {
    /// The key of the LAN that is VLAN `id` of switch `switch`: what a network the spec nests
    /// under the switch's LAN came from, as the topology names it (a topology that compiles has
    /// at most one).
    #[must_use]
    pub fn vlan_lan(&self, switch: &str, id: u16) -> Option<&str> {
        self.networks
            .iter()
            .find(|(_, l)| l.switch.as_deref() == Some(switch) && l.vlan == Some(id))
            .map(|(name, _)| name.as_str())
    }
}

/// A LAN (or VLAN): one shared segment for every machine on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lan {
    /// A private IPv4 block (10/8, 172.16/12, 192.168/16), /24 to /29 (Isoloom's rule), e.g.
    /// `10.10.1.0/24`.
    pub cidr: String,
    /// Whether machines on it may reach the internet (Isoloom's default: yes).
    #[serde(default = "yes")]
    pub internet: bool,
    /// A VLAN of a switch: the switch (or the first of the switches a trunk joins) it belongs to.
    /// In the spec it nests under that switch's LAN (`sw1.vlan10`). Given with [`Lan::vlan`], or
    /// neither ([`Error::VlanIncomplete`](crate::Error::VlanIncomplete)).
    #[serde(default)]
    pub switch: Option<String>,
    /// A VLAN's id on its switch: one LAN per id of a switch
    /// ([`Error::VlanTaken`](crate::Error::VlanTaken)).
    #[serde(default)]
    pub vlan: Option<u16>,
    /// The switch's machine when it runs a Cisco OS (a node of its own, with no networks):
    /// Isoloom's `switch` on the LAN its VLANs nest under. A VLAN's only
    /// ([`Error::DeviceOffSwitch`](crate::Error::DeviceOffSwitch)).
    #[serde(default)]
    pub device: Option<String>,
    /// How the LAN behaves (a slow or lossy link): Isoloom's `tc`. A VLAN has none
    /// ([`Error::VlanTc`](crate::Error::VlanTc)): Isoloom shapes a whole LAN, not one VLAN.
    #[serde(default)]
    pub tc: Option<Tc>,
}

fn yes() -> bool {
    true
}

/// A node of the drawing: a machine, or an automatic router.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    /// What it is.
    pub kind: Kind,
    /// The machine the user lands on (Isoloom's `access`).
    #[serde(default)]
    pub access: bool,
    /// The ports it answers on.
    #[serde(default)]
    pub services: Vec<Service>,
    /// The LANs it is on: LAN name → the last octet of its address there, or `None` for the
    /// next free one.
    #[serde(default)]
    pub networks: IndexMap<String, Option<u8>>,
}

/// A port a machine answers on (`services` in the editor's JSON), and what it speaks: `tcp` (the default), `udp`, or TCP speaking
/// `http`, `https` or `tls` (Isoloom's port `protocol`).
#[doc(alias = "port")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    /// The port, 1 to 65535.
    pub port: NonZeroU16,
    /// What it speaks.
    #[serde(default)]
    pub protocol: PortProtocol,
}

/// What a node is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
#[non_exhaustive]
pub enum Kind {
    /// A container from a published image: Isoloom's `app.image` (or a network appliance's).
    Container {
        /// The image, by tag or digest. Empty for `cisco-dynamips`, which Isoloom builds.
        image: String,
        /// The image runs no service of its own (a stock Linux image): it is kept running idle,
        /// as a machine to work from. Ignored for an appliance, which runs its network OS.
        #[serde(default)]
        idle: bool,
        /// A network OS Isoloom wires as its image expects (Cisco IOL, as vrnetlab builds it).
        #[serde(default)]
        appliance: Option<Appliance>,
        /// The appliance's own configuration (IOS commands), written to `configs/<name>.cfg`.
        /// Ignored without an `appliance`: a plain container has no configuration file.
        #[serde(default)]
        config: Option<String>,
    },
    /// A machine of an OS, set up by its provisioning steps: Isoloom's `os` plus `app.steps`
    /// (each target decides whether it is a VM or a container built from the OS's image; `vm` is
    /// the editor's JSON name for it).
    Vm {
        /// The OS, `<family>@<version>` (`debian@12`).
        os: OsRef,
        /// Steps run in order on the machine: `.sh` scripts (`.ps1` on Windows), or Ansible
        /// playbooks (`.yml`); a Windows machine's playbooks run from Isoloom's controller instead,
        /// after the machine's own steps.
        /// Each is a file of the project, relative to its folder and inside it (no leading `/`,
        /// no `..`: [`Error::StepPath`](crate::Error::StepPath)).
        #[serde(default)]
        provision: Vec<String>,
    },
    /// An automatic router: not a machine. The networks it is on (LANs, VLANs, cables) can all
    /// reach each other, through Isoloom's own router (`reach` rules both ways); nothing to
    /// configure. A router you configure yourself is a container or a VM (FRR, for instance).
    Router {},
}

/// A cable between two ports (`links` in the editor's JSON, whose names these types keep). A port
/// carries at most one cable.
#[doc(alias = "cable")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    /// One end.
    pub a: Endpoint,
    /// The other end.
    pub b: Endpoint,
    /// How the cable behaves (latency, jitter, loss, bandwidth): Isoloom's `tc` on its network.
    #[serde(default)]
    pub tc: Option<Tc>,
}

/// One end of a cable: a node's port.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    /// The node.
    pub node: String,
    /// The port's name on the node (`eth0`, `GigabitEthernet0/1`, ...).
    pub port: String,
}
