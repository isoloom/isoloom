//! Each deployment of cloud services has a name (`isoloom-instance`, next to the state), which
//! the scripts pass to the module as `TF_VAR_isoloom_instance` for its tags. After `down`, what
//! the cloud still holds with that tag is listed, never deleted.

use std::path::Path;
use std::process::Stdio;

use isoloom_spec::model::CloudProvider;

use crate::Res;
use crate::tools::{process, terraform};

/// The file holding the deployment's name, in the cloud-services folder.
const NAME_FILE: &str = "isoloom-instance";
/// The variables the AWS CLI reads its region from.
const REGION_ENV: [&str; 2] = ["AWS_REGION", "AWS_DEFAULT_REGION"];

/// A unique name for one deployment of an environment: `isoloom-<name>[-<instance>]-<8 hex>`.
fn deployment_name(name: &str, instance: Option<u8>) -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    h.write_u128(nanos);
    h.write_u32(std::process::id());
    let suffix = instance.map(|n| format!("-{n}")).unwrap_or_default();
    format!("isoloom-{name}{suffix}-{:08x}", h.finish() as u32)
}

/// Names the deployment in `wd` (the cloud-services folder) before its first apply; a name
/// already there stays until `down` finds nothing left with it.
pub fn name_deployment(wd: &Path, name: &str, instance: Option<u8>) -> Res<()> {
    let file = wd.join(NAME_FILE);
    if !file.exists() {
        std::fs::write(&file, format!("{}\n", deployment_name(name, instance)))?;
    }
    Ok(())
}

/// After `down`: the resources still tagged `isoloom-instance=<this deployment>` in the cloud,
/// listed (never deleted). AWS only, through the Resource Groups Tagging API (`aws` CLI), in the
/// region `cloud.env` or `cloud.vars.region` names. The name is forgotten once nothing is left
/// in a known region.
pub fn leftovers(spec: &isoloom_spec::Spec, wd: &Path) {
    let file = wd.join(NAME_FILE);
    let Some(id) = std::fs::read_to_string(&file).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) else {
        return;
    };
    let Some(c) = &spec.cloud else { return };
    if c.provider != CloudProvider::Aws {
        let _ = std::fs::remove_file(&file);
        return;
    }
    let (region, known) = region(c, wd);
    let Some(arns) = tagged(c, &id, region.as_deref()) else { return };
    if arns.is_empty() {
        if known {
            let _ = std::fs::remove_file(&file);
        } else {
            eprintln!(
                "note: nothing tagged isoloom-instance={id} in the aws CLI's default region; the region isn't known (set AWS_REGION in `cloud.env`), so the name is kept for the next `down`"
            );
        }
        return;
    }
    eprintln!(
        "\nStill in the cloud, tagged isoloom-instance={id} ({}; created outside Terraform, or still being deleted). Not deleted:",
        arns.len()
    );
    for a in &arns {
        eprintln!("  {a}");
    }
}

/// The region to search, when Isoloom picks it, and whether the region searched is known at
/// all: `cloud.env`'s (the CLI reads it, so none to pass), else `cloud.vars.region` as applied
/// (an input's value read back from `inputs.tfvars.json`), else the user's own environment.
/// Unknown, the CLI's default is searched.
fn region(c: &isoloom_spec::CloudServices, wd: &Path) -> (Option<String>, bool) {
    if REGION_ENV.iter().any(|k| c.env.contains_key(*k)) {
        return (None, true);
    }
    let applied = || -> Option<String> {
        let v = c.vars.get("region")?;
        if isoloom_spec::CloudServices::inputs_in(v).is_empty() {
            return v.as_str().map(String::from);
        }
        let inputs: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(wd.join("inputs.tfvars.json")).ok()?).ok()?;
        inputs["region"].as_str().map(String::from)
    };
    match applied().filter(|r| !r.is_empty() && !r.contains("{{")) {
        Some(r) => (Some(r), true),
        None => (None, REGION_ENV.iter().any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty()))),
    }
}

/// The ARNs tagged `isoloom-instance=<id>` (in `region` when given), or `None` when the `aws`
/// CLI can't say (it says why).
fn tagged(c: &isoloom_spec::CloudServices, id: &str, region: Option<&str>) -> Option<Vec<String>> {
    let mut cmd = process::command("aws");
    cmd.args(["resourcegroupstaggingapi", "get-resources", "--tag-filters"])
        .arg(format!("Key=isoloom-instance,Values={id}"))
        .args(["--query", "ResourceTagMappingList[].ResourceARN", "--output", "json"])
        .envs(&c.env)
        .envs(terraform::aws_session_env())
        .stderr(Stdio::null());
    if let Some(r) = region {
        cmd.args(["--region", r]);
    }
    match cmd.output() {
        Ok(o) if o.status.success() => Some(serde_json::from_slice(&o.stdout).unwrap_or_default()),
        Ok(_) => {
            eprintln!("note: couldn't list what is left tagged isoloom-instance={id} (`aws resourcegroupstaggingapi get-resources` failed)");
            None
        }
        Err(_) => {
            eprintln!("note: install the aws CLI to list what is left tagged isoloom-instance={id}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::deployment_name;

    #[test]
    fn deployment_names_are_unique_and_say_whose() {
        let a = deployment_name("lab", None);
        assert!(a.starts_with("isoloom-lab-") && a.len() == "isoloom-lab-".len() + 8, "{a}");
        assert!(deployment_name("lab", Some(2)).starts_with("isoloom-lab-2-"));
        assert_ne!(a, deployment_name("lab", None));
    }
}
