//! A machine's `os` and its `vm.os` name the same OS when both are set. Lowering resolved both
//! (an unknown OS is refused when the spec is parsed, see [`crate::os::resolve`]).

use super::Problems;
use super::machines::Subject;
use crate::os::{by_name, resolve};

/// The machine's `os` agrees with its `vm.os`.
pub(super) fn of_machine(s: &Subject, p: &mut Problems) {
    let Some(os) = s.m.os.as_deref().and_then(|t| resolve(t).ok()) else {
        return;
    };
    let Some(vm) = s.m.vm.as_ref().and_then(|v| by_name(&v.os)) else {
        return;
    };
    if vm.name != os.name {
        p.add(
            format!("{}.vm.os", s.at),
            format!(
                "`vm.os` is {} but the machine's `os` is {}: they must agree; leave `vm.os` out (it defaults to `os`) or write the same OS in both",
                vm.id(),
                os.id()
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{lab, problems};

    /// The problems of a VM whose `os` is `os` and `vm.os` is `vm_os`.
    fn of(os: &str, vm_os: &str) -> Vec<String> {
        problems(&lab(&format!(
            "machines:\n  a: {{ networks: {{ lan: 10 }}, os: {os}, vm: {{ os: {vm_os} }} }}\n"
        )))
    }

    #[test]
    fn a_machines_os_and_its_vm_os_agree() {
        assert_eq!(of("debian@12", "debian@12"), Vec::<String>::new());
        assert_eq!(
            of("debian@12", "ubuntu@24.04"),
            [
                "machines.a.vm.os: `vm.os` is ubuntu@24.04 but the machine's `os` is debian@12: they must agree; leave `vm.os` out (it defaults to `os`) or write the same OS in both"
            ]
        );
    }
}
