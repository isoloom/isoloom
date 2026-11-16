//! What a pod gets: its secrets with their owner, its setup scripts, its command and values as the
//! kubelet reads them.

use isoloom_kubernetes::KubernetesBackend;
use isoloom_render::backend::Backend;

use crate::testing::{BASE, Outcome, file, parsed};

/// The pod spec of the workload whose containers include `container`, among the manifests `yaml`.
fn pod_with(yaml: &str, container: &str) -> Outcome<serde_yaml_ng::Value> {
    for doc in isoloom_test_support::yaml_documents(yaml) {
        let Some(pod) = isoloom_test_support::yaml_at(&doc, "spec.template.spec") else {
            continue;
        };
        let containers = pod.get("containers").and_then(serde_yaml_ng::Value::as_sequence);
        if containers
            .into_iter()
            .flatten()
            .any(|c| c.get("name").and_then(serde_yaml_ng::Value::as_str) == Some(container))
        {
            return Ok(pod.clone());
        }
    }
    Err(format!("no pod runs {container}").into())
}

/// The entries of the list at `key` of `value`, each as `(field, text)` pairs.
fn entries(value: &serde_yaml_ng::Value, key: &str) -> Vec<Vec<(String, String)>> {
    let text = |v: &serde_yaml_ng::Value| match v {
        serde_yaml_ng::Value::String(s) => s.clone(),
        other => serde_yaml_ng::to_string(other).unwrap_or_default().trim().to_owned(),
    };
    value
        .get(key)
        .and_then(serde_yaml_ng::Value::as_sequence)
        .into_iter()
        .flatten()
        .map(|e| {
            e.as_mapping()
                .into_iter()
                .flatten()
                .map(|(k, v)| (k.as_str().unwrap_or_default().to_owned(), text(v)))
                .collect()
        })
        .collect()
}

/// `(field, text)` pairs, as [`entries`] reads them.
fn pairs(p: &[(&str, &str)]) -> Vec<(String, String)> {
    p.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
}

/// A secret with an owner is copied out of the Secret volume by an init container, into a
/// memory-backed `emptyDir`, and mounted from there; one without stays on the Secret's volume.
#[test]
fn a_secret_with_an_owner_is_copied_into_memory_with_it() -> Outcome {
    let env = parsed(
        "version: 1\nname: s\nsecrets: { db-password: { env: DB_PASSWORD }, api-key: { env: API_KEY } }\nnetworks: { lan: { cidr: 10.20.0.0/24 } }\nmachines:\n  db:\n    networks: { lan: 10 }\n    app: { image: postgres }\n    files:\n      - { path: /run/secrets/pw, secret: db-password, uid: 999, gid: 998, mode: '0400' }\n      - { path: /run/secrets/api-key, secret: api-key }\n",
    )?;
    let files = KubernetesBackend.render(&env)?;
    let pod = pod_with(file(&files, "kubernetes/environment.yaml")?, "db")?;
    // The copy, in an init container's own command.
    let copy = "cp /isoloom-secrets/in/db-password /isoloom-secrets/out/db-password && chown 999:998 /isoloom-secrets/out/db-password && chmod 0400 /isoloom-secrets/out/db-password";
    let copying: Vec<&serde_yaml_ng::Value> = pod
        .get("initContainers")
        .and_then(serde_yaml_ng::Value::as_sequence)
        .into_iter()
        .flatten()
        .filter(|c| isoloom_test_support::yaml_strs(c, "command").iter().any(|a| a.contains(copy)))
        .collect();
    assert_eq!(copying.len(), 1, "one init container copies it: {pod:?}");
    let owned = entries(&pod, "volumes")
        .into_iter()
        .find(|v| v.first().is_some_and(|(_, n)| n == "isoloom-secrets-owned"));
    assert_eq!(owned, Some(pairs(&[("name", "isoloom-secrets-owned"), ("emptyDir", "medium: Memory")])));
    let db = isoloom_test_support::named(&pod, "containers", "db");
    let mounts = entries(db, "volumeMounts");
    assert!(
        mounts.contains(&pairs(&[
            ("name", "isoloom-secrets-owned"),
            ("mountPath", "/run/secrets/pw"),
            ("readOnly", "true"),
            ("subPath", "db-password")
        ])),
        "the owned copy: {mounts:?}"
    );
    assert!(
        mounts
            .iter()
            .any(|m| m.starts_with(&pairs(&[("name", "isoloom-secrets"), ("mountPath", "/run/secrets/api-key")]))),
        "the Secret's own volume: {mounts:?}"
    );
    Ok(())
}

#[test]
fn a_setup_script_with_a_space_is_one_configmap_key_its_job_runs() -> Outcome {
    let env = parsed(
        "version: 1
name: t
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  web: { networks: { lan: 10 }, app: { image: nginx:1.27 }, setup: [build/cache/my seed.sh, build/cache/a_b.sh] }
",
    )?;
    let files = KubernetesBackend.render(&env)?;
    let kustomization = isoloom_test_support::yaml(file(&files, "kubernetes/kustomization.yaml")?);
    assert_eq!(
        isoloom_test_support::yaml_strs(&kustomization, "configMapGenerator.0.files"),
        [
            "build__cache__a_5fb.sh=chart/files/project/build/cache/a_b.sh",
            "build__cache__my_20seed.sh=chart/files/project/build/cache/my seed.sh"
        ],
        "one key per script, sorted"
    );
    // The job runs both, in order, each by its key.
    let mut commands = Vec::new();
    for doc in isoloom_test_support::yaml_documents(file(&files, "kubernetes/environment.yaml")?) {
        for path in ["spec.template.spec.containers", "spec.template.spec.initContainers"] {
            for c in isoloom_test_support::yaml_at(&doc, path)
                .and_then(serde_yaml_ng::Value::as_sequence)
                .into_iter()
                .flatten()
            {
                commands.extend(
                    isoloom_test_support::yaml_strs(c, "command")
                        .into_iter()
                        .filter(|a| a.contains("/isoloom/scripts/")),
                );
            }
        }
    }
    assert!(
        matches!(commands.as_slice(), [one] if one.contains("sh /isoloom/scripts/build__cache__my_20seed.sh && sh /isoloom/scripts/build__cache__a_5fb.sh")),
        "{commands:?}"
    );
    Ok(())
}

/// What the kubelet makes of `text` in a container's `command`, `args` or an `env` value:
/// `$(NAME)` is the value of a variable defined before it (left as written when none is), `$$`
/// is one `$`, any other `$` stays.
fn kubelet_expands(text: &str, defined: &[(String, String)]) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some((before, after)) = rest.split_once('$') {
        out.push_str(before);
        if let Some(tail) = after.strip_prefix('$') {
            out.push('$');
            rest = tail;
        } else if let Some((name, tail)) = after.strip_prefix('(').and_then(|r| r.split_once(')')) {
            if let Some((_, value)) = defined.iter().rev().find(|(n, _)| n == name) {
                out.push_str(value);
            } else {
                out.push_str("$(");
                out.push_str(name);
                out.push(')');
            }
            rest = tail;
        } else {
            out.push('$');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// The `web` container of the manifests rendered for `yaml`.
fn web_container(yaml: &str) -> Outcome<serde_yaml_ng::Value> {
    let files = KubernetesBackend.render(&parsed(yaml)?)?;
    let manifests = file(&files, "kubernetes/environment.yaml")?;
    for doc in manifests.split("---\n") {
        let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(doc)?;
        let containers = doc
            .get("spec")
            .and_then(|s| s.get("template"))
            .and_then(|t| t.get("spec"))
            .and_then(|s| s.get("containers"))
            .and_then(serde_yaml_ng::Value::as_sequence);
        if let Some(web) = containers
            .into_iter()
            .flatten()
            .find(|c| c.get("name").and_then(serde_yaml_ng::Value::as_str) == Some("web"))
        {
            return Ok(web.clone());
        }
    }
    Err("no web container".into())
}

#[test]
fn the_kubelet_reads_back_a_command_and_values_with_dollars_as_written() -> Outcome {
    let spec = format!(
        "{BASE}    environment:\n      FIRST: 'one'\n      PASSWORD: 'pa$$word'\n      REF: '$(FIRST) $HOME'\n    command: 'echo pid $$ $(FIRST) $HOME'\n"
    );
    let web = web_container(&spec)?;
    let text = |v: &serde_yaml_ng::Value| v.as_str().map(str::to_owned).ok_or("not a text");
    let mut defined: Vec<(String, String)> = Vec::new();
    for var in web.get("env").and_then(serde_yaml_ng::Value::as_sequence).ok_or("no env")? {
        let (Some(name), Some(value)) = (var.get("name"), var.get("value")) else {
            continue;
        };
        let value = kubelet_expands(&text(value)?, &defined);
        defined.push((text(name)?, value));
    }
    let read = |name: &str| defined.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
    assert_eq!(read("PASSWORD"), Some("pa$$word"));
    assert_eq!(read("REF"), Some("$(FIRST) $HOME"));
    let argv: Vec<String> = ["command", "args"]
        .iter()
        .filter_map(|k| web.get(*k).and_then(serde_yaml_ng::Value::as_sequence))
        .flatten()
        .map(|a| text(a).map(|a| kubelet_expands(&a, &defined)))
        .collect::<Result<_, _>>()?;
    assert_eq!(argv.last().map(String::as_str), Some("echo pid $$ $(FIRST) $HOME"), "{argv:?}");
    Ok(())
}
