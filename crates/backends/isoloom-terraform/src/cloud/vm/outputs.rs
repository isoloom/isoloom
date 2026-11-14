//! The outputs every cloud module gives: every machine's public address and SSH user, where to
//! stand, the ready marker, the checks, the published services.

use crate::hcl_builder::{Expression, Module, null, object, reference, string};
use isoloom_manifest::contract::OUTPUT_DIR;
use isoloom_plan::checks::Position;
use isoloom_plan::{Environment, Vm};
use isoloom_render::steps::AsRoot;
use isoloom_spec::model::Target;

use crate::checks::{Invocation, Runner, output_block};
use crate::machine::controller;
use crate::machine::provisioner::Connection;
use crate::machine::user_data::SETUP_LOG;
use crate::module::manifest::{PLAYBOOKS_S, SETUP_S};
use crate::module::output;
use crate::ready::Wait;

/// How the operator reaches the machines, for the outputs and the readiness waits.
pub(crate) struct Reach<'a> {
    /// A machine's public address.
    pub(crate) public_ip: &'a dyn Fn(&str) -> Expression,
    /// A machine's instance id (the controller's by its name, `controller::NAME`): what makes
    /// its readiness wait again when it is made again.
    pub(crate) instance: &'a dyn Fn(&str) -> Expression,
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

/// The resource of machine `name` (the controller's `isoloom_controller`), its instance being a
/// `kind` named after it (`ident`).
fn resource_of(kind: &str, name: &str) -> String {
    let id = if name == controller::NAME {
        "isoloom_controller".to_owned()
    } else {
        crate::module::ident(name)
    };
    format!("{kind}.{id}")
}

/// The outputs of a cloud with no controller, its SSH users `user` (the cloud's), its instances
/// `kind`s named after their machines.
pub(crate) fn write_as(env: &Environment, user: fn(&str) -> &'static str, public_ip: &dyn Fn(&str) -> Expression, kind: &str, module: &mut Module) {
    let reach = Reach {
        public_ip,
        instance: &|n| reference(&format!("{}.id", resource_of(kind, n))),
        user,
        controller: None,
    };
    write(env, &reach, module);
}

/// The outputs of a cloud whose machines' resources are named after them (`ident`), the
/// controller's `isoloom_controller`: `public_ip` of a resource name, the cloud's SSH `user`,
/// its instances `kind`s.
pub(crate) fn write_named(env: &Environment, user: fn(&str) -> &'static str, public_ip: fn(&str) -> Expression, kind: &str, module: &mut Module) {
    let reach = Reach {
        public_ip: &|n| public_ip(&crate::module::ident(n)),
        instance: &|n| reference(&format!("{}.id", resource_of(kind, n))),
        user,
        controller: Some(public_ip("isoloom_controller")),
    };
    write(env, &reach, module);
}

/// The machines Terraform waits for (`crate::ready`): each Linux VM, then the controller when
/// the environment has one. A Windows VM is set up by Terraform itself, over WinRM.
pub(crate) fn waited(env: &Environment) -> Vec<&str> {
    let mut out: Vec<&str> = env.vms().filter(|vm| !vm.plan.is_windows()).map(|vm| vm.name).collect();
    if controller::needed(env) {
        out.push(controller::NAME);
    }
    out
}

/// Each waited machine's readiness wait: Terraform signs in at its public address as the cloud's
/// user, brings the project in (with the check runners, which the `checks` output runs from
/// there over SSH), then waits for its set-up from its user data.
fn waits(env: &Environment, reach: &Reach, m: &mut Module) {
    let checks: Vec<String> = if env.checks_by_position().is_empty() {
        Vec::new()
    } else {
        vec![format!("{OUTPUT_DIR}/{}/checks", Target::CloudVm.id())]
    };
    for name in waited(env) {
        let (os, host, within, reboots) = if name == controller::NAME {
            let Some(host) = reach.controller.clone() else { continue };
            (controller::OS, host, PLAYBOOKS_S, 0)
        } else {
            let Some(vm) = env.vm(name) else { continue };
            (vm.os, (reach.public_ip)(name), SETUP_S, crate::ready::reboots(&vm.plan.lifecycle))
        };
        let user = (reach.user)(os);
        m.block(
            Wait {
                machine: name.to_owned(),
                connection: Connection::Ssh { host, user: user.to_owned() },
                replace_on: (reach.instance)(name),
                as_root: AsRoot::of(user),
                log: SETUP_LOG,
                within_s: within,
                reboots,
                project: true,
                outputs: checks.clone(),
            }
            .block(),
        );
    }
}

/// The outputs every cloud module gives: `machines`, each machine's `address_<m>` and
/// `ssh_user_<m>` (what the run manifest reads), `ssh_users` (for an operator reading
/// `terraform output`), `ip`, `ready_file`, `checks` and `published`.
pub(crate) fn write(env: &Environment, reach: &Reach, module: &mut Module) {
    waits(env, reach, module);
    let machines: Vec<(&str, Expression)> = env.vms().map(|Vm { name: n, .. }| (n, (reach.public_ip)(n))).collect();
    let users: Vec<(&str, Expression)> = env.vms().map(|vm| (vm.name, string(reach.user_of(vm)))).collect();
    module.block(output("machines", object(machines)));
    for vm in env.vms() {
        module.block(output(&super::manifest::address_output(vm.name), (reach.public_ip)(vm.name)));
        module.block(output(&super::manifest::ssh_user_output(vm.name), string(reach.user_of(vm))));
    }
    if let Some(address) = reach.controller.as_ref().filter(|_| controller::needed(env)) {
        module.block(output(&super::manifest::address_output(controller::NAME), address.clone()));
        module.block(output(
            &super::manifest::ssh_user_output(controller::NAME),
            string((reach.user)(controller::OS)),
        ));
    }
    module.block(output("ssh_users", object(users)));
    module.block(output("ip", stand_at(env, reach)));
    module.block(output("ready_file", string(isoloom_render::steps::READY_FILE)));
    super::console::outputs(env, module);
    if let Some(checks) = output_block(env, Target::CloudVm.id(), Invocation::Command, &|p| runner(env, reach, p)) {
        module.commented("The checks: each runner on the machine it stands for (ssh <user>@<host> '<command>').", checks);
    }
    let published = crate::module::published_output_entries(env.vms().map(|vm| (vm.name, vm.plan)), |n| (reach.public_ip)(n));
    if !published.is_empty() {
        module.block(output("published", object(published)));
    }
}

/// Where a user stands (checks, a session): the access machine, else the first Linux machine,
/// else the controller.
fn stand_at(env: &Environment, reach: &Reach) -> Expression {
    let linux: Vec<(&str, &isoloom_plan::MachinePlan)> = env.vms().filter(|vm| !vm.plan.is_windows()).map(|vm| (vm.name, vm.plan)).collect();
    let access = linux.iter().rev().find(|(_, mp)| mp.is_access()).or(linux.first()).map(|(n, _)| n);
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
    /// guest: the readiness wait signs in as the one, the run manifest's SSH steps as the
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
