//! A VM whose uplink address the guest agent reports (the router, the Docker host): a bounded
//! wait, and a clear failure when no address comes. A Debian cloud image boots and installs
//! `qemu-guest-agent` in one to three minutes on a server, four and a half on a nested Proxmox
//! under load (measured); with no DHCP, DNS or internet on the uplink the agent never reports one,
//! and the provider's own wait was 15 minutes, after which the module failed on an empty list's
//! index with no reason. The address is read with `try`, so a destroy after such a failure still
//! runs.

use crate::hcl_builder::{BinaryOperator, Block, Expression, Template, binary, boolean, call, for_list, index, list, not, number, reference, string};

/// How long the provider waits for the agent's address: the slowest boot measured, and margin.
const AGENT_WAIT: &str = "8m";

/// The `agent` block: on, an IPv4 address awaited for [`AGENT_WAIT`] at most.
pub(super) fn agent() -> Block {
    Block::builder("agent")
        .add_attribute(("enabled", boolean(true)))
        .add_attribute(("timeout", string(AGENT_WAIT)))
        .add_block(Block::builder("wait_for_ip").add_attribute(("ipv4", boolean(true))).build())
        .build()
}

/// How long a VM with environment addresses waits for its uplink address before it starts the
/// agent anyway: within [`AGENT_WAIT`], so the check below still sees the outcome.
const UPLINK_WAIT_STEPS: u32 = 84;

/// The command that starts the guest agent once the VM has an IPv4 address on its uplink: one
/// that is none of `own` (its environment addresses), waited for at most [`UPLINK_WAIT_STEPS`]
/// times 5 s. The provider's `wait_for_ip` ends at the first IPv4 address the agent reports,
/// whichever card has it: started at once, the agent reports the environment addresses (set
/// statically) before a slow DHCP answers on the uplink, and the check failed after a minute
/// instead of waiting. `own` must not be empty (a VM without environment addresses starts it at
/// once).
pub(super) fn start_agent_after_uplink(own: &[String]) -> String {
    let excluded = own.iter().map(|a| format!("-e {a}")).collect::<Vec<_>>().join(" ");
    format!(
        "i=0; while [ \"$i\" -lt {UPLINK_WAIT_STEPS} ] && ! ip -4 -o addr show scope global | awk '{{split($4, a, \"/\"); print a[1]}}' | grep -qvxF {excluded}; do i=$((i + 1)); sleep 5; done; systemctl enable --now qemu-guest-agent"
    )
}

/// The first address of [`addresses`], or `""` when it reported none (a destroy after a failed
/// apply evaluates it too).
pub(super) fn first_address(vm: &str, own: &[String]) -> Expression {
    call("try", [index(addresses(vm, own), 0), string("")])
}

/// The IPv4 addresses `vm` (`self`, or a resource address) reports that are neither the loopback
/// nor one of `own` (its environment addresses); none while the provider reports none at all (`null`
/// before the guest agent answers), so the postcondition fails with its own message rather than
/// Terraform's "Invalid function argument" from `flatten(null)`.
pub(super) fn addresses(vm: &str, own: &[String]) -> Expression {
    let a = || reference("a");
    let mut cond = binary(a(), BinaryOperator::NotEq, string("127.0.0.1"));
    if !own.is_empty() {
        let own = list(own.iter().map(|o| string(o.as_str())));
        cond = binary(cond, BinaryOperator::And, not(call("contains", [own, a()])));
    }
    let reported = call("try", [call("flatten", [reference(&format!("{vm}.ipv4_addresses"))]), list(std::iter::empty())]);
    for_list("a", reported, a(), Some(cond))
}

/// The `lifecycle` block failing the apply, with the reason, when the VM reported no uplink
/// address (its environment addresses `own` don't count); `what` names the VM.
pub(super) fn checked(what: &str, own: &[String]) -> Block {
    let condition = binary(call("length", [addresses("self", own)]), BinaryOperator::Greater, number(0));
    let message = Template::text(format!("{what} got no IPv4 address on its uplink bridge `"))
        .with_expr(reference("var.uplink_bridge"))
        .with_text(format!(
            "` within {AGENT_WAIT}: the bridge has no DHCP, or no DNS or internet to install qemu-guest-agent, which reports the address (an IPv6 address alone isn't enough: the uplink needs IPv4 from DHCP). Give it an uplink bridge with DHCP and internet (`uplink_bridge`)."
        ));
    crate::server::router::postcondition(condition, message)
}

/// The command run through a POSIX shell with stand-in tools: Unix only, like the guest it runs on.
#[cfg(all(test, unix))]
mod tests {
    use std::process::Command;

    use super::start_agent_after_uplink;

    /// The command run with a stand-in `ip` printing `addresses` (one `ip -o` line each), a
    /// `sleep` that counts, and a `systemctl` that logs: the waits, then the calls.
    fn run(addresses: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        let dir = tempfile::Builder::new().prefix("isoloom-uplink-").tempdir()?;
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin)?;
        let log = dir.path().join("log");
        let lines = addresses
            .iter()
            .map(|a| format!("echo '2: eth0    inet {a}/24 brd x scope global eth0'"))
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        let stand_ins = [
            ("ip", lines),
            ("sleep", format!("echo sleep >> {}\n", log.display())),
            ("systemctl", format!("echo \"systemctl $*\" >> {}\n", log.display())),
        ];
        for (name, body) in stand_ins {
            std::fs::write(bin.join(name), format!("#!/bin/sh\n{body}"))?;
            Command::new("chmod").arg("+x").arg(bin.join(name)).status()?;
        }
        for tool in ["awk", "grep"] {
            let found = Command::new("sh").arg("-c").arg(format!("command -v {tool}")).output()?;
            std::os::unix::fs::symlink(String::from_utf8(found.stdout)?.trim(), bin.join(tool))?;
        }
        let script = start_agent_after_uplink(&["10.254.250.254".to_owned()]);
        let out = Command::new("/bin/sh").arg("-c").arg(script).env_clear().env("PATH", &bin).output()?;
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        Ok(std::fs::read_to_string(log)?)
    }

    #[test]
    fn the_agent_starts_at_once_when_the_uplink_has_an_address() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(run(&["10.254.250.254", "192.168.1.20"])?, "systemctl enable --now qemu-guest-agent\n");
        Ok(())
    }

    /// The environment address alone (set statically, before any DHCP answer) isn't the uplink's:
    /// the agent waits, then starts anyway so the check reports the missing address.
    #[test]
    fn the_environment_address_alone_doesnt_start_the_agent() -> Result<(), Box<dyn std::error::Error>> {
        let log = run(&["10.254.250.254"])?;
        assert_eq!(log.matches("sleep").count(), 84, "{log}");
        assert!(log.ends_with("systemctl enable --now qemu-guest-agent\n"));
        Ok(())
    }
}
