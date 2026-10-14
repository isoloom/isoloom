//! The `cloud-services` target's two sides of a deployment: the launch-time inputs `run`
//! hands the module, and the outputs `test` and the message read back from it.

use std::path::Path;
use std::process::{Command, Stdio};

use isoloom_core as core;

use crate::tools::terraform;
use crate::{Res, spec};

/// A cloud-services environment's outputs, by the spec's names: `outputs.sh` read (empty when
/// it isn't deployed or Terraform can't say).
pub fn outputs(dir: &Path, instance: Option<u8>) -> indexmap::IndexMap<String, String> {
    let script = dir.join(core::instance::output_dir(instance)).join("cloud-services/outputs.sh");
    let Ok(out) = Command::new("sh")
        .arg(&script)
        .envs(terraform::aws_session_env())
        .stderr(Stdio::null())
        .output()
    else {
        return Default::default();
    };
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let Ok((spec, _)) = spec::load_settings(dir, None, &[]) else {
        return Default::default();
    };
    let Some(cloud) = &spec.cloud else { return Default::default() };
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
pub fn write_inputs(cloud: &core::CloudServices, wd: &Path) -> Res<()> {
    if let Some(h) = cloud.hourly_usd {
        eprintln!("note: about ${h:.2} an hour while it exists (cloud.hourly_usd); `isoloom down` destroys it");
    }
    let mut values = serde_json::Map::new();
    let mut missing = Vec::new();
    for (k, v) in &cloud.vars {
        if let Some(i) = core::CloudServices::input_of(v) {
            match std::env::var(i) {
                Ok(val) if !val.is_empty() => {
                    values.insert(k.clone(), serde_json::Value::String(val));
                }
                _ => missing.push(i.to_string()),
            }
        }
    }
    if !missing.is_empty() {
        return Err(format!("set {} in the environment (the services take them at launch)", missing.join(", ")).into());
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
