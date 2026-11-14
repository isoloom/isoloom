//! Each cloud VM's graphical console. A cloud gives none of its own that Isoloom reaches (a
//! serial console at most), so a Windows VM's is its own Remote Desktop, tunnelled through a
//! Linux machine of the lab on a network the Windows one is on, at that machine's public address
//! ([`crate::consoles`]). A Linux VM has none: its way in is SSH.

use isoloom_plan::Environment;

use super::outputs::Reach;
use crate::consoles::{self, Tunnel};
use crate::hcl_builder::Module;
use crate::machine::controller;

/// The console outputs: each Windows VM opening Remote Desktop through the first Linux VM beside
/// it, in the lab's order, then the controller.
pub(crate) fn outputs(env: &Environment, reach: &Reach, module: &mut Module) {
    let linux: Vec<&str> = env.vms().filter(|vm| !vm.plan.is_windows()).map(|vm| vm.name).collect();
    let with_controller = reach.controller.as_ref().filter(|_| controller::needed(env));
    consoles::outputs(env, module, &|vm| {
        let (guest, host) = consoles::jump(env, vm, &linux, with_controller.map(|_| controller::NAME))?;
        let (via, os) = match (guest == controller::NAME, with_controller) {
            (true, Some(address)) => (address.clone(), controller::OS),
            _ => ((reach.public_ip)(guest), env.vm(guest)?.os),
        };
        Some(Tunnel {
            via,
            user: (reach.user)(os).to_owned(),
            host,
        })
    });
}
