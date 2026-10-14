//! The `cloud-services` target's two sides of a deployment: the launch-time inputs `run`
//! hands the module, and the outputs `test` and the message read back from it; the
//! deployment's name and what is left with it after `down` ([`mod@leftovers`]).

mod leftovers;

use std::path::Path;
use std::process::Stdio;

use isoloom_runtime::events::ErrorCode;

use crate::failure::Failure;
use crate::tools::terraform;
use crate::{Res, spec};

pub use leftovers::{leftovers, name_deployment};

/// A cloud-services environment's outputs, by the spec's names: `outputs.sh` read (empty when
/// it isn't deployed or Terraform can't say).
pub fn outputs(dir: &Path, instance: Option<u8>) -> indexmap::IndexMap<String, String> {
    let script = dir.join(isoloom_spec::instance::output_dir(instance)).join("cloud-services/outputs.sh");
    let Ok(out) = crate::tools::process::command("sh")
        .arg(&script)
        .envs(terraform::aws_session_env())
        .stderr(Stdio::null())
        .output()
    else {
        return indexmap::IndexMap::default();
    };
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let Ok((spec, _)) = spec::load_settings(dir, None, &[]) else {
        return indexmap::IndexMap::default();
    };
    let Some(cloud) = &spec.cloud else { return indexmap::IndexMap::default() };
    cloud
        .outputs
        .iter()
        .filter_map(|(name, output)| {
            let v = &json[output]["value"];
            let text = v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
            (!v.is_null()).then(|| (name.clone(), text))
        })
        .collect()
}

/// Cloud services taking launch-time inputs: from the environment at `run`, written next to
/// the state so `down` destroys with the same values.
pub fn write_inputs(cloud: &isoloom_spec::CloudServices, wd: &Path) -> Res<()> {
    if let Some(h) = cloud.hourly_usd
        && !crate::output::json()
    {
        eprintln!("note: about ${h:.2} an hour while it exists (cloud.hourly_usd); `isoloom down` destroys it");
    }
    let mut values = serde_json::Map::new();
    let mut missing: Vec<String> = Vec::new();
    for (k, v) in &cloud.vars {
        if isoloom_spec::CloudServices::inputs_in(v).is_empty() {
            continue;
        }
        // Each `{{ inputs.NAME }}` in the value, at any depth, filled from the environment.
        let filled = isoloom_spec::CloudServices::fill_inputs(v, &mut |i| match spec::input(i) {
            Some(val) if !val.is_empty() => Some(val),
            _ => {
                if !missing.iter().any(|m| m == i) {
                    missing.push(i.to_string());
                }
                None
            }
        });
        values.insert(k.clone(), filled);
    }
    if !missing.is_empty() {
        let message = format!("set {} in the environment (the services take them at launch)", missing.join(", "));
        return Err(Failure::new(ErrorCode::InputMissing, message)
            .hint("set them in the shell, or in the profile's `inputs`")
            .into());
    }
    if !values.is_empty() {
        let file = wd.join("inputs.tfvars.json");
        std::fs::write(&file, serde_json::to_string_pretty(&serde_json::Value::Object(values))?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    Ok(())
}
