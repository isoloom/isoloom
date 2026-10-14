//! The `cloud-vm` target: one cloud VM per machine, a Terraform module per cloud under
//! `.isoloom/cloud-vm/<cloud>/`, and the check runners every cloud shares under
//! `.isoloom/cloud-vm/checks/`.
//!
//! AWS is the target's reference cloud: its limits refuse the whole target ([`aws::refusal`]),
//! and it renders from the plan through the VM core (`crate::vm`). The other clouds are drivers
//! that still read the spec (`crate::cloud_vm_others`): a cloud whose own refusal fires drops
//! only its module (best-effort: a lab runs on the clouds whose model fits it). Their modules
//! still get the text passes they were written for (the project lists, the constraints'
//! preamble); AWS's needs none.

pub mod aws;
pub mod legacy;

pub use legacy::{
    CONTROLLER_OS, REBOOT_MARK, aligned, checks_output, cidr, has_windows, hcl_cmd, image_user, instance_type, inventory, linux_machines, linux_setup_cmds,
    needs_controller, redirects, remote_exec, tf_expr, windows_firewall, windows_project, windows_udp_redirect,
};

use isoloom_plan::{Capabilities, Environment, Feature};
use isoloom_render::backend::{self, Backend, Files, RenderError};
use isoloom_render::{GenerateError, constraints, project};
use isoloom_spec::model::Target;

use crate::vm::checks;

/// The target's folder under `.isoloom/`.
pub const DIR: &str = "cloud-vm";

/// The `cloud-vm` backend: AWS from the plan, then the other clouds' drivers.
#[derive(Debug, Clone, Copy, Default)]
pub struct CloudVmBackend;

impl Backend for CloudVmBackend {
    fn target(&self) -> Target {
        Target::CloudVm
    }

    /// Environment-level provisioning (a controller), playbook checks (refused by AWS's own
    /// limit until the cloud runs them) and `exec` checks (run on the machine).
    fn capabilities(&self) -> Capabilities {
        Capabilities::of([
            Feature::EnvironmentProvisioning,
            Feature::PlaybookChecks,
            Feature::ExecWithoutContainer,
            // Gateway networks are refused by the target's own limit (`LIMITS.gateways`), with
            // its own reason: the plan's requirement names hybrid.
            Feature::Gateways,
        ])
    }

    fn render(&self, env: &Environment) -> Result<Files, RenderError> {
        backend::check(Target::CloudVm, env, &self.capabilities())?;
        if let Some(what) = aws::refusal(env) {
            return Err(unsupported(what));
        }
        let mut files = vec![aws::render(env).map_err(|e| unsupported(format!("the AWS module couldn't be written: {e}")))?];
        files.extend(others(env));
        files.extend(checks::runners(env, DIR));
        Ok(files)
    }

    fn refusal(&self, env: &Environment) -> Option<String> {
        isoloom_plan::refusal(&env.requirements, &self.capabilities())
            .map(|r| r.to_string())
            .or_else(|| aws::refusal(env))
    }
}

fn unsupported(what: String) -> RenderError {
    RenderError::Generate(GenerateError::Unsupported { target: Target::CloudVm, what })
}

/// The other clouds' modules, for the clouds whose model fits the spec, with their text passes.
fn others(env: &Environment) -> Files {
    let spec = env.spec();
    let mut files: Files = crate::cloud_vm_others::DRIVERS
        .iter()
        .filter(|(_, _, refusal)| refusal(spec).is_none())
        .map(|(_, build, _)| build(spec))
        .collect();
    for f in &mut files {
        project::terraform(&mut f.contents);
    }
    constraints::terraform_prelude(spec, &mut files);
    files
}
