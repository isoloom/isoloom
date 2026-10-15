//! What a cloud adds to a Linux machine's set-up ([`Hooks`]): the auto-stop scheduled again
//! after a restart, the published ports redirected before the machine closes itself off, and
//! the steps the cloud's network does itself left out.

use isoloom_plan::{MachinePlan, Step};
use isoloom_render::hcl_builder::Template;

use crate::cloud::auto_stop;
use crate::machine::linux::Hooks;

/// A Linux machine's set-up hooks on a cloud: `at_boot` first, the auto-stop again after a
/// restart, the published-port redirects before the machine closes itself off.
pub(crate) fn of(mp: &MachinePlan, at_boot: Vec<Template>) -> Hooks {
    Hooks {
        at_boot,
        after_reboot: vec![auto_stop::rescheduled()],
        before_egress: redirects(mp),
        skip: network_does,
    }
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
