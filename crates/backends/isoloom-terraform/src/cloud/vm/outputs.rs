//! The outputs every cloud module gives: every machine's public address and SSH user, where to
//! stand, the ready marker, the checks, the published services.

use isoloom_plan::checks::Position;
use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{Expression, Module, Template, null, object, string};
use isoloom_spec::model::Target;

use crate::checks::{Invocation, Runner, output_block};
use crate::machine::{controller, vms};
use crate::module::output;

/// How the operator reaches the machines, for the outputs.
pub(crate) struct Reach<'a> {
    /// A machine's public address.
    pub(crate) public_ip: &'a dyn Fn(&str) -> Expression,
    /// A machine's SSH user.
    pub(crate) user: &'a dyn Fn(&MachinePlan) -> String,
    /// The controller's public address and SSH user, when there is one.
    pub(crate) controller: Option<(Expression, &'a str)>,
}

/// The outputs of a cloud with no controller, whose machines all log in as `user`.
pub(crate) fn write_as(env: &Environment, user: &str, public_ip: &dyn Fn(&str) -> Expression, m: &mut Module) {
    let reach = Reach {
        public_ip,
        user: &|_| user.to_owned(),
        controller: None,
    };
    write(env, &reach, m);
}

/// The outputs every cloud module gives: `machines`, `ssh_users`, `ip`, `ready_file`, `checks`
/// and `published`.
pub(crate) fn write(env: &Environment, reach: &Reach, m: &mut Module) {
    let machines: Vec<(&str, Expression)> = vms(env).map(|(n, _)| (n, (reach.public_ip)(n))).collect();
    let users: Vec<(&str, Expression)> = vms(env).map(|(n, mp)| (n, string((reach.user)(mp)))).collect();
    m.block(output("machines", object(machines)));
    m.block(output("ssh_users", object(users)));
    m.block(output("ip", stand_at(env, reach)));
    m.block(output("ready_file", string("/var/lib/isoloom/ready")));
    if let Some(checks) = output_block(env, Target::CloudVm.id(), Invocation::Command, &|p| runner(env, reach, p)) {
        m.commented(
            "The checks: each runner on the machine it stands for (ssh <user>@<host> '<command>'), or `isoloom test cloud-vm`.",
            checks,
        );
    }
    let published: Vec<(String, Expression)> = vms(env)
        .filter_map(|(n, _)| Some((n, env.spec().machines.get(n)?)))
        .flat_map(|(n, sm)| {
            sm.ports.iter().filter_map(move |sv| {
                let h = sv.publish?;
                let at = Template::new().with_expr((reach.public_ip)(n)).with_text(format!(":{h}"));
                Some((format!("{n}/{}", sm.port_label(sv)), at.into_expression()))
            })
        })
        .collect();
    if !published.is_empty() {
        m.block(output("published", object(published)));
    }
}

/// Where a user stands (checks, a session): the access machine, else the first Linux machine,
/// else the controller.
fn stand_at(env: &Environment, reach: &Reach) -> Expression {
    let linux: Vec<&str> = vms(env).filter(|(_, mp)| !mp.is_windows()).map(|(n, _)| n).collect();
    let access = linux
        .iter()
        .rev()
        .find(|n| env.spec().machines.get(**n).is_some_and(|m| m.access))
        .or(linux.first());
    match (access, &reach.controller) {
        (Some(n), _) => (reach.public_ip)(n),
        (None, Some((c, _))) if controller::needed(env) => c.clone(),
        _ => null(),
    }
}

/// Where a position's checks run: on its machine when it is one of the VMs, else on the
/// controller when there is one.
fn runner(env: &Environment, reach: &Reach, p: &Position) -> Option<Runner> {
    let on_vm = match p {
        Position::Machine(m) => vms(env).find(|(n, _)| n == m),
        Position::Networks => None,
    };
    match on_vm {
        Some((n, mp)) => Some(Runner {
            machine: n.to_owned(),
            host: (reach.public_ip)(n),
            user: string((reach.user)(mp)),
        }),
        None => reach.controller.clone().filter(|_| controller::needed(env)).map(|(host, user)| Runner {
            machine: "controller".to_owned(),
            host,
            user: string(user),
        }),
    }
}
