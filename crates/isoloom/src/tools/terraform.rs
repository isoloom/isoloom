//! Terraform for Proxmox and the clouds: reading a module's outputs, and the AWS credentials
//! it can't find on its own.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::Res;
use crate::tools::process;

/// `terraform output -json` of a module, as a map of output name to value.
pub fn output(module: &Path) -> Res<serde_json::Value> {
    let out = Command::new("terraform")
        .args(["output", "-json"])
        .current_dir(module)
        .output()
        .map_err(|e| process::spawn_error("terraform", &e))?;
    if !out.status.success() {
        return Err(format!(
            "terraform output failed in {}: {}",
            module.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    let raw: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    // Each output is { "value": ..., "type": ... }: keep the values.
    let mut map = serde_json::Map::new();
    if let Some(obj) = raw.as_object() {
        for (k, v) in obj {
            map.insert(k.clone(), v.get("value").cloned().unwrap_or(serde_json::Value::Null));
        }
    }
    Ok(serde_json::Value::Object(map))
}

/// Temporary AWS credentials from the AWS CLI's own session (`aws login`, SSO), for Terraform,
/// which reads only keys, profiles with keys and SSO: nothing when keys are already set or the CLI
/// has no session. Never written anywhere; the child process gets them in its environment.
pub fn aws_session_env() -> Vec<(String, String)> {
    if std::env::var_os("AWS_ACCESS_KEY_ID").is_some() {
        return Vec::new();
    }
    let Ok(out) = Command::new("aws")
        .args(["configure", "export-credentials", "--format", "env-no-export"])
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(k, _)| k.starts_with("AWS_"))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}
