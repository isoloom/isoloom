//! The outputs every cloud module gives: every machine's public address and SSH user, where to
//! stand, the ready marker, the checks, the published services.

use crate::hcl_builder::{Expression, Module, Template, null, object, string};
use isoloom_plan::checks::Position;
use isoloom_plan::{Environment, Vm};
use isoloom_render::run_manifest::RUNNER;
use isoloom_spec::model::Target;

use crate::checks::{Invocation, Runner, output_block};
use crate::machine::{controller, vms};
use crate::module::output;

/// How the operator reaches the machines, for the outputs.
pub(crate) struct Reach<'a> {
    /// A machine's public address.
    pub(crate) public_ip: &'a dyn Fn(&str) -> Expression,
    /// The SSH user on a machine of an OS: the cloud's (`crate::cloud::Base::user`), the one
    /// its run manifest gives too.
    pub(crate) user: fn(&str) -> &'static str,
    /// The controller's public address, when there is one (its user: `user` of its OS).
    pub(crate) controller: Option<Expression>,
}

impl Reach<'_> {
    /// The SSH user on VM `vm`.
    fn user_of(&self, vm: Vm<'_>) -> &'static str {
        (self.user)(vm.os)
    }
}

/// The outputs of a cloud with no controller, its SSH users `user` (the cloud's).
pub(crate) fn write_as(env: &Environment, user: fn(&str) -> &'static str, public_ip: &dyn Fn(&str) -> Expression, m: &mut Module) {
    let reach = Reach {
        public_ip,
        user,
        controller: None,
    };
    write(env, &reach, m);
}

/// The outputs of a cloud whose machines' resources are named after them (`ident`), the
/// controller's `isoloom_controller`: `public_ip` of a resource name, the cloud's SSH `user`.
pub(crate) fn write_named(env: &Environment, user: fn(&str) -> &'static str, public_ip: fn(&str) -> Expression, m: &mut Module) {
    let reach = Reach {
        public_ip: &|n| public_ip(&crate::module::ident(n)),
        user,
        controller: Some(public_ip("isoloom_controller")),
    };
    write(env, &reach, m);
}

/// The outputs every cloud module gives: `machines`, each machine's `address_<m>` and
/// `ssh_user_<m>` (what the run manifest reads), `ssh_users` (for an operator reading
/// `terraform output`), `ip`, `ready_file`, `checks` and `published`.
pub(crate) fn write(env: &Environment, reach: &Reach, m: &mut Module) {
    let machines: Vec<(&str, Expression)> = vms(env).map(|(n, _)| (n, (reach.public_ip)(n))).collect();
    let users: Vec<(&str, Expression)> = env.vms().map(|vm| (vm.name, string(reach.user_of(vm)))).collect();
    m.block(output("machines", object(machines)));
    for vm in env.vms() {
        m.block(output(&super::manifest::address_output(vm.name), (reach.public_ip)(vm.name)));
        m.block(output(&super::manifest::ssh_user_output(vm.name), string(reach.user_of(vm))));
    }
    if let Some(address) = reach.controller.as_ref().filter(|_| controller::needed(env)) {
        m.block(output(&super::manifest::address_output(controller::NAME), address.clone()));
        m.block(output(
            &super::manifest::ssh_user_output(controller::NAME),
            string((reach.user)(controller::OS)),
        ));
    }
    m.block(output("ssh_users", object(users)));
    m.block(output("ip", stand_at(env, reach)));
    m.block(output("ready_file", string(isoloom_render::steps::READY_FILE)));
    if let Some(checks) = output_block(env, Target::CloudVm.id(), Invocation::Command, &|p| runner(env, reach, p)) {
        m.commented(
            &format!("The checks: each runner on the machine it stands for (ssh <user>@<host> '<command>'), or {RUNNER}."),
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
        (None, Some(c)) if controller::needed(env) => c.clone(),
        _ => null(),
    }
}

/// Where a position's checks run: on its machine when it is one of the VMs, else on the
/// controller when there is one.
fn runner(env: &Environment, reach: &Reach, p: &Position) -> Option<Runner> {
    let on_vm = match p {
        Position::Machine(m) => env.vm(m),
        Position::Networks => None,
    };
    match on_vm {
        Some(vm) => Some(Runner {
            machine: vm.name.to_owned(),
            host: (reach.public_ip)(vm.name),
            user: string(reach.user_of(vm)),
        }),
        None => reach.controller.clone().filter(|_| controller::needed(env)).map(|host| Runner {
            machine: controller::NAME.to_owned(),
            host,
            user: string((reach.user)(controller::OS)),
        }),
    }
}

#[cfg(test)]
mod tests {
    use crate::hcl_builder::Module;
    use isoloom_spec::model::Target;

    use crate::cloud::{aws, azure, digitalocean, gcp, linode, oci};

    /// Each `ssh_user_<machine>` output is the user the cloud's driver gives the run manifest's
    /// guest: the runner's secret delivery signs in as the one, its ready and SSH steps as the
    /// other, so a difference locks the delivery out.
    #[test]
    fn every_clouds_ssh_user_outputs_are_its_drivers_users() {
        let spec = "version: 1\nname: u\nnetworks: { lan: { cidr: 10.30.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, os: debian@12 }\n  api: { networks: { lan: 11 }, os: ubuntu@24.04 }\n";
        let env = crate::testing::planned(spec, Target::CloudVm);
        for driver in [
            aws::vm::DRIVER,
            azure::vm::DRIVER,
            gcp::vm::DRIVER,
            digitalocean::vm::DRIVER,
            linode::vm::DRIVER,
            oci::vm::DRIVER,
        ] {
            let mut m = Module::new("");
            (driver.write)(&env, &mut m).unwrap();
            let text = m.render().unwrap();
            for vm in env.vms() {
                let (name, user) = (vm.name, (driver.base.user)(vm.os));
                let output = format!("output \"{}\" {{\n  value = \"{user}\"\n}}", super::super::manifest::ssh_user_output(name));
                assert!(text.contains(&output), "{name}: {output}\n{text}");
            }
        }
    }
}
