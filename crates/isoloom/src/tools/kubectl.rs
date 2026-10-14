//! kubectl for the Kubernetes target: the namespace an environment runs in, and the check
//! manifests as Kustomize renders them.

use crate::Res;
use crate::tools::process;

/// The namespace an environment runs in on Kubernetes.
pub fn namespace(name: &str) -> String {
    format!("isoloom-{name}")
}

/// `kubectl kustomize` of a folder, as text.
pub fn kustomize(dir: &str) -> Res<String> {
    let out = crate::tools::process::command("kubectl")
        .args(["kustomize", "--load-restrictor", "LoadRestrictionsNone", dir])
        .output()
        .map_err(|e| process::spawn_error("kubectl", &e))?;
    if !out.status.success() {
        return Err(format!("kubectl kustomize failed: {}", String::from_utf8_lossy(&out.stderr).trim()).into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The rendered check manifests with `ISOLOOM_DERIVED=0` in every Job's containers, so the
/// runners skip the derived checks. Other documents (the ConfigMaps) pass through.
pub fn derived_off(yaml: &str) -> Res<String> {
    use serde::Deserialize;
    use serde_yaml_ng::Value;
    let mut docs = Vec::new();
    for doc in serde_yaml_ng::Deserializer::from_str(yaml) {
        let mut v = Value::deserialize(doc)?;
        if v.get("kind").and_then(Value::as_str) == Some("Job")
            && let Some(containers) = v
                .get_mut("spec")
                .and_then(|s| s.get_mut("template"))
                .and_then(|t| t.get_mut("spec"))
                .and_then(|s| s.get_mut("containers"))
                .and_then(Value::as_sequence_mut)
        {
            for c in containers.iter_mut() {
                let env: Value = serde_yaml_ng::from_str("name: ISOLOOM_DERIVED\nvalue: \"0\"\n")?;
                match c.get_mut("env").and_then(Value::as_sequence_mut) {
                    Some(list) => list.push(env),
                    None => {
                        if let Some(m) = c.as_mapping_mut() {
                            m.insert(Value::from("env"), Value::Sequence(vec![env]));
                        }
                    }
                }
            }
        }
        docs.push(serde_yaml_ng::to_string(&v)?);
    }
    Ok(docs.join("---\n"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn derived_off_reaches_every_job_and_leaves_the_rest() {
        let yaml = "apiVersion: batch/v1\nkind: Job\nmetadata:\n  name: a\nspec:\n  template:\n    spec:\n      containers:\n      - name: check\n        image: x\n---\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: m\ndata:\n  k: v\n";
        let out = super::derived_off(yaml).unwrap();
        assert_eq!(out.matches("ISOLOOM_DERIVED").count(), 1, "{out}");
        assert!(out.contains("value: '0'") || out.contains("value: \"0\""), "{out}");
        assert!(out.contains("kind: ConfigMap") && out.contains("k: v"), "{out}");
    }
}
