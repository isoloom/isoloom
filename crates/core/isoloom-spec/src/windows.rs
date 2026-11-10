//! What a Windows guest asks of the VM that runs it: a virtual TPM (Windows 11 expects one),
//! and how Isoloom reaches it (WinRM, or Windows' own OpenSSH). Each target reads these once,
//! so a TPM is given, or refused, the same way everywhere. And Isoloom's folder on it, which
//! every target and the catalogue write to.

use crate::images::is_windows;
use crate::model::VmImpl;

/// Isoloom's folder on a Windows machine ([`ISOLOOM_DIR`]), or a path under it: `$rest` appended
/// at compile time, as [`linux_state_path!`](crate::linux_state_path) does on Linux. Hidden: the
/// crates of the workspace share it.
#[doc(hidden)]
#[macro_export]
macro_rules! windows_isoloom_path {
    () => {
        "C:\\isoloom"
    };
    ($rest:literal) => {
        concat!($crate::windows_isoloom_path!(), $rest)
    };
}

/// Isoloom's folder on a Windows machine: the project unpacked, the machine's own scripts, its
/// marks, the catalogue's values.
pub const ISOLOOM_DIR: &str = windows_isoloom_path!();

/// The mark a Windows machine's set-up leaves in [`ISOLOOM_DIR`] once it is ready, which the
/// controller and the runner wait for.
pub const READY_FILE: &str = windows_isoloom_path!("\\ready");

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
}
