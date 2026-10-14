//! A machine's `os`: a known family and version, and the same OS as its `vm.os` when both are
//! set.

use super::Problems;
use super::machines::Subject;
use crate::os::resolve;

/// The machine's `os` resolves, and agrees with its `vm.os`.
pub(super) fn of_machine(s: &Subject, p: &mut Problems) {
    let Some(text) = &s.m.os else { return };
    let os = match resolve(text) {
        Ok(os) => os,
        Err(e) => return p.add(format!("{}.os", s.at), e.to_string()),
    };
    let Some(vm) = s.m.vm.as_ref().and_then(|v| resolve(&v.os).ok()) else {
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
