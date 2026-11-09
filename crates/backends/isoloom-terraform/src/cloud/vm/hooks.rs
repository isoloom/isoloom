//! What a cloud adds to a Linux machine's set-up ([`Hooks`]): the published ports redirected
//! before the machine closes itself off, and the steps the cloud's network does itself left out;
//! and the machine's user data, the set-up it runs itself (`crate::machine::user_data`).

use crate::hcl_builder::{Expression, Template};
use isoloom_plan::{Environment, MachinePlan, Step};
use isoloom_render::boot::{self, Trigger};
use isoloom_render::steps;
use isoloom_spec::shell::quote;

use crate::Result;
use crate::machine::linux::{self, Hooks};
use crate::machine::user_data::{self, Capped};

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

/// [`user_data()`] on a cloud that caps it: refused, naming the machine, when it is over the cap
/// ([`user_data::capped`]).
///
/// # Errors
///
/// [`crate::Error::Refused`] when the machine's user data is over the cap.
pub(crate) fn capped_user_data(env: &Environment, machine: (&str, &MachinePlan), user: &str, at_boot: Vec<Template>, cap: Capped) -> Result<Expression> {
    let hooks = of(machine.1, at_boot);
    user_data::capped(&linux::set_up(env, machine, user, &hooks), &format!("machine `{}`", machine.0), cap)
}

/// The steps a cloud's network does itself: it routes between its subnets, each VLAN its own.
pub(crate) fn network_does(step: &Step) -> bool {
    matches!(step, Step::Routes { .. } | Step::Trunks)
}

/// A published port that differs from the service's own, redirected to it inside the machine
/// (a `before_egress` hook): the rules kept in a file and loaded now and at every boot, as the
/// egress filter is, with `nft` from the machine's package manager. Run as root; running it
/// again replaces the table.
fn redirects(mp: &MachinePlan) -> Vec<Template> {
    let rules: Vec<String> = mp
        .ports
        .iter()
        .filter(|p| p.publish != p.port)
        .map(|p| quote(&format!("    {} dport {} redirect to :{}", p.protocol.id(), p.publish, p.port)))
        .collect();
    if rules.is_empty() {
        return Vec::new();
    }
    let load = boot::at_boot(
        "publish",
        "Published ports redirected to their services",
        &format!("nft -f {PUBLISH_NFT}\n"),
        Trigger::Boot,
        &[],
    );
    vec![Template::text(format!(
        "{}printf '%s\\n' 'table ip isoloom-publish' 'delete table ip isoloom-publish' 'table ip isoloom-publish {{' '  chain prerouting {{' '    type nat hook prerouting priority -100;' {} '  }}' '}}' > {PUBLISH_NFT}\n{load}",
        steps::nftables(),
        rules.join(" ")
    ))]
}

/// The published-port redirects' ruleset.
const PUBLISH_NFT: &str = "/etc/isoloom-publish.nft";

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use crate::hcl_builder::Piece;

    use super::redirects;
    use crate::testing::environment;

    #[test]
    fn a_moved_port_is_redirected_as_root_now_and_at_every_boot() {
        let env = environment(
            "version: 1\nname: r\nnetworks: { lab: { cidr: 10.0.0.0/24 } }\nmachines:\n  web:\n    os: debian@12\n    networks: { lab: 10 }\n    ports: { web: { port: 80, protocol: http, publish: 8080 } }\n",
            Target::CloudVm,
        );
        let hooks = redirects(&env.machines["web"]);
        assert_eq!(hooks.len(), 1);
        let sh: String = hooks[0]
            .pieces()
            .map(|p| match p {
                Piece::Text(t) => t.to_owned(),
                Piece::Expr(_) => panic!("plain text"),
            })
            .collect();
        assert!(sh.contains("'    tcp dport 8080 redirect to :80'"), "{sh}");
        assert!(sh.contains("> /etc/isoloom-publish.nft\n"), "{sh}");
        assert!(
            sh.contains("nft -f /etc/isoloom-publish.nft") && sh.contains("systemctl enable isoloom-publish.service"),
            "{sh}"
        );
        assert!(!sh.contains("sudo"), "the set-up runs as root: {sh}");
        assert!(sh.contains("pkg nftables"), "the machine's own package manager: {sh}");
    }
}
