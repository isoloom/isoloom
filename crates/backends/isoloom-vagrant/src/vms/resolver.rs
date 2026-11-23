//! A Linux VM's resolver, as its distribution sets it up rather than as its box does.
//!
//! The roboxes `generic/*` boxes (the libvirt and Hyper-V boxes of Ubuntu and others) ship a
//! systemd-resolved that validates DNSSEC (`DNSSEC=yes`) against public servers of their choice.
//! Behind a network where one of those servers answers without the DNSSEC keys (one that strips
//! them, or a resolver in between), every name fails (`DNSSEC validation failed: missing-key`):
//! the VM's packages and downloads with it, seen on libvirt with `generic/ubuntu2204`. The
//! distributions themselves don't validate by default (Ubuntu and Debian: `DNSSEC=no`).
//!
//! So each Linux VM's provisioner, once, when its resolved.conf enforces DNSSEC, gives it its
//! distribution's default back in a configuration file of its own, and restarts the resolver. A
//! guest whose resolver doesn't enforce it is left alone.

use crate::ruby::{Provision, Ruby, Value};

/// The provisioner's name: what `vagrant up --provision-with` takes.
pub(crate) const NAME: &str = "resolver";

/// The script: nothing unless the box's resolved.conf sets `DNSSEC=yes`; then a drop-in setting
/// it back to `no`, and the resolver restarted.
pub(crate) const SCRIPT: &str = r"set -eu
grep -Eqs '^[[:space:]]*DNSSEC[[:space:]]*=[[:space:]]*(yes|true|1)[[:space:]]*$' /etc/systemd/resolved.conf || exit 0
mkdir -p /etc/systemd/resolved.conf.d
printf '[Resolve]\nDNSSEC=no\n' >/etc/systemd/resolved.conf.d/90-isoloom-dnssec.conf
systemctl restart systemd-resolved
";

/// `m.vm.provision "resolver"`, for a Linux VM.
pub(crate) fn provisioner(r: &mut Ruby) {
    r.provision(Provision::named("m", NAME, "shell").kw("inline", Value::str(SCRIPT)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_linux_vm_gets_its_resolver_back_once() {
        let mut r = Ruby::new();
        provisioner(&mut r);
        let out = r.finish();
        assert!(out.contains("m.vm.provision \"resolver\", type: \"shell\""), "{out}");
        assert!(!out.contains("run: \"always\""), "a drop-in stays: once is enough\n{out}");
    }

    #[cfg(unix)]
    #[test]
    fn the_script_parses() {
        let parsed = std::process::Command::new("sh").args(["-n", "-c", SCRIPT]).status().unwrap();
        assert!(parsed.success(), "not valid sh");
    }
}
