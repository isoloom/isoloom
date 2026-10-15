//! A VM's own network set-up: the other machines by name, its routes, its trunks, its link
//! impairment and, offline, the egress filter. Windows has the names and the routes.

use std::net::Ipv4Addr;

use isoloom_plan::Environment;
use isoloom_plan::addressing::address;
use isoloom_plan::environment::Route;
use isoloom_spec::shell::quote;
use isoloom_spec::validate::Cidr;

use super::PKG;
use crate::network::{ipv6, router, trunks};
use crate::{boot, powershell};

/// The other machines by name, appended to `/etc/hosts`, each line once (running the step again
/// adds nothing).
pub(super) fn hosts(entries: &[(Ipv4Addr, Vec<String>)]) -> Option<String> {
    let lines: Vec<String> = entries.iter().map(|(a, names)| quote(&format!("{a} {}", names.join(" ")))).collect();
    (!lines.is_empty()).then(|| {
        format!(
            "for l in {}; do grep -qxF \"$l\" /etc/hosts || printf '%s\\n' \"$l\" >> /etc/hosts; done\n",
            lines.join(" ")
        )
    })
}

/// The same in the Windows hosts file.
pub(super) fn windows_hosts(entries: &[(Ipv4Addr, Vec<String>)]) -> Option<String> {
    let lines = entries
        .iter()
        .map(|(a, names)| powershell::quote(&format!("{a} {}", names.join(" "))))
        .collect::<Vec<_>>()
        .join(", ");
    (!entries.is_empty()).then(|| {
        format!(
            "$h = \"$env:SystemRoot\\System32\\drivers\\etc\\hosts\"; foreach ($l in @({lines})) {{ if (-not (Select-String -Path $h -SimpleMatch $l -Quiet)) {{ Add-Content -Path $h -Value $l }} }}"
        )
    })
}

/// Routes to the networks the machine isn't on, then its default route through a gateway
/// (`ip route replace` is idempotent).
pub(super) fn routes(routes: &[Route], default: Option<Ipv4Addr>) -> Option<String> {
    let lines: String = routes
        .iter()
        .map(|r| format!("ip route replace {} via {}\n", r.cidr, r.via))
        .chain(default.map(|gw| format!("ip route replace default via {gw}\n")))
        .collect();
    (!lines.is_empty()).then_some(lines)
}

/// The routes on Windows, kept across restarts (`-p`); a default route is the hypervisor's.
pub(super) fn windows_routes(routes: &[Route]) -> Option<String> {
    let lines: Vec<String> = routes
        .iter()
        .filter_map(|r| {
            let block = Cidr::parse(&r.cidr)?;
            let mask = u32::MAX.checked_shl(32 - u32::from(block.len)).unwrap_or(0);
            Some(format!("route -p add {} MASK {} {}", Ipv4Addr::from(block.base), Ipv4Addr::from(mask), r.via))
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("; "))
}

/// The machine's trunks, built inside it.
pub(super) fn trunks(env: &Environment, machine: &str) -> Option<String> {
    let spec = env.spec();
    let lines: String = trunks::vms(spec)
        .iter()
        .filter(|t| t.machine == machine)
        .flat_map(|t| trunks::vm_commands(spec, t, |n, o| address(spec, n, o)))
        .map(|l| l + "\n")
        .collect();
    (!lines.is_empty()).then_some(lines)
}

/// Link impairment on the machine's own interfaces.
pub(super) fn tc(env: &Environment, machine: &str) -> Option<String> {
    let spec = env.spec();
    let m = spec.machines.get(machine)?;
    router::machine_tc_script(spec, m, |n, o| address(spec, n, o))
}

/// Offline: no new connection leaves the environment's networks (each network's block is
/// allowed, everything else new is dropped; the loopback stays open), now and at every boot
/// ([`boot::at_boot`] loads the rules). Run again, it replaces its table: nftables comes from the
/// machine's package manager (Alpine's too: the controller's default box).
pub(super) fn egress(env: &Environment) -> String {
    let lab = env.networks.values().map(|n| n.cidr.as_str()).collect::<Vec<_>>().join(", ");
    let lab6 = ipv6::egress_rule(env);
    let load = boot::at_boot(
        "egress",
        "No new connection leaves the isoloom networks",
        "nft -f /etc/isoloom-egress.nft\n",
        boot::Trigger::Boot,
        &[],
    );
    format!(
        "{PKG}\ncommand -v nft >/dev/null || if command -v apk >/dev/null; then apk add -q --no-cache nftables; else pkg nftables; fi\n\
         printf '%s\\n' 'table inet isoloom-egress' 'delete table inet isoloom-egress' 'table inet isoloom-egress {{' '  chain output {{' '    type filter hook output priority 0; policy accept;' '    oifname \"lo\" accept' '    ip daddr != {{ {lab} }} ct state new drop'{lab6} '  }}' '}}' > /etc/isoloom-egress.nft\n{load}"
    )
}
