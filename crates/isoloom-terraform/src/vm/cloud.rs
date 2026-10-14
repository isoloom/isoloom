//! What every cloud module repeats around its own resources: the auto-stop, published-port
//! redirects, the common variables, the environment's id and names, and the outputs (every
//! machine's public address and SSH user, where to stand, the ready marker, the checks, the
//! published services).

use isoloom_plan::checks::Position;
use isoloom_plan::{Environment, MachinePlan};
use isoloom_render::hcl_builder::{
    BinaryOperator, Block, Expression, Module, Template, binary, call, conditional, list, null, number, object, reference, string,
};

use super::checks::{Invocation, Runner, output_block};
use super::module::{Variable, inputs_variable, output};
use super::{controller, is_windows, vms};

/// `var.auto_stop_minutes > 0 ? <yes> : <no>`.
pub fn auto_stop(yes: Expression, no: Expression) -> Expression {
    conditional(binary(reference("var.auto_stop_minutes"), BinaryOperator::Greater, number(0)), yes, no)
}

/// The auto-stop as a first-boot script: `#!/bin/sh` then `shutdown -h +<minutes>`.
pub fn shutdown_script() -> Expression {
    Template::text("#!/bin/sh\nshutdown -h +")
        .with_expr(reference("var.auto_stop_minutes"))
        .with_text("\n")
        .into_expression()
}

/// The auto-stop scheduled again after a restart (an `after_reboot` hook).
pub fn reschedule_auto_stop() -> Template {
    Template::new().with_expr(auto_stop(
        Template::text("sudo shutdown -h +")
            .with_expr(reference("var.auto_stop_minutes"))
            .into_expression(),
        string("true"),
    ))
}

/// A published port that differs from the service's own, redirected to it inside the machine
/// (a `before_egress` hook).
pub fn redirects(mp: &MachinePlan) -> Vec<Template> {
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

/// The variables every cloud module has (after its own): who may reach the machines, the SSH
/// keys, the auto-stop, the expiry, and the inputs when the spec has some.
pub fn variables(env: &Environment) -> Vec<Block> {
    let mut v = vec![
        Variable::new("allowed_cidr", "string")
            .describe("Who may reach the machines (SSH and the published ports), e.g. your IP/32")
            .block(),
        Variable::new("ssh_public_key", "string").block(),
        Variable::new("ssh_private_key_file", "string")
            .describe("The private key of ssh_public_key: Terraform sets the machines up over SSH")
            .block(),
        Variable::new("auto_stop_minutes", "number")
            .default(number(0))
            .describe("Shut the machines down after this many minutes (0: never)")
            .block(),
        Variable::new("expires_at", "string")
            .default(string(""))
            .describe("When the environment should end, in Unix seconds (empty: no end), as a tag on every resource so a reaper can find what to destroy")
            .block(),
    ];
    if !env.spec().inputs.is_empty() {
        v.push(inputs_variable());
    }
    v
}

/// The environment's random id (`terraform_data.id`) and the locals: its `name`, the project's
/// `root`, then `extra`.
pub fn id_and_locals(env: &Environment, extra: Vec<(&str, Expression)>) -> Vec<Block> {
    let id = call("substr", [call("replace", [call("uuid", []), string("-"), string("")]), number(0), number(8)]);
    let name = Template::text(format!("isoloom-{}-", env.name))
        .with_expr(reference("terraform_data.id.output"))
        .into_expression();
    let root = call(
        "abspath",
        [Template::new().with_expr(reference("path.module")).with_text("/../../..").into_expression()],
    );
    let locals = extra.into_iter().fold(
        Block::builder("locals").add_attribute(("name", name)).add_attribute(("root", root)),
        isoloom_render::hcl_builder::BlockBuilder::add_attribute,
    );
    vec![
        Block::builder("resource")
            .add_label("terraform_data")
            .add_label("id")
            .add_attribute(("input", id))
            .add_block(
                Block::builder("lifecycle")
                    .add_attribute(("ignore_changes", list([reference("input")])))
                    .build(),
            )
            .build(),
        locals.build(),
    ]
}

/// The tags of a cloud whose tags are a list of words: `isoloom`, the environment, its
/// instance's name, and `isoloom-expires-<time>` when it has an end.
pub fn tag_list(env: &Environment) -> Expression {
    let expires = Template::text("isoloom-expires-").with_expr(reference("var.expires_at")).into_expression();
    let none = binary(reference("var.expires_at"), BinaryOperator::Eq, string(""));
    call(
        "concat",
        [
            list([string("isoloom"), string(env.name.as_str()), reference("local.name")]),
            conditional(none, list(Vec::new()), list([expires])),
        ],
    )
}

/// How the operator reaches the machines, for the outputs.
pub struct Reach<'a> {
    /// A machine's public address.
    pub public_ip: &'a dyn Fn(&str) -> Expression,
    /// A machine's SSH user.
    pub user: &'a dyn Fn(&MachinePlan) -> String,
    /// The controller's public address, when there is one.
    pub controller: Option<Expression>,
}

/// The outputs every cloud module gives: `machines`, `ssh_users`, `ip`, `ready_file`, `checks`
/// and `published`.
pub fn outputs(env: &Environment, reach: &Reach, m: &mut Module) {
    let machines: Vec<(&str, Expression)> = vms(env).map(|(n, _)| (n, (reach.public_ip)(n))).collect();
    let users: Vec<(&str, Expression)> = vms(env).map(|(n, mp)| (n, string((reach.user)(mp)))).collect();
    m.block(output("machines", object(machines)));
    m.block(output("ssh_users", object(users)));
    m.block(output("ip", stand_at(env, reach)));
    m.block(output("ready_file", string("/var/lib/isoloom/ready")));
    if let Some(checks) = output_block(env, crate::cloud_vm::DIR, Invocation::Command, &|p| runner(env, reach, p)) {
        m.commented(
            "The checks: each runner on the machine it stands for (ssh <user>@<host> '<command>'), or `isoloom test cloud-vm`.",
            checks,
        );
    }
    let published: Vec<(String, Expression)> = vms(env)
        .flat_map(|(n, _)| {
            let sm = &env.spec().machines[n];
            sm.services.iter().filter_map(move |sv| {
                let h = sv.publish?;
                let at = Template::new().with_expr((reach.public_ip)(n)).with_text(format!(":{h}"));
                Some((format!("{n}/{}", sm.service_label(sv)), at.into_expression()))
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
    let linux: Vec<&str> = vms(env).filter(|(_, mp)| !is_windows(mp)).map(|(n, _)| n).collect();
    let access = linux.iter().rev().find(|n| env.spec().machines[**n].access).or(linux.first());
    match (access, &reach.controller) {
        (Some(n), _) => (reach.public_ip)(n),
        (None, Some(c)) if controller::needed(env) => c.clone(),
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
            machine: n.to_string(),
            host: (reach.public_ip)(n),
            user: string((reach.user)(mp)),
        }),
        None => reach.controller.clone().filter(|_| controller::needed(env)).map(|host| Runner {
            machine: "controller".into(),
            host,
            user: string("admin"),
        }),
    }
}

/// `"${local.name}<suffix>"`: a resource's name in the environment.
pub fn name_with(suffix: &str) -> Expression {
    Template::new().with_expr(reference("local.name")).with_text(suffix).into_expression()
}
