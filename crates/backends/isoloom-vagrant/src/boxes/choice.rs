//! The box one VM boots under each provider, on a computer of each architecture: its main box
//! (`m.vm.box`), then a provider's own where that provider boots another (libvirt, QEMU, UTM,
//! Hyper-V, ESXi). Its Vagrantfile is written from it, and its run manifest lists it, so a runner
//! can download each box before `vagrant up` and the two never disagree.
//!
//! A box's build is the computer's architecture unless the VM pins one (`box_architecture`):
//! Vagrant 2.4 picks, among a box's versions, the newest with a build for the provider and that
//! architecture. On ESXi it is the ESXi host's, which the runner tells Vagrant.

use isoloom_spec::model::Arch;

use super::HyperVBox;
use crate::ruby::{Ruby, Value};

/// Whether the computer Vagrant runs on is an ARM one, in Ruby.
pub(crate) const ARM_HOST: &str = "RbConfig::CONFIG[\"host_cpu\"] =~ /arm|aarch64/";

/// A VM's main box: the one every provider boots unless it has its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MainBox<'a> {
    /// Its name.
    pub(crate) name: &'a str,
    /// Its version, when pinned (`None`: the newest with the build).
    pub(crate) version: Option<&'a str>,
    /// The build it is pinned to (`None`: the computer's).
    pub(crate) arch: Option<Arch>,
    /// The box an arm64 computer boots instead, the main one having no arm64 build for the
    /// desktop hypervisors (the controller's Alpine).
    pub(crate) on_arm: Option<&'a str>,
}

impl<'a> MainBox<'a> {
    /// Box `name`, at its newest version, the computer's build.
    pub(crate) fn plain(name: &'a str) -> MainBox<'a> {
        MainBox {
            name,
            version: None,
            arch: None,
            on_arm: None,
        }
    }

    /// `<var>.vm.box` (on an arm64 computer, [`MainBox::on_arm`] when set), its version and its
    /// build when pinned. `name` is the box's name as Ruby, when it isn't a plain string (the
    /// access box, which the runner may give).
    pub(crate) fn render(&self, r: &mut Ruby, var: &str, name: Option<Value>) {
        let main = name.unwrap_or_else(|| Value::str(self.name));
        let named = match self.on_arm {
            Some(arm) => Value::raw(format!("{ARM_HOST} ? {} : {main}", Value::str(arm))),
            None => main,
        };
        r.assign(&format!("{var}.vm.box"), named);
        if let Some(v) = self.version {
            r.assign(&format!("{var}.vm.box_version"), Value::str(v));
        }
        if let Some(a) = self.arch {
            r.assign(&format!("{var}.vm.box_architecture"), Value::str(a.id()));
        }
    }
}

/// The boxes one VM boots, provider by provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Choice<'a> {
    /// The main box.
    pub(crate) main: MainBox<'a>,
    /// libvirt's own box, at its newest version, when it boots another.
    pub(crate) libvirt: Option<&'a str>,
    /// QEMU's own box, at its newest version, when it boots another.
    pub(crate) qemu: Option<&'a str>,
    /// The build QEMU boots, and libvirt its own box's: an environment machine's CPU (`None`
    /// for Isoloom's own VMs: the computer's).
    pub(crate) cpu: Option<Arch>,
    /// Hyper-V's box, when the VM has a Hyper-V block.
    pub(crate) hyperv: Option<&'a HyperVBox>,
    /// ESXi's own box, when it boots another (the main one's version pin kept).
    pub(crate) esxi: Option<&'a str>,
    /// UTM's own box, at its newest version, when it boots another.
    pub(crate) utm: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_arm_computer_boots_its_own_main_box_when_there_is_one() {
        let main = MainBox {
            on_arm: Some("bento/debian-12"),
            ..MainBox::plain("generic/alpine319")
        };
        let mut r = Ruby::new();
        main.render(&mut r, "m", None);
        assert_eq!(r.finish(), format!("m.vm.box = {ARM_HOST} ? \"bento/debian-12\" : \"generic/alpine319\"\n"));
        let mut pinned_ruby = Ruby::new();
        let pinned = MainBox {
            version: Some("1.0"),
            arch: Some(Arch::Arm64),
            ..MainBox::plain("bento/debian-12")
        };
        pinned.render(&mut pinned_ruby, "config", None);
        assert_eq!(
            pinned_ruby.finish(),
            "config.vm.box = \"bento/debian-12\"\nconfig.vm.box_version = \"1.0\"\nconfig.vm.box_architecture = \"arm64\"\n"
        );
    }
}
