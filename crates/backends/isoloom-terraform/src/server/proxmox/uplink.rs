//! A VM whose uplink address the guest agent reports (the router, the Docker host): a bounded
//! wait, and a clear failure when no address comes. A Debian cloud image boots and installs
//! `qemu-guest-agent` in one to three minutes on a server, four and a half on a nested Proxmox
//! under load (measured); with no DHCP, DNS or internet on the uplink the agent never reports one,
//! and the provider's own wait was 15 minutes, after which the module failed on an empty list's
//! index with no reason. The address is read with `try`, so a destroy after such a failure still
//! runs.

use isoloom_render::hcl_builder::{BinaryOperator, Block, Expression, Template, binary, boolean, call, for_list, index, list, not, number, reference, string};

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

/// The first address of [`addresses`], or `""` when it reported none (a destroy after a failed
/// apply evaluates it too).
pub(super) fn first_address(vm: &str, own: &[String]) -> Expression {
    call("try", [index(addresses(vm, own), 0), string("")])
}

/// The IPv4 addresses `vm` (`self`, or a resource address) reports that are neither the loopback
/// nor one of `own` (its lab addresses).
pub(super) fn addresses(vm: &str, own: &[String]) -> Expression {
    let a = || reference("a");
    let mut cond = binary(a(), BinaryOperator::NotEq, string("127.0.0.1"));
    if !own.is_empty() {
        let own = list(own.iter().map(|o| string(o.as_str())));
        cond = binary(cond, BinaryOperator::And, not(call("contains", [own, a()])));
    }
    for_list("a", call("flatten", [reference(&format!("{vm}.ipv4_addresses"))]), a(), Some(cond))
}

/// The `lifecycle` block failing the apply, with the reason, when the VM reported no uplink
/// address (its lab addresses `own` don't count); `what` names the VM.
pub(super) fn checked(what: &str, own: &[String]) -> Block {
    let condition = binary(call("length", [addresses("self", own)]), BinaryOperator::Greater, number(0));
    let message = Template::text(format!("{what} got no IPv4 address on its uplink bridge `"))
        .with_expr(reference("var.uplink_bridge"))
        .with_text(format!(
            "` within {AGENT_WAIT}: the bridge has no DHCP, or no DNS or internet to install qemu-guest-agent, which reports the address. Give it an uplink bridge with DHCP and internet (`uplink_bridge`)."
        ));
    let postcondition = Block::builder("postcondition")
        .add_attribute(("condition", condition))
        .add_attribute(("error_message", message.into_expression()))
        .build();
    Block::builder("lifecycle").add_block(postcondition).build()
}
