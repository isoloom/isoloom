//! What a run creates: its networks and its machines, Isoloom's own included, each with the role
//! that tells them apart (the runner never infers a role from a name, a label or an image).

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::net::IpAddr;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::console::Console;
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
    /// One of the environment's own machines: one the spec describes.
    Member,
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

/// A network block, IPv4 or IPv6 (`10.30.0.0/24`): its first address, with no bit set past the
/// prefix, and a prefix length that fits the address.
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
    /// address (at most 32 for IPv4, 128 for IPv6), or the address has a bit set past the prefix
    /// (`10.30.0.1/24`: an address in the block, not the block).
    pub fn parse(text: &str) -> Result<Cidr> {
        let err = || Error::Cidr { value: text.to_owned() };
        let (address, prefix) = text.split_once('/').ok_or_else(err)?;
        let address: IpAddr = address.parse().map_err(|_| err())?;
        let prefix: u8 = prefix.parse().map_err(|_| err())?;
        let width = if address.is_ipv4() { 32 } else { 128 };
        // The address's bits past the prefix (a shift by the whole width leaves none: `/32`).
        let host_bits = match address {
            IpAddr::V4(a) => u32::from(a).checked_shl(u32::from(prefix)).is_some_and(|rest| rest != 0),
            IpAddr::V6(a) => u128::from(a).checked_shl(u32::from(prefix)).is_some_and(|rest| rest != 0),
        };
        if prefix > width || host_bits {
            return Err(err());
        }
        Ok(Cidr { address, prefix })
    }

    /// The IPv4 block of `prefix` bits holding `address`: its bits past the prefix cleared, a
    /// prefix past 32 taken as 32. Total, so a block already parsed (the spec's
    /// `isoloom_spec::net::Cidr`) converts without going through text.
    #[must_use]
    pub fn ipv4(address: std::net::Ipv4Addr, prefix: u8) -> Cidr {
        let prefix = prefix.min(32);
        let mask = u32::MAX.checked_shl(32 - u32::from(prefix)).unwrap_or(0);
        Cidr {
            address: IpAddr::V4(std::net::Ipv4Addr::from(u32::from(address) & mask)),
            prefix,
        }
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

/// What a [`Cidr`] looks like: an IPv4 address and a prefix length up to 32, or an IPv6 address
/// and one up to 128.
const CIDR_PATTERN: &str =
    r"^(?:[0-9]{1,3}(?:\.[0-9]{1,3}){3}/(?:3[0-2]|[12][0-9]|[0-9])|[0-9A-Fa-f.:]*:[0-9A-Fa-f.:]*/(?:12[0-8]|1[01][0-9]|[1-9][0-9]|[0-9]))$";

impl JsonSchema for Cidr {
    fn schema_name() -> Cow<'static, str> {
        "Cidr".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A network block: its first IPv4 or IPv6 address, with no bit set past the prefix (which the pattern can't check), and a prefix length that fits the address (`10.30.0.0/24`).",
            "type": "string",
            "pattern": CIDR_PATTERN,
        })
    }
}

/// One of the environment's networks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Network {
    /// Its name in the spec.
    pub name: Name,
    /// Its block where the target creates it.
    pub cidr: Cidr,
    /// `false`: no route out. On container targets Isoloom enforces it with
    /// `isoloom-routes-<machine>` sidecars (role `infra`), not an `internal:` network, so
    /// published ports still work.
    pub internet: bool,
    /// Its name where the target creates it: the Docker network (`${run.name}_<network>`), the
    /// hypervisor's internal network (`isoloom-<environment>-<network>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<Template>,
}

/// A machine's size.
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
        /// A digest of what Vagrant applies to the VM only when it boots (`vagrant up` of a
        /// stopped VM, `vagrant reload`): its Vagrantfile configuration but its provisioners,
        /// such as its synced folders, networks, forwarded ports, size and provider settings.
        /// Opaque: a runner compares it with the one the VM was last booted with and reloads a
        /// running VM whose digest changed; `vagrant up --provision` alone would not apply it.
        boot_digest: Option<String>,
    },
}

/// [`Native`] as written: one shape, told apart by its keys.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Native",
    description = "A machine's name where the target creates it: the Compose `service`, or the `vagrant` machine with its VM name per provider (what orphan clean-up matches, exactly) and the digest of what Vagrant applies to it only at boot (`boot_digest`).",
    extend("oneOf" = [{ "required": ["service"] }, { "required": ["vagrant"] }], "dependentRequired" = { "vm_names": ["vagrant"], "boot_digest": ["vagrant"] })
)]
struct NativeFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    service: Option<Name>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vagrant: Option<Name>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vm_names: Option<BTreeMap<Provider, String>>,
    /// A digest of the VM's Vagrantfile configuration but its provisioners: what Vagrant applies
    /// only when the VM boots or reloads. Opaque; a runner reloads a running VM whose digest
    /// changed since it booted. Absent from older manifests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    boot_digest: Option<String>,
}

impl Serialize for Native {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let fields = match self {
            Native::Compose { service } => NativeFields {
                service: Some(service.clone()),
                vagrant: None,
                vm_names: None,
                boot_digest: None,
            },
            Native::Vagrant {
                vagrant,
                vm_names,
                boot_digest,
            } => NativeFields {
                service: None,
                vagrant: Some(vagrant.clone()),
                vm_names: Some(vm_names.clone()),
                boot_digest: boot_digest.clone(),
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
                boot_digest: None,
            } => Ok(Native::Compose { service }),
            NativeFields {
                service: None,
                vagrant: Some(vagrant),
                vm_names,
                boot_digest,
            } => Ok(Native::Vagrant {
                vagrant,
                vm_names: vm_names.unwrap_or_default(),
                boot_digest,
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

/// A named volume of a machine. `down` removes it; `park` and `resume` keep it (a file written
/// once by an init job lives in one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Volume {
    /// Its name in the spec.
    pub name: Name,
    /// Where the machine mounts it.
    pub path: GuestPath,
    /// Its name where the target creates it (`${run.name}_web-data`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<Template>,
}

/// A machine the target creates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Machine {
    /// Its name: the spec's, or Isoloom's for its own (`isoloom-init-web-1`, `isoloom-check`).
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
    /// Its CPU architecture, for the environment's own machines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<Arch>,
    /// Its size: an environment's machine's as it runs on this target, its `resources` each field they leave
    /// out the default of its form (a container: 1 CPU, 512 MB; a VM: 1 CPU, 1024 MB); one of
    /// Isoloom's own, its own size when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<Resources>,
    /// Its static address on each of the environment's networks, by network name (what the status shows).
    pub addresses: BTreeMap<Name, IpAddr>,
    /// Infra that stays on once the environment is built (a controller with `keep_running`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_running: bool,
    /// Brought up on demand, never by the initial `up`: the access machine, which the runner
    /// hot-adds onto a chosen network of the environment when asked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub on_demand: bool,
    /// Its name where the target creates it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<Native>,
    /// Its named volumes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<Volume>,
    /// The ports it answers on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<Port>,
    /// Its graphical consoles, or why it has none, each provider at most once (a VM's; a
    /// container has none to list). Absent from older manifests: nothing said.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consoles: Vec<Console>,
}

impl Machine {
    /// Its console under `provider` (`None`: a run with no provider), if it lists one.
    #[must_use]
    pub fn console(&self, provider: Option<Provider>) -> Option<&Console> {
        self.consoles.iter().find(|c| c.applies_to(provider))
    }
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
        assert_eq!(Cidr::parse("0.0.0.0/0").unwrap().to_string(), "0.0.0.0/0");
        for bad in [
            "10.30.0.0",
            "10.30.0.0/33",
            "fd00::/129",
            "lab/24",
            "10.30.0.0/x",
            "10.30.0.0/999",
            "10.30.0.1/24",
            "fd00::1/64",
        ] {
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

    #[test]
    fn a_vagrant_machine_may_carry_its_boot_digest_and_an_older_one_has_none() {
        let older: Native = serde_json::from_str(r#"{"vagrant": "default", "vm_names": {}}"#).unwrap();
        assert!(matches!(older, Native::Vagrant { boot_digest: None, .. }));
        let with: Native = serde_json::from_str(r#"{"vagrant": "default", "boot_digest": "sha256:ab"}"#).unwrap();
        assert!(matches!(with, Native::Vagrant { boot_digest: Some(ref d), .. } if d == "sha256:ab"));
        assert_eq!(
            serde_json::to_string(&with).unwrap(),
            r#"{"vagrant":"default","vm_names":{},"boot_digest":"sha256:ab"}"#
        );
        assert_eq!(serde_json::to_string(&older).unwrap(), r#"{"vagrant":"default","vm_names":{}}"#);
        let service = serde_json::from_str::<Native>(r#"{"service": "db", "boot_digest": "sha256:ab"}"#);
        assert!(service.is_err());
    }
}
