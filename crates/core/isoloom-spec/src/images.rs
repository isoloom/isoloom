//! The image each target uses for an OS name: the built-in table, then the runner's own
//! table ([`Table`]: a file given to `isoloom generate --images`, the defaults' `images:`, or a
//! tool's own settings), then what a spec gives itself (a machine's image under its target's
//! `overrides:`, `overrides.vagrant.machines.<m>.box`).

use indexmap::IndexMap;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

use crate::model::{DockerImpl, Spec, VmImage, VmImpl};
use crate::{Error, Result};

/// The runner's own image table, between the built-in one and the spec's own image for a
/// machine (under `overrides:`):
///
/// ```yaml
/// os:                     # an OS's image, instead of the built-in one
///   debian@12: { vagrant: my-org/debian-12, vagrant_version: "1.2.0" }
/// access:                 # the access machine, when the spec leaves it to the runner
///   docker: registry.example.com/workstation:1
///   vm: ubuntu@24.04
/// ```
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    /// An OS's image per target, instead of the built-in one. Written as a spec writes the OS
    /// (`debian@12`, `debian`), held by the image table's name for it (`debian-12`).
    #[serde(default, deserialize_with = "os_keys")]
    pub os: IndexMap<String, VmImage>,
    /// The access machine, when the spec leaves it to the runner.
    #[serde(default)]
    pub access: Option<Access>,
}

/// How the runner supplies an access machine that has no implementation in the spec.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Access {
    /// A container image (kept running idle) for container targets.
    #[serde(default)]
    pub docker: Option<String>,
    /// An OS for VM targets, written as a spec writes it (`ubuntu@24.04`, `debian@12`), held by the
    /// image table's name for it (its image from this table or the built-in one).
    #[serde(default, deserialize_with = "access_os")]
    pub vm: Option<String>,
}

/// The table's `os:` keys, each resolved to the image table's name for its OS, and each image's
/// boxes checked to be box names.
fn os_keys<'de, D: Deserializer<'de>>(d: D) -> Result<IndexMap<String, VmImage>, D::Error> {
    IndexMap::<String, VmImage>::deserialize(d)?
        .into_iter()
        .map(|(key, image)| {
            if let Some((field, value)) = bad_boxes(&image).next() {
                return Err(D::Error::custom(format!("os.{key}.{field}: `{value}` isn't {BOX_NAME}")));
            }
            match crate::os::resolve(&key) {
                Ok(os) => Ok((os.name.to_owned(), image)),
                Err(e) => Err(D::Error::custom(format!("os.{key}: {e}"))),
            }
        })
        .collect()
}

/// What a Vagrant box's name is, for the refusal of one that isn't.
pub(crate) const BOX_NAME: &str = "a Vagrant box name: letters, digits, `.`, `_` and `-`, after an optional `<org>/` (`bento/debian-12`)";

/// The fields of `image` that name a Vagrant box (`vagrant`, `qemu`, `hyperv`) and hold something
/// else, each with its value: the Vagrantfile reads a box name as Ruby text, so it is only ever
/// [`BOX_NAME`].
pub(crate) fn bad_boxes(image: &VmImage) -> impl Iterator<Item = (&'static str, &str)> {
    [("vagrant", &image.vagrant), ("qemu", &image.qemu), ("hyperv", &image.hyperv)]
        .into_iter()
        .filter_map(|(field, value)| Some((field, value.as_deref()?)))
        .filter(|(_, value)| !is_box_name(value))
}

/// Whether `s` is a box name: `<name>` or `<org>/<name>`, each part letters, digits, `.`, `_`
/// and `-`, not starting with `.` or `-`.
fn is_box_name(s: &str) -> bool {
    let part = |p: &str| !p.is_empty() && !p.starts_with(['.', '-']) && p.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    match s.split_once('/') {
        Some((org, name)) => part(org) && part(name),
        None => part(s),
    }
}

/// The access machine's OS, resolved to the image table's name for it.
fn access_os<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(d)?
        .map(|text| match crate::os::resolve(&text) {
            Ok(os) => Ok(os.name.to_owned()),
            Err(e) => Err(D::Error::custom(e)),
        })
        .transpose()
}

impl Table {
    /// The table's image for an OS by its image-table name (`debian-12`).
    fn image_of(&self, os: &str) -> Option<&VmImage> {
        self.os.get(os)
    }

    /// Reads a table from YAML. Unknown fields are errors.
    ///
    /// # Errors
    ///
    /// [`Error::Images`]: text that isn't YAML, an unknown field, or an OS no family names.
    pub fn parse(yaml: &str) -> Result<Table> {
        serde_yaml_ng::from_str(yaml).map_err(Error::Images)
    }

    /// The spec with this table applied: each VM's image from `os:` (unless the spec gives its
    /// own, under `overrides:`), and the access machine's missing implementations from `access:`. The
    /// generators then treat a supplied machine like any other, on every target.
    #[must_use]
    pub fn apply(&self, spec: &Spec) -> Spec {
        let mut out = spec.clone();
        for m in out.machines.values_mut() {
            if let Some(vm) = &mut m.vm
                && vm.image.is_none()
                && let Some(image) = self.image_of(&vm.os)
            {
                vm.image = Some(image.clone());
            }
            let Some(access) = self.access.as_ref().filter(|_| m.access) else { continue };
            if m.docker.is_none()
                && let Some(image) = &access.docker
            {
                m.docker = Some(DockerImpl {
                    image: Some(image.clone()),
                    build: None,
                    dockerfile: None,
                    target: None,
                    args: IndexMap::new(),
                    init: Vec::new(),
                    idle: false,
                    appliance: None,
                    config: None,
                    firmware: None,
                    lxc: false,
                    generated: None,
                });
                m.supplied = true;
            }
            if m.vm.is_none()
                && let Some(os) = &access.vm
            {
                m.vm = Some(VmImpl {
                    image: self.image_of(os).cloned(),
                    os: os.clone(),
                    provision: Vec::new(),
                    tpm: None,
                    boot: None,
                    setup: Vec::new(),
                });
                m.supplied = true;
            }
        }
        out
    }
}

/// Whether an OS name is a Windows one (WinRM and PowerShell instead of SSH and sh).
#[must_use]
pub fn is_windows(os: &str) -> bool {
    os.starts_with("windows")
}

/// How a machine's Windows box answers WinRM: what its image declares, else plain HTTP (every
/// built-in Windows box uses plain HTTP today).
#[must_use]
pub fn winrm(vm: &VmImpl) -> crate::model::Winrm {
    vm.image.as_ref().and_then(|i| i.winrm).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table with Debian 12's box and an access machine of each form.
    const TABLE: &str = "os:\n  debian@12: { vagrant: my-org/debian-12 }\naccess: { docker: registry.example.com/workstation:1, vm: debian }\n";

    #[test]
    fn a_table_refuses_a_box_that_isnt_a_box_name() {
        for bad in ["corp/deb#{`id`}", "a b", "\"x\"", "/abs", "a/b/c", ".hidden", "org/"] {
            let yaml = format!("os:\n  debian@12: {{ vagrant: {} }}\n", serde_json::Value::String(bad.to_owned()));
            let err = Table::parse(&yaml).expect_err(bad).to_string();
            assert!(
                err.contains("os.debian@12.vagrant:") && err.contains("isn't a Vagrant box name"),
                "{bad}: {err}"
            );
        }
        let qemu = Table::parse("os:\n  debian@12: { qemu: \"q;rm\" }\n").unwrap_err().to_string();
        assert!(qemu.contains("os.debian@12.qemu:"), "{qemu}");
        for good in ["bento/debian-12", "StefanScherer/windows_2019", "ubuntu/trusty64", "local-box", "my.org/box.v2"] {
            assert!(Table::parse(&format!("os:\n  debian@12: {{ vagrant: {good} }}\n")).is_ok(), "{good}");
        }
    }

    #[test]
    fn a_table_holds_each_os_by_its_image_table_name() {
        let table = Table::parse(TABLE).unwrap();
        assert_eq!(table.os.keys().collect::<Vec<_>>(), ["debian-12"]);
        assert_eq!(table.os["debian-12"].vagrant.as_deref(), Some("my-org/debian-12"));
        let access = table.access.unwrap();
        assert_eq!(
            (access.docker.as_deref(), access.vm.as_deref()),
            (Some("registry.example.com/workstation:1"), Some("debian-12"))
        );
        assert!(
            Table::parse("access: { vm: windows-95 }")
                .unwrap_err()
                .to_string()
                .contains("unknown OS `windows-95`")
        );
    }

    #[test]
    fn a_table_gives_vms_their_image_unless_the_spec_does() {
        let table = Table::parse(TABLE).unwrap();
        let spec = crate::testing::model(&crate::testing::lab(
            "machines:\n  a: { networks: { lan: 10 }, vm: { os: debian@12 } }\n  b: { networks: { lan: 11 }, vm: { os: debian@12, image: { vagrant: own/box } } }\n  c: { networks: { lan: 12 }, vm: { os: debian@13 } }\n",
        ))
        .unwrap();
        let applied = table.apply(&spec);
        let vagrant = |m: &str| applied.machines[m].vm.as_ref().and_then(|v| v.image.as_ref()).and_then(|i| i.vagrant.clone());
        assert_eq!(vagrant("a").as_deref(), Some("my-org/debian-12"));
        assert_eq!(vagrant("b").as_deref(), Some("own/box"));
        assert_eq!(applied.machines["c"].vm.as_ref().and_then(|v| v.image.clone()), None);
    }

    #[test]
    fn a_table_supplies_the_access_machine_in_both_forms() {
        let table = Table::parse(TABLE).unwrap();
        let spec = crate::testing::model(&crate::testing::lab("machines:\n  user: { networks: { lan: 10 }, access: true }\n")).unwrap();
        let user = &table.apply(&spec).machines["user"];
        assert!(user.supplied);
        assert_eq!(
            user.docker.as_ref().and_then(|d| d.image.as_deref()),
            Some("registry.example.com/workstation:1")
        );
        let vm = user.vm.as_ref().unwrap();
        assert_eq!(
            (vm.os.as_str(), vm.image.as_ref().and_then(|i| i.vagrant.as_deref())),
            ("debian-12", Some("my-org/debian-12"))
        );
        let own = crate::testing::model(&crate::testing::lab(
            "machines:\n  user: { networks: { lan: 10 }, access: true, docker: { image: mine } }\n",
        ))
        .unwrap();
        let kept = &table.apply(&own).machines["user"];
        assert_eq!(kept.docker.as_ref().and_then(|d| d.image.as_deref()), Some("mine"));
    }

    #[test]
    fn a_windows_box_answers_winrm_as_its_image_says() {
        let declared: VmImpl = serde_yaml_ng::from_str("os: windows-server-2022\nimage: { winrm: ssl }\n").unwrap();
        assert_eq!(winrm(&declared), crate::model::Winrm::Ssl);
        let plain: VmImpl = serde_yaml_ng::from_str("os: windows-server-2022\n").unwrap();
        assert_eq!(winrm(&plain), crate::model::Winrm::Plaintext);
    }
}
