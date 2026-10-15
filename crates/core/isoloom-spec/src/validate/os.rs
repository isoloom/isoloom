//! A machine's `os` and its `vm.os` name the same OS when both are set. Parsing resolved both
//! (an unknown OS is refused there, see [`crate::os::canonical`]).

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
