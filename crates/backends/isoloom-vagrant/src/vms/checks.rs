//! The checks on Vagrant: a runner script per Linux VM that has some, run where the machine
//! stands, and one for the controller (on every network), each run on demand
//! (`vagrant provision --provision-with checks`).

use isoloom_plan::checks::{self, Host, Position, Probe, Resolved};
use isoloom_plan::{Environment, MachineShape};
use isoloom_render::{GeneratedFile, OUTPUT_DIR};
use isoloom_spec::images;

use crate::ruby::{Ruby, Value};
use crate::vagrantfile;

/// The environment's checks, by where they run.
pub(crate) struct Checks<'a> {
    /// Each position's checks, in the plan's order.
    groups: Vec<(Position, Vec<&'a Resolved>)>,
    /// The checks the controller runs.
    pub(crate) controller: Vec<&'a Resolved>,
}

impl<'a> Checks<'a> {
    /// The checks of `env`, `in_their_place` standing in for the derived checks no Linux VM can
    /// run here (see [`in_their_place`]).
    pub(crate) fn of(env: &'a Environment, in_their_place: &'a [Resolved]) -> Checks<'a> {
        let spec = env.spec();
        let controller = env
            .checks
            .iter()
            .filter(|c| matches!(c.probe, Probe::Playbook { .. }) || (!runs_on_vm(env, &c.position) && (!c.derived || c.position == Position::Networks)))
            .chain(in_their_place)
            .collect();
        Checks {
            groups: checks::by_position(spec, &env.checks),
            controller,
        }
    }

    /// The checks of machine `name`, if it has some.
    pub(crate) fn of_machine(&self, name: &str) -> Option<&[&'a Resolved]> {
        self.groups
            .iter()
            .find(|(p, _)| matches!(p, Position::Machine(m) if m == name))
            .map(|(_, g)| g.as_slice())
    }
}

/// Whether a position's checks run on that machine itself: a Linux VM of the environment.
fn runs_on_vm(env: &Environment, pos: &Position) -> bool {
    match pos {
        Position::Machine(m) => env.machines.get(m).is_some_and(|mp| mp.shape == MachineShape::Vm) && !windows(env, m),
        Position::Networks => false,
    }
}

/// Whether machine `name` is a Windows VM.
pub(crate) fn windows(env: &Environment, name: &str) -> bool {
    env.spec()
        .machines
        .get(name)
        .and_then(|m| m.vm.as_ref())
        .is_some_and(|vm| images::is_windows(&vm.os))
}

/// The derived checks the controller runs from the networks when the spec's machines could run
/// them but none is a Linux VM here (a hybrid environment's containers next to Windows VMs, or
/// a supplied access machine): nothing else would check the services. Empty when a Linux VM
/// runs them, or when they already stand on the networks.
pub(crate) fn in_their_place(env: &Environment) -> Vec<Resolved> {
    let any_vm = env.machines.keys().any(|m| runs_on_vm(env, &Position::Machine(m.clone())));
    let dropped = env.checks.iter().any(|c| c.derived && c.position != Position::Networks);
    if any_vm || !dropped {
        return Vec::new();
    }
    checks::from_networks(env.spec())
}

/// The runner script of a position's checks, next to the Vagrantfile as `checks/<file>.sh`.
pub(crate) fn runner(env: &Environment, dir: &str, file: &str, position: &Position, group: &[&Resolved]) -> GeneratedFile {
    let host = |h: &Host, _: &Position| isoloom_render::checks::address_of(env, h);
    let run_script = |path: &str| format!("cd /opt/isoloom && sh {path}");
    let run_playbook = |path: &str| {
        let (folder, playbook) = path.rsplit_once('/').unwrap_or((".", path));
        format!(
            "export PATH=/opt/ansible/bin:$PATH ANSIBLE_HOST_KEY_CHECKING=False && cd /opt/isoloom/{folder} && ansible-playbook -i /etc/isoloom/inventory.ini {playbook}"
        )
    };
    let render = checks::Render {
        host: &host,
        script: &run_script,
        playbook: Some(&run_playbook),
    };
    GeneratedFile {
        path: format!("{OUTPUT_DIR}/{dir}/checks/{file}.sh"),
        contents: checks::script(position, group, &render),
    }
}

/// A position's checks, on demand. The Vagrantfile is read on every `vagrant` command, so the
/// project's check scripts are read then and written over the VM's copy first: an edited
/// script runs at the next `isoloom test`, not the copy made when the VM was provisioned. Then
/// the runner itself.
pub(crate) fn provisioner(r: &mut Ruby, file: &str, group: &[&Resolved]) {
    let runner = format!("checks/{file}.sh");
    let mut scripts: Vec<&str> = group
        .iter()
        .filter_map(|c| match &c.probe {
            Probe::Script { path } => Some(path.as_str()),
            _ => None,
        })
        .collect();
    scripts.dedup();
    let call = vagrantfile::shell("m", "checks").kw("run", Value::str("never"));
    let call = if scripts.is_empty() {
        call.kw("path", Value::str(&runner))
    } else {
        let list = Value::array(scripts.iter().map(|p| Value::str(p)));
        call.kw(
            "inline",
            Value::raw(format!("{list}{REFRESH} + File.read(File.join(__dir__, {}))", Value::str(&runner))),
        )
    };
    let derived = Value::hash([(Value::str("ISOLOOM_DERIVED"), Value::raw("ENV.fetch(\"ISOLOOM_DERIVED\", \"1\")"))]);
    r.call(call.kw("env", derived));
}

/// Writes each listed check script (`p`) over the VM's copy, from the project on the host.
const REFRESH: &str = ".map { |p| \"mkdir -p /opt/isoloom/#{File.dirname(p)} && cat > /opt/isoloom/#{p} <<'ISOLOOM_EOF'\\n#{File.read(File.join(ROOT, p))}\\nISOLOOM_EOF\\n\" }.join";
