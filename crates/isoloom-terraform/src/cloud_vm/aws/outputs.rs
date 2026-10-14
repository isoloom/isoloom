//! The module's outputs: every machine's public address and SSH user, where to stand (`ip`), the
//! ready marker, the checks, the published services.

use isoloom_plan::Environment;
use isoloom_plan::checks::Position;
use isoloom_render::hcl_builder::{Expression, Module, Template, null, object, string};

use super::image_user;
use super::machines::{controller_ip, public_ip};
use crate::vm::checks::{Invocation, Runner, output_block};
use crate::vm::module::output;
use crate::vm::{controller, is_windows, os, vms};

/// The outputs every cloud module gives.
pub fn blocks(env: &Environment, m: &mut Module) {
    let machines: Vec<(&str, Expression)> = vms(env).map(|(n, _)| (n, public_ip(env, n))).collect();
    let users: Vec<(&str, Expression)> = vms(env).map(|(n, mp)| (n, string(user(mp)))).collect();
    m.block(output("machines", object(machines)));
    m.block(output("ssh_users", object(users)));
    m.block(output("ip", stand_at(env)));
    m.block(output("ready_file", string("/var/lib/isoloom/ready")));
    if let Some(checks) = output_block(env, super::super::DIR, Invocation::Command, &|p| runner(env, p)) {
        m.commented(
            "The checks: each runner on the machine it stands for (ssh <user>@<host> '<command>'), or `isoloom test cloud-vm`.",
            checks,
        );
    }
    let published: Vec<(String, Expression)> = vms(env)
        .flat_map(|(n, _)| {
            let spec_machine = &env.spec().machines[n];
            spec_machine.services.iter().filter_map(move |sv| {
                let h = sv.publish?;
                let at = Template::new().with_expr(public_ip(env, n)).with_text(format!(":{h}"));
                Some((format!("{n}/{}", spec_machine.service_label(sv)), at.into_expression()))
            })
        })
        .collect();
    if !published.is_empty() {
        m.block(output("published", object(published)));
    }
}

/// A machine's SSH user (`isoloom` on Windows).
fn user(mp: &isoloom_plan::MachinePlan) -> &'static str {
    if is_windows(mp) { "isoloom" } else { image_user(os(mp).unwrap_or_default()) }
}

/// Where a user stands (checks, a session): the access machine, else the first Linux machine,
/// else the controller (a Windows-only environment).
fn stand_at(env: &Environment) -> Expression {
    let linux: Vec<&str> = vms(env).filter(|(_, mp)| !is_windows(mp)).map(|(n, _)| n).collect();
    let access = linux.iter().rev().find(|n| env.spec().machines[**n].access).or(linux.first());
    match access {
        Some(n) => public_ip(env, n),
        None if controller::needed(env) => controller_ip(),
        None => null(),
    }
}

/// Where a position's checks run: on its machine when it is one of the VMs, else on the
/// controller when there is one.
fn runner(env: &Environment, p: &Position) -> Option<Runner> {
    let on_vm = match p {
        Position::Machine(m) => vms(env).find(|(n, _)| n == m),
        Position::Networks => None,
    };
    match on_vm {
        Some((n, mp)) => Some(Runner {
            machine: n.to_string(),
            host: public_ip(env, n),
            user: string(user(mp)),
        }),
        None => controller::needed(env).then(|| Runner {
            machine: "controller".into(),
            host: controller_ip(),
            user: string("admin"),
        }),
    }
}
