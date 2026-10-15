//! A VM's own image and boot, as the plan carries them: which machines boot their own way (a
//! requirement only some backends meet), and the network boots each network serves.

use std::fmt;

use indexmap::IndexMap;
use isoloom_spec::model::NetworkBoot;

use crate::environment::{MachinePlan, MachineShape};
use crate::requirements::Requirement;

/// What the machines' own boots ask of the backend: the first VM that sets `vm.boot`. A
/// container or an existing host boots its own way whatever the spec says, so only VMs count.
pub fn requirements(machines: &IndexMap<String, MachinePlan>) -> Option<Requirement> {
    machines
        .iter()
        .find(|(_, mp)| mp.shape == MachineShape::Vm && mp.boot.is_some())
        .map(|(name, _)| Requirement::OwnBoot { machine: name.clone() })
}

/// Why a target that boots its images' own way refuses machine `machine`'s own boot.
pub(crate) fn no_own_boot(f: &mut fmt::Formatter<'_>, machine: &str) -> fmt::Result {
    write!(
        f,
        "machine `{machine}` boots its own way (`hardware.boot`): this target boots a VM from its image's own kernel and firmware; Vagrant's libvirt provider boots it"
    )
}

/// The network boot each network serves: a VM's `boot.network` on its first network (the
/// validator keeps the machines of one network in agreement).
pub fn network_boots(machines: &IndexMap<String, MachinePlan>) -> IndexMap<String, NetworkBoot> {
    let mut out = IndexMap::new();
    for mp in machines.values().filter(|mp| mp.shape == MachineShape::Vm) {
        let Some(n) = mp.boot.as_ref().and_then(|b| b.network.as_ref()) else {
            continue;
        };
        if let Some(net) = mp.addresses.keys().next() {
            out.entry(net.clone()).or_insert_with(|| n.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;
    use crate::{Environment, Lab};

    const SPEC: &str = "
version: 1
name: t
networks: { lan: { cidr: 10.0.0.0/24 } }
machines:
  web: { networks: { lan: 10 }, os: debian@12, hardware: { boot: { order: [network, hd], network: { file: pxelinux.0 } } }, app: { image: nginx, steps: [{ run: s.sh }] } }
";

    #[test]
    fn only_a_vm_requires_its_own_boot() {
        let lab = Lab::new(isoloom_spec::parse(SPEC).unwrap());
        let vm = Environment::build(&lab.for_target(Target::Vagrant), Target::Vagrant, None);
        assert!(vm.requirements.contains(&Requirement::OwnBoot { machine: "web".into() }));
        assert_eq!(network_boots(&vm.machines)["lan"].file.as_deref(), Some("pxelinux.0"));
        // A boot is VM-only hardware: the machine has no container form, so a container target
        // refuses it at the field.
        let refused = lab.refusal(Target::Docker).expect("a container target refuses it");
        assert_eq!(refused.path.as_str(), "machines.web.hardware.boot");
        assert!(lab.for_target(Target::Docker).machines["web"].docker.is_none());
    }
}
