//! Where the environment is reached from outside: the router's uplink address (the guest agent
//! reports it), each published service there, each machine through the router, the checks.

use isoloom_plan::Environment;
use isoloom_plan::checks::Position;
use isoloom_render::hcl_builder::{BinaryOperator, Block, Expression, Module, Template, binary, call, for_list, index, list, not, object, reference, string};
use isoloom_render::router;
use isoloom_spec::validate::Cidr;

use super::DIR;
use crate::vm::checks::{Invocation, Runner, output_block};
use crate::vm::module::output;
use crate::vm::{controller, vms};

/// The outputs.
pub fn blocks(env: &Environment, m: &mut Module) {
    m.block(router_address(env));
    behind_router(env, DIR, m);
}

/// The outputs of a target whose machines sit behind its router (Proxmox, ESXi; `dir` is its
/// folder), once `local.router_address` holds the router's uplink address.
pub(crate) fn behind_router(env: &Environment, dir: &str, m: &mut Module) {
    m.block(output("address", reference("local.router_address")));
    let published: Vec<(String, Expression)> = env
        .spec()
        .machines
        .iter()
        .filter(|(_, m)| !m.networks.is_empty())
        .flat_map(|(name, m)| {
            m.services.iter().filter_map(move |sv| {
                let h = sv.publish?;
                let at = Template::new().with_expr(reference("local.router_address")).with_text(format!(":{h}"));
                Some((format!("{name}/{}", m.service_label(sv)), at.into_expression()))
            })
        })
        .collect();
    if !published.is_empty() {
        m.commented("Published services, from the router's uplink address.", output("published", object(published)));
    }
    let machines: Vec<(String, Expression)> = env
        .spec()
        .machines
        .iter()
        .filter(|(_, m)| m.vm.is_some())
        .filter_map(|(n, _)| first_address(env, n).map(|a| (n.clone(), string(a))))
        .collect();
    m.commented(
        "The machines, through the router: ssh -J isoloom@<address> isoloom@<machine>.",
        output("machines", object(machines)),
    );
    m.block(output("ssh_user", string("isoloom")));
    if let Some(checks) = output_block(env, dir, Invocation::Script, &|p| runner(env, p)) {
        m.commented(
            &format!("The checks: each runner piped to its machine (ssh -J isoloom@<address> isoloom@<host> sh -s < <script>), or `isoloom test {dir}`."),
            checks,
        );
    }
}

/// `local.router_address`: the router's address that is none of its environment addresses.
fn router_address(env: &Environment) -> Block {
    let own = list(env.networks.keys().map(|n| string(router::address(env.spec(), n).to_string())));
    let a = || reference("a");
    let cond = binary(
        binary(a(), BinaryOperator::NotEq, string("127.0.0.1")),
        BinaryOperator::And,
        not(call("contains", [own, a()])),
    );
    let all = call("flatten", [reference("proxmox_virtual_environment_vm.isoloom_router.ipv4_addresses")]);
    Block::builder("locals")
        .add_attribute(("router_address", index(for_list("a", all, a(), Some(cond)), 0)))
        .build()
}

/// A machine's first address.
fn first_address(env: &Environment, name: &str) -> Option<String> {
    env.machines.get(name)?.addresses.values().next().map(ToString::to_string)
}

/// Where a position's checks run: on its machine when it is a VM, else from the controller,
/// else from the first machine.
fn runner(env: &Environment, p: &Position) -> Option<Runner> {
    let at = |machine: &str, host: String| Runner {
        machine: machine.to_string(),
        host: string(host),
        user: string("isoloom"),
    };
    if let Position::Machine(m) = p
        && env.spec().machines.get(m).is_some_and(|x| x.vm.is_some())
    {
        return first_address(env, m).map(|a| at(m, a));
    }
    if controller::needed(env) {
        let first = env.networks.values().next().and_then(|n| Cidr::parse(&n.cidr))?;
        return Some(at("controller", first.controller().to_string()));
    }
    vms(env).find_map(|(n, _)| first_address(env, n).map(|a| at(n, a)))
}
