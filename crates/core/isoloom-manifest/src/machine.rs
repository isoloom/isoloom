//! What a run creates: its networks and its machines, Isoloom's own included, each with the role
//! that tells them apart (the runner never infers a role from a name, a label or an image).

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::net::IpAddr;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::identity::Provider;
use crate::names::{Name, OsName};
use crate::path::GuestPath;
use crate::port::Port;
use crate::template::Template;
use crate::{Error, Result};

/// A machine's CPU architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    /// x86-64.
    Amd64,
    /// 64-bit ARM.
    Arm64,
}

/// What a machine is to the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Part of the lab.
    Target,
    /// A one-shot that must exit 0 (an `init:` script).
    Job,
    /// Isoloom's plumbing (the controller, probes, route keepers): never shown, never counted as down.
    Infra,
    /// The user's seat, or its stand-in.
    Access,
    /// An observer (`tools:`).
    Tool,
    /// A check runner (Compose profile `check`).
    Check,
}

/// A network block, IPv4 or IPv6 (`10.30.0.0/24`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cidr {
    address: IpAddr,
    prefix: u8,
}

impl Cidr {
    /// The block written as `<address>/<prefix length>`.
    ///
    /// # Errors
    ///
    /// [`Error::Cidr`] when it isn't an address, a `/` and a prefix length that fits the
    /// address (at most 32 for IPv4, 128 for IPv6).
    pub fn parse(text: &str) -> Result<Cidr> {
        let err = || Error::Cidr { value: text.to_owned() };
        let (address, prefix) = text.split_once('/').ok_or_else(err)?;
        let address: IpAddr = address.parse().map_err(|_| err())?;
        let prefix: u8 = prefix.parse().map_err(|_| err())?;
        let max = if address.is_ipv4() { 32 } else { 128 };
        if prefix > max {
            return Err(err());
        }
        Ok(Cidr { address, prefix })
    }
}

impl FromStr for Cidr {
    type Err = Error;

    fn from_str(text: &str) -> Result<Cidr> {
        Cidr::parse(text)
    }
}

impl fmt::Display for Cidr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.address, self.prefix)
    }
}

impl Serialize for Cidr {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Cidr {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Cidr, D::Error> {
        Cidr::parse(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Cidr {
    fn schema_name() -> Cow<'static, str> {
        "Cidr".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A network block: an IPv4 or IPv6 address and a prefix length (`10.30.0.0/24`).",
            "type": "string",
            "pattern": "^[0-9A-Fa-f.:]+/[0-9]{1,3}$",
        })
    }
}

/// One of the lab's networks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Network {
    /// Its name in the spec.
    pub name: Name,
    /// Its block where the target creates it.
    pub cidr: Cidr,
    /// `false`: no route out. On container targets Isoloom enforces it with `<machine>-routes`
    /// sidecars (role `infra`), not an `internal:` network, so published ports still work.
    pub internet: bool,
    /// Its name where the target creates it: the Docker network (`${run.name}_lab`), the
    /// hypervisor's internal network (`isoloom-<lab>-lab`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<Template>,
}

/// A machine's size, when the spec sets one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    /// CPUs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpus: Option<u32>,
    /// Memory, in MB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
}

/// A machine's name where the target creates it: what status and orphan clean-up match, exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Native {
    /// A Compose service.
    Compose {
        /// The service's name.
        service: Name,
    },
    /// A Vagrant machine.
    Vagrant {
        /// Its `config.vm.define` name.
        vagrant: Name,
        /// Its VM's name under each provider.
        vm_names: BTreeMap<Provider, String>,
    },
}

/// [`Native`] as written: one shape, told apart by its keys.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Native",
    description = "A machine's name where the target creates it: the Compose `service`, or the `vagrant` machine with its VM name per provider (what orphan clean-up matches, exactly).",
    extend("oneOf" = [{ "required": ["service"] }, { "required": ["vagrant"] }], "dependentRequired" = { "vm_names": ["vagrant"] })
)]
struct NativeFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    service: Option<Name>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vagrant: Option<Name>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vm_names: Option<BTreeMap<Provider, String>>,
}

impl Serialize for Native {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let fields = match self {
            Native::Compose { service } => NativeFields {
                service: Some(service.clone()),
                vagrant: None,
                vm_names: None,
            },
            Native::Vagrant { vagrant, vm_names } => NativeFields {
                service: None,
                vagrant: Some(vagrant.clone()),
                vm_names: Some(vm_names.clone()),
            },
        };
        fields.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Native {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Native, D::Error> {
        match NativeFields::deserialize(d)? {
            NativeFields {
                service: Some(service),
                vagrant: None,
                vm_names: None,
            } => Ok(Native::Compose { service }),
            NativeFields {
                service: None,
                vagrant: Some(vagrant),
                vm_names,
            } => Ok(Native::Vagrant {
                vagrant,
                vm_names: vm_names.unwrap_or_default(),
            }),
            _ => Err(serde::de::Error::custom(Error::NativeShape)),
        }
    }
}

impl JsonSchema for Native {
    fn schema_name() -> Cow<'static, str> {
        "Native".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        NativeFields::json_schema(generator)
    }
}

/// A named volume of a machine. `down` removes it; `park` and `resume` keep it (a flag written
/// once by an init job lives in one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Volume {
    /// Its name in the spec.
    pub name: Name,
    /// Where the machine mounts it.
    pub path: GuestPath,
    /// Its name where the target creates it (`${run.name}_web-ctf`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<Template>,
}

/// A machine the target creates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Machine {
    /// Its name: the spec's, or Isoloom's for its own (`web-init-1`, `isoloom-check`).
    pub name: Name,
    /// What it is to the run.
    pub role: Role,
    /// The machine a job, a route keeper or a check runner belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<Name>,
    /// Its operating system, for a VM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsName>,
    /// It runs Windows.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub windows: bool,
    /// Its CPU architecture, for the lab's own machines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<Arch>,
    /// Its size, when the spec sets one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<Resources>,
    /// Its static address on each lab network, by network name (what the status shows).
    pub addresses: BTreeMap<Name, IpAddr>,
    /// Infra that stays on once the lab is built (a controller with `keep_running`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_running: bool,
    /// Its name where the target creates it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<Native>,
    /// Its named volumes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<Volume>,
    /// The ports it answers on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<Port>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_has_an_address_and_a_prefix() {
        assert_eq!(Cidr::parse("10.30.0.0/24").unwrap().to_string(), "10.30.0.0/24");
        assert_eq!(Cidr::parse("fd00:30::/64").unwrap().to_string(), "fd00:30::/64");
        assert_eq!(Cidr::parse("10.30.0.1/32").unwrap().to_string(), "10.30.0.1/32");
        assert_eq!("fd00::1/128".parse::<Cidr>().unwrap().to_string(), "fd00::1/128");
        for bad in ["10.30.0.0", "10.30.0.0/33", "fd00::/129", "lab/24", "10.30.0.0/x"] {
            assert_eq!(Cidr::parse(bad).unwrap_err().code(), "cidr", "{bad}");
        }
    }

    #[test]
    fn native_is_a_service_or_a_vagrant_machine() {
        let c: Native = serde_json::from_str(r#"{"service": "db"}"#).unwrap();
        assert!(matches!(c, Native::Compose { .. }));
        let v: Native = serde_json::from_str(r#"{"vagrant": "dc01", "vm_names": {"hyperv": "mini-ad-dc01"}}"#).unwrap();
        assert!(matches!(v, Native::Vagrant { ref vm_names, .. } if vm_names[&Provider::Hyperv] == "mini-ad-dc01"));
        let both = serde_json::from_str::<Native>(r#"{"service": "db", "vagrant": "db"}"#).unwrap_err();
        assert!(both.to_string().contains("not both"), "{both}");
    }
}
