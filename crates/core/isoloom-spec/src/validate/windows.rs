//! How a Windows box is reached: OpenSSH (`vm.image.winssh`) is for Windows boxes only, and a
//! box answers either WinRM or OpenSSH, not both.

use super::Problems;
use crate::images::is_windows;
use crate::model::Spec;

/// The machines' and the controller's `winssh`.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    for (name, m) in &spec.machines {
        let Some(vm) = &m.vm else { continue };
        let Some(image) = vm.image.as_ref().filter(|i| i.winssh) else { continue };
        let at = format!("machines.{name}.vm.image.winssh");
        if !vm.os.is_empty() && !is_windows(&vm.os) {
            p.add(
                &at,
                format!("`{}` is reached over SSH already: `winssh` says how a Windows box is reached", vm.os),
            );
        } else if image.winrm.is_some() {
            p.add(&at, "a box answers WinRM or OpenSSH: drop `winrm` or `winssh`");
        }
    }
    if spec.controller.as_ref().and_then(|c| c.image.as_ref()).is_some_and(|i| i.winssh) {
        p.add("controller.image.winssh", "the controller is a Linux VM: no OpenSSH for Windows");
    }
}

#[cfg(test)]
mod tests {
    use crate::validate::validate;

    fn problems(machine: &str) -> Vec<(String, String)> {
        let yaml = format!("version: 1\nname: w\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  a: {{ networks: {{ lab: 10 }}, vm: {machine} }}\n");
        validate(&crate::model::from_yaml(&yaml).unwrap())
            .into_iter()
            .map(|p| (p.at, p.message))
            .collect()
    }

    #[test]
    fn winssh_is_for_a_windows_box_that_answers_no_winrm() {
        assert_eq!(problems("{ os: windows-server@2022, provision: [a.ps1], image: { winssh: true } }"), vec![]);
        let linux = problems("{ os: debian@12, provision: [a.sh], image: { winssh: true } }");
        assert_eq!(linux.len(), 1, "{linux:?}");
        assert_eq!(linux[0].0, "machines.a.vm.image.winssh");
        let both = problems("{ os: windows-server@2022, provision: [a.ps1], image: { winssh: true, winrm: ssl } }");
        assert_eq!(both[0].1, "a box answers WinRM or OpenSSH: drop `winrm` or `winssh`");
    }

    #[test]
    fn any_vm_may_ask_for_a_tpm_or_none() {
        assert_eq!(problems("{ os: debian@12, provision: [a.sh], tpm: true }"), vec![]);
        assert_eq!(problems("{ os: windows@11, provision: [a.ps1], tpm: false }"), vec![]);
    }
}
