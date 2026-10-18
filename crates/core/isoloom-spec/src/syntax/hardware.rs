//! `hardware:`: what a machine asks of the hardware under it. Each target either gives it or
//! refuses the machine with the reason.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::model::{Arch, Boot, Device, DeviceInput, Gpus, GpusInput};

/// A virtual TPM 2.0 (BitLocker, Windows Hello, measured boot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Tpm {
    /// Windows 11 gets one wherever the target can give one, and runs without where it can't;
    /// every other OS gets none.
    #[default]
    Auto,
    /// The machine needs one: a target that gives none refuses it. A VM only.
    Required,
    /// None, even on Windows 11.
    None,
}

/// The hardware a machine asks for.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hardware {
    /// The CPU architecture: `amd64` (the default) or `arm64`.
    #[serde(default)]
    pub arch: Arch,
    /// A virtual TPM 2.0: `auto` (the default: Windows 11 gets one where the target can),
    /// `required` (a VM only) or `none`.
    #[serde(default)]
    pub tpm: Tpm,
    /// GPUs: a count (`1`), `all`, or `{ count, ids, driver, capabilities, options }`. A container
    /// gets the host's; a VM gets them passed through or sized with them.
    #[serde(default)]
    #[schemars(with = "Option<GpusInput>")]
    pub gpus: Option<Gpus>,
    /// The host's devices (`/dev/fuse`, `host:guest[:permissions]`, or `{ host, guest,
    /// permissions }`). A container only.
    #[serde(default)]
    #[schemars(with = "Vec<DeviceInput>")]
    pub devices: Vec<Device>,
    /// The machine runs VMs of its own: its VM gets the CPU's virtualization extensions. A VM
    /// only.
    #[serde(default)]
    pub nested_virtualization: bool,
    /// How it boots, when not the way its image does: its own kernel, firmware, boot order or
    /// network boot. A VM only.
    #[serde(default)]
    pub boot: Option<Boot>,
}

impl Hardware {
    /// What only a VM can give, as the field naming it (`hardware.tpm`), when the machine asks
    /// for any.
    #[must_use]
    pub fn vm_only(&self) -> Option<&'static str> {
        if self.tpm == Tpm::Required {
            Some("tpm")
        } else if self.nested_virtualization {
            Some("nested_virtualization")
        } else if self.boot.is_some() {
            Some("boot")
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_reads_its_enums() {
        let h: Hardware = serde_yaml_ng::from_str("{ arch: arm64, tpm: required }").unwrap();
        assert_eq!((h.arch, h.tpm, h.vm_only()), (Arch::Arm64, Tpm::Required, Some("tpm")));
        assert_eq!(Hardware::default().vm_only(), None);
        assert!(serde_yaml_ng::from_str::<Hardware>("{ tpm: true }").is_err());
    }
}
