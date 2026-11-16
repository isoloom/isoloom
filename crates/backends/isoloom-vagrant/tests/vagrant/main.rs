//! The Vagrant backend's integration tests: the examples' Vagrantfiles, its refusals, and each
//! provider's part of a Vagrantfile, read or run as Vagrant's own Ruby would.

#![expect(
    clippy::disallowed_methods,
    reason = "the tests run the binary and the tools that check its output: test code, not an adapter"
)]

mod examples;
mod hyperv;
mod libvirt;
mod names;
mod provisioning;
mod qemu;
mod refusals;
mod testing;
mod utm;
mod vmware;
