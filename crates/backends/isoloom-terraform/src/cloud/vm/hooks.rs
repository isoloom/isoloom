//! What a cloud adds to a Linux machine's set-up ([`Hooks`]): the published ports redirected
//! before the machine closes itself off, and the steps the cloud's network does itself left out;
//! and the machine's user data, the set-up it runs itself (`crate::machine::user_data`).

use isoloom_plan::{Environment, MachinePlan, Step};
use isoloom_render::hcl_builder::{Expression, Template};

use crate::machine::linux::{self, Hooks};
use crate::machine::user_data;

/// A Linux machine's set-up hooks on a cloud: `at_boot` first (its interfaces after the first,
/// a gateway's forwarding), the published-port redirects before the machine closes itself off.
/// Its expiry needs no hook: the set-up schedules it at every boot.
pub(crate) fn of(mp: &MachinePlan, at_boot: Vec<Template>) -> Hooks {
    Hooks {
        at_boot,
        after_reboot: Vec::new(),
        before_egress: redirects(mp),
        skip: network_does,
    }
}

/// A Linux machine's user data on a cloud: its whole set-up, logging in as `user`, `at_boot`
/// run first at every boot.
pub(crate) fn user_data(env: &Environment, machine: (&str, &MachinePlan), user: &str, at_boot: Vec<Template>) -> Expression {
    let hooks = of(machine.1, at_boot);
    user_data::expression(&linux::set_up(env, machine, user, &hooks))
}

/// The steps a cloud's network does itself: it routes between its subnets, each VLAN its own.
pub(crate) fn network_does(step: &Step) -> bool {
    matches!(step, Step::Routes { .. } | Step::Trunks)
}

/// A published port that differs from the service's own, redirected to it inside the machine
/// (a `before_egress` hook).
fn redirects(mp: &MachinePlan) -> Vec<Template> {
    let moved: Vec<_> = mp.ports.iter().filter(|p| p.publish != p.port).collect();
    if moved.is_empty() {
        return Vec::new();
    }
    let mut out = vec![Template::text(
        "command -v nft >/dev/null || (sudo apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq nftables)",
    )];
    out.extend(moved.into_iter().map(|p| {
        Template::text(format!(
            "sudo nft add table ip isoloom-publish && sudo nft 'add chain ip isoloom-publish prerouting {{ type nat hook prerouting priority -100; }}' && sudo nft add rule ip isoloom-publish prerouting {} dport {} redirect to :{}",
            p.protocol.id(),
            p.publish,
            p.port
        ))
    }));
    out
}
