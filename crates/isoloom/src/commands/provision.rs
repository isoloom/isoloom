//! `isoloom provision`: the environment's provisioning again, in place, on local VMs.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use isoloom_core as core;
use isoloom_core::Target;

use isoloom_core::events::{ErrorCode, Scope};

use crate::failure::{Failure, fail};
use crate::prepare::prepare;
use crate::tools::process;
use crate::tools::vagrant::{CONTROLLER, wake_controller};
use crate::{Res, output};

#[derive(clap::Args)]
pub struct Args {
    /// Only these machines (all when none).
    machines: Vec<String>,
    /// The target the environment is running on (as for `run`).
    #[arg(long)]
    target: Option<String>,
    /// The project folder (holding isoloom.yml).
    #[arg(long, default_value = ".")]
    dir: PathBuf,
    /// As instance N of the spec (as for `run`).
    #[arg(long, value_name = "N")]
    instance: Option<u8>,
    /// Override a value for this command (as for `run`). Repeatable.
    #[arg(short = 's', long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
    /// Machine-readable output: JSON events, one per line, then the outcome or the error (see
    /// docs/json.md).
    #[arg(long)]
    pub json: bool,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    provision_cmd(&a.dir, a.target.as_deref(), &a.machines, a.instance, &a.sets)
}

/// `isoloom provision`: the environment's provisioning again, in place (see the command's help).
fn provision_cmd(dir: &Path, target: Option<&str>, machines: &[String], instance: Option<u8>, sets: &[String]) -> Res<ExitCode> {
    let prep = Scope::step(target, "prepare");
    output::begin(&prep, None);
    let p = prepare(dir, target, None, instance, sets)?;
    if let Some(m) = machines.iter().find(|m| !p.spec.machines.contains_key(*m)) {
        return Err(Failure::new(ErrorCode::MachineUnknown, format!("no machine named `{m}` in the spec"))
            .machine(m)
            .into());
    }
    let sub = match p.target {
        Target::Vagrant => "vagrant",
        Target::DockerVm => "docker-vm",
        Target::Hybrid => "hybrid",
        other => {
            let message = format!(
                "`isoloom provision` runs on local VMs (vagrant, docker-vm, hybrid) for now, not `{}`",
                other.id()
            );
            return Err(fail(ErrorCode::Unsupported, message));
        }
    };
    let out = p.dir.join(core::instance::output_dir(instance));
    let wd = out.join(sub);
    if !wd.join(".vagrant").is_dir() {
        return Err(fail(
            ErrorCode::NotRunning,
            format!("nothing runs here yet: `isoloom run {}` first", p.target.id()),
        ));
    }
    output::done(&prep, Some(p.target.id().to_string()));
    let at = Scope::step(Some(p.target.id()), "provision");
    let ok = if p.spec.provision.is_empty() {
        // No controller steps: each machine's own, again.
        let mut args = vec!["provision"];
        args.extend(machines.iter().map(String::as_str));
        output::begin(&at, None);
        vagrant(&wd, &args, &at)?
    } else {
        let _rehalt = wake_controller(&out, p.target, output::json())?;
        from_controller(&p.spec, &wd, machines, &at)?
    };
    if ok {
        output::done(&at, None);
    } else {
        output::failed(&at, "a provisioning step failed (see the log events)", None);
    }
    output::result("provision", ok, serde_json::json!({ "target": p.target.id(), "machines": machines }));
    Ok(output::exit_code(ok))
}

fn vagrant(wd: &Path, args: &[&str], at: &Scope) -> Res<bool> {
    let mut c = process::command("vagrant");
    c.args(args).current_dir(wd);
    Ok(process::run_step(&mut c, at)?.success())
}

/// The environment's `provision:` steps run again by the controller, limited to `machines`.
fn from_controller(spec: &core::Spec, wd: &Path, machines: &[String], at: &Scope) -> Res<bool> {
    let which = if machines.is_empty() {
        "every machine".to_string()
    } else {
        machines.join(", ")
    };
    output::begin(at, Some(format!("{which} from the controller")));
    // The project's current files (playbooks edited since), then the playbooks.
    if !vagrant(wd, &["provision", CONTROLLER, "--provision-with", "file,project"], at)? {
        return Err(fail(ErrorCode::ToolFailed, "couldn't copy the project to the controller"));
    }
    if !output::json() {
        println!("provisioning {which} from the controller");
    }
    // The script on stdin: no quoting through the host's shell, whatever the host.
    let script = core::generate::provision_script(spec, machines);
    let mut c = process::command("vagrant");
    c.args(["ssh", CONTROLLER, "-c", "sudo sh -s"]).current_dir(wd);
    Ok(process::run_step_input(&mut c, Some(script.as_bytes()), at)?.success())
}

#[cfg(test)]
mod provision_tests {
    fn example() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/ansible-pair")
    }

    #[test]
    fn provision_names_machines_of_the_spec() {
        let e = super::provision_cmd(&example(), Some("vagrant"), &["ghost".into()], None, &[]).unwrap_err();
        assert!(e.to_string().contains("no machine named `ghost`"), "{e}");
    }

    #[test]
    fn provision_runs_on_local_vms_for_now() {
        let e = super::provision_cmd(&example(), Some("proxmox"), &[], None, &[]).unwrap_err();
        assert!(e.to_string().contains("local VMs"), "{e}");
    }

    #[test]
    fn provision_needs_a_running_environment() {
        let e = super::provision_cmd(&example(), Some("vagrant"), &["web".into()], None, &[]).unwrap_err();
        assert!(e.to_string().contains("isoloom run vagrant"), "{e}");
    }
}
