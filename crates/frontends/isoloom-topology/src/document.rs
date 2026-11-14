//! The spec as the compiler writes it: the part of Isoloom's authored format a drawing uses, in
//! the order and shape a person would write it (defaults left out). Isoloom's own types only read
//! that format, so these write it; every document is read back by Isoloom before it is handed out.

use std::num::NonZeroU16;

use indexmap::IndexMap;
use isoloom_spec::net::Cidr;
use isoloom_spec::syntax::{OsRef, PortName, PortProtocol};
use isoloom_spec::{Appliance, Tc};
use serde::{Serialize, Serializer};

/// A value as its text: a block as the spec writes it (`10.10.1.0/24`).
fn as_text<S: Serializer, T: std::fmt::Display>(value: &T, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

/// A spec, version 1.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct SpecDoc {
    pub(crate) version: u8,
    pub(crate) name: String,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) networks: IndexMap<String, NetworkDoc>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) reach: Vec<ReachDoc>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) machines: IndexMap<String, MachineDoc>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) provision: Vec<ProvisionDoc>,
}

/// A step of the whole project: an Ansible playbook run from Isoloom's controller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ProvisionDoc {
    pub(crate) ansible: String,
}

/// A network: its block, and what differs from Isoloom's defaults.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct NetworkDoc {
    #[serde(serialize_with = "as_text")]
    pub(crate) cidr: Cidr,
    /// `Some(false)` when its machines may not reach the internet; Isoloom's default, yes, isn't
    /// written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) internet: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) gateway: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) switch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tc: Option<Tc>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) vlans: IndexMap<u16, VlanDoc>,
}

impl NetworkDoc {
    /// A plain network: a block, whether it reaches the internet, how its link behaves.
    pub(crate) fn new(cidr: Cidr, internet: bool, tc: Option<Tc>) -> NetworkDoc {
        NetworkDoc {
            cidr,
            internet: (!internet).then_some(false),
            gateway: None,
            switch: None,
            tc,
            vlans: IndexMap::new(),
        }
    }

    /// Whether its machines may reach the internet.
    pub(crate) fn reaches_internet(&self) -> bool {
        self.internet.unwrap_or(true)
    }

    /// Says whether its machines may reach the internet (only a no is written).
    pub(crate) fn set_internet(&mut self, internet: bool) {
        self.internet = (!internet).then_some(false);
    }
}

/// A VLAN of a LAN: its block, and its `internet` when it differs from the LAN's.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct VlanDoc {
    #[serde(serialize_with = "as_text")]
    pub(crate) cidr: Cidr,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) internet: Option<bool>,
}

/// Traffic allowed from one network to another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ReachDoc {
    pub(crate) from: String,
    pub(crate) to: String,
}

/// A machine: what it is (`os`, `app`), its own command, where it sits and what it answers on.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub(crate) struct MachineDoc {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) os: Option<OsRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) app: Option<AppDoc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) networks: IndexMap<String, u8>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub(crate) ports: IndexMap<PortName, PortDoc>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) access: bool,
}

/// A custom app's recipe: an image, an appliance, or portable steps.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub(crate) struct AppDoc {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) appliance: Option<Appliance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) config: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) firmware: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) steps: Vec<StepDoc>,
}

/// A portable step: a script, or an Ansible playbook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StepDoc {
    /// `{ run: <script> }`.
    Run(String),
    /// `{ playbook: <file> }`.
    Playbook(String),
}

impl Serialize for StepDoc {
    // A one-key map (`run: x`), not the YAML tag serde gives an enum's variant.
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let (key, value) = match self {
            StepDoc::Run(script) => ("run", script),
            StepDoc::Playbook(file) => ("playbook", file),
        };
        let mut map = s.serialize_map(Some(1))?;
        map.serialize_entry(key, value)?;
        map.end()
    }
}

/// A port: its number alone (TCP), or with what it speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub(crate) enum PortDoc {
    /// A TCP port.
    Tcp(NonZeroU16),
    /// A port speaking `protocol`.
    Speaking { port: NonZeroU16, protocol: PortProtocol },
}
