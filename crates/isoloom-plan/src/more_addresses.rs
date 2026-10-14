//! A machine's several addresses on one network (`networks: { lan: [10, 11, 12] }`): the
//! addresses after its primary one, where a VM's set-up puts them (on the interface that holds
//! the primary one), and why a target that can't give them refuses.
//!
//! A VM gets them from its own set-up on every VM target: the hypervisor or the cloud gives an
//! interface its primary address, and the guest adds the others. A cloud that filters traffic
//! by address also learns them (AWS's secondary private addresses, Azure's further IP
//! configurations, Google Cloud's alias ranges, Oracle Cloud's secondary private IPs, Linode's
//! routed ranges). A container has one address per network, and an existing host keeps its own.

use std::fmt;
use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_spec::model::{Machine, Spec};

use crate::addressing::{address, cidr};
use crate::environment::MachineShape;
use crate::lifecycle::Step;
use crate::requirements::Requirement;

/// The further addresses of a machine on one network, as its set-up adds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoreAddresses {
    /// Its primary address there: it finds the interface.
    pub primary: Ipv4Addr,
    /// The network's prefix length.
    pub prefix: u8,
    /// The addresses after the primary one, in spec order.
    pub more: Vec<Ipv4Addr>,
}

/// Machine `m`'s addresses after its primary one, by network (in its networks' order).
pub fn of(spec: &Spec, m: &Machine) -> IndexMap<String, Vec<Ipv4Addr>> {
    m.networks
        .keys()
        .filter_map(|n| {
            let octets = m.more_addresses.get(n).filter(|o| !o.is_empty())?;
            Some((n.clone(), octets.iter().map(|o| address(spec, n, *o)).collect()))
        })
        .collect()
}

/// The set-up step that adds them, when the machine has any.
pub fn step(spec: &Spec, m: &Machine) -> Option<Step> {
    let addresses: Vec<MoreAddresses> = of(spec, m)
        .into_iter()
        .map(|(n, more)| MoreAddresses {
            primary: address(spec, &n, m.networks[&n]),
            prefix: cidr(spec, &n).len,
            more,
        })
        .collect();
    (!addresses.is_empty()).then_some(Step::MoreAddresses { addresses })
}

/// What they ask of the backend: the first machine with several addresses on a network, as the
/// target produces it.
pub fn requirements(spec: &Spec, shape: impl Fn(&str) -> Option<MachineShape>) -> Option<Requirement> {
    let (name, m) = spec.machines.iter().find(|(_, m)| m.more_addresses.values().any(|o| !o.is_empty()))?;
    let network = m.more_addresses.iter().find(|(_, o)| !o.is_empty()).map(|(n, _)| n.clone())?;
    Some(Requirement::MoreAddresses {
        machine: name.clone(),
        network,
        shape: shape(name)?,
    })
}

/// Why a target refuses a machine's several addresses on `network`.
pub(crate) fn refused(f: &mut fmt::Formatter<'_>, machine: &str, network: &str, shape: MachineShape) -> fmt::Result {
    let at = format!("machine `{machine}` has several addresses on `{network}`");
    match shape {
        MachineShape::Vm => write!(f, "{at}, which this target doesn't give a VM yet"),
        MachineShape::Container => write!(
            f,
            "{at}: a container has one address per network (Docker and Kubernetes give an endpoint one IPv4 address); give the machine a `vm:` and run it on a VM target"
        ),
        MachineShape::Supplied | MachineShape::External => {
            write!(f, "{at}: an existing host keeps the addresses it has, which Isoloom doesn't change")
        }
    }
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;
    use crate::Environment;

    const SPEC: &str = "
version: 1
name: t
networks: { lan: { cidr: 10.0.0.0/24 }, dmz: { cidr: 10.0.1.0/24 } }
machines:
  web: { networks: { lan: [10, 11, 12], dmz: 10 }, docker: { image: nginx }, vm: { os: debian-12, provision: [s.sh] } }
";

    #[test]
    fn a_vm_adds_them_on_the_interface_of_its_primary_address() {
        let spec = isoloom_spec::parse(SPEC).unwrap();
        let env = Environment::build(&spec, Target::Vagrant, None);
        let web = &env.machines["web"];
        assert_eq!(
            web.more_addresses["lan"],
            ["10.0.0.11".parse::<Ipv4Addr>().unwrap(), "10.0.0.12".parse().unwrap()]
        );
        assert!(!web.more_addresses.contains_key("dmz"));
        let step = web.lifecycle.iter().find(|s| matches!(s, Step::MoreAddresses { .. })).unwrap();
        let Step::MoreAddresses { addresses } = step else { unreachable!() };
        assert_eq!(addresses[0].primary, "10.0.0.10".parse::<Ipv4Addr>().unwrap());
        assert_eq!(addresses[0].prefix, 24);
        let need = |t| {
            Environment::build(&spec, t, None)
                .requirements
                .into_iter()
                .find(|r| matches!(r, Requirement::MoreAddresses { .. }))
        };
        let vm = need(Target::Vagrant).unwrap();
        assert!(matches!(vm, Requirement::MoreAddresses { shape: MachineShape::Vm, .. }));
        let container = need(Target::Docker).unwrap();
        assert!(container.to_string().contains("a container has one address per network"), "{container}");
    }
}
