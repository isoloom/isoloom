//! What a Windows guest asks of the VM that runs it: a virtual TPM (Windows 11 expects one),
//! and how Isoloom reaches it (WinRM, or Windows' own OpenSSH). Each target reads these once,
//! so a TPM is given, or refused, the same way everywhere. And Isoloom's folder on it, which
//! every target and the catalogue write to.

use crate::images::is_windows;
use crate::model::VmImpl;

/// Isoloom's folder on a Windows machine: the project unpacked, the machine's own scripts, its
/// marks, the catalogue's values.
pub const ISOLOOM_DIR: &str = "C:\\isoloom";

/// The mark a Windows machine's set-up leaves in [`ISOLOOM_DIR`] once it is ready, which the
/// controller and the runner wait for.
pub const READY_FILE: &str = "C:\\isoloom\\ready";

/// Why a VM gets a virtual TPM 2.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tpm {
    /// Its OS expects one (Windows 11): given where the target can, left out where it can't,
    /// since the images Isoloom boots for it install without one.
    Wanted,
    /// The spec asks for one (`vm.tpm: true`): a target that gives none refuses the environment.
    Required,
}

/// Whether an OS expects a TPM 2.0: Windows 11 refuses to install or upgrade without one.
#[must_use]
fn expects_tpm(os: &str) -> bool {
    os == "windows-11"
}

/// The TPM a VM gets: what `vm.tpm` says, else what its OS expects.
#[must_use]
pub fn tpm(vm: &VmImpl) -> Option<Tpm> {
    match vm.tpm {
        Some(true) => Some(Tpm::Required),
        Some(false) => None,
        None => expects_tpm(&vm.os).then_some(Tpm::Wanted),
    }
}

/// Whether a Windows VM is reached over OpenSSH (`vm.image.winssh`) rather than WinRM.
#[must_use]
pub fn winssh(vm: &VmImpl) -> bool {
    is_windows(&vm.os) && vm.image.as_ref().is_some_and(|i| i.winssh)
}

/// The guest OS type ESXi gives a VM of Windows `os` (VMware's names: Windows Server 2022 is the
/// one after 2019, 2025 the one after 2022), so it offers the right virtual hardware and tools;
/// `None` for any other OS, which keeps the type its box or OVA was built with. The one table
/// Vagrant's ESXi provider and the `esxi` target read.
#[must_use]
pub fn esxi_guest_os(os: &str) -> Option<&'static str> {
    Some(match os {
        "windows-10" => "windows9-64",
        "windows-11" => "windows11-64",
        "windows-server-2008r2" => "windows7srv-64",
        "windows-server-2012r2" => "windows8srv-64",
        "windows-server-2016" => "windows9srv-64",
        "windows-server-2019" => "windows2019srv-64",
        "windows-server-2022" => "windows2019srvnext-64",
        "windows-server-2025" => "windows2022srvnext-64",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm(os: &str, tpm_field: Option<bool>) -> VmImpl {
        VmImpl {
            os: os.into(),
            provision: Vec::new(),
            image: None,
            tpm: tpm_field,
            boot: None,
            setup: Vec::new(),
        }
    }

    #[test]
    fn windows_11_wants_a_tpm_and_the_spec_decides_otherwise() {
        assert_eq!(tpm(&vm("windows-11", None)), Some(Tpm::Wanted));
        assert_eq!(tpm(&vm("windows-server-2022", None)), None);
        assert_eq!(tpm(&vm("debian-12", Some(true))), Some(Tpm::Required));
        assert_eq!(tpm(&vm("windows-11", Some(false))), None);
    }

    #[test]
    fn winssh_is_only_for_a_windows_box() {
        let mut v = vm("windows-server-2022", None);
        assert!(!winssh(&v));
        v.image = Some(crate::model::VmImage {
            winssh: true,
            ..Default::default()
        });
        assert!(winssh(&v));
        v.os = "debian-12".into();
        assert!(!winssh(&v));
    }

    #[test]
    fn every_windows_os_has_an_esxi_guest_os_and_any_other_keeps_its_own() {
        for os in crate::model::KNOWN_OS.iter().filter(|o| o.starts_with("windows")) {
            assert!(super::esxi_guest_os(os).is_some(), "{os}");
        }
        assert_eq!(super::esxi_guest_os("windows-server-2019"), Some("windows2019srv-64"));
        assert_eq!(super::esxi_guest_os("windows-server-2022"), Some("windows2019srvnext-64"));
        assert_eq!(super::esxi_guest_os("debian-12"), None);
    }
}
