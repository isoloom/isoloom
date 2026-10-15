//! Secrets at `run`: every one read from its store before anything starts (a missing one stops
//! the run and says which), then handed to the target's own mechanism, never to a file: Compose
//! reads each from a variable of its own environment, Kubernetes from a Secret created with
//! `kubectl apply --server-side` (which, unlike a client-side apply, keeps no copy of the object
//! in an annotation), and a VM or a Docker host gets them over its own connection (`vms`).

use std::process::Command;

use isoloom_runtime::events::{ErrorCode, Scope};
use isoloom_runtime::secrets::{self, Lookup, SecretValue};
use isoloom_spec::Target;
use isoloom_spec::model::secret_variable;

use crate::failure::Failure;
use crate::prepare::Prepared;
use crate::tools::{kubectl, process};
use crate::{Res, output, spec};

pub mod vms;
pub mod windows;

/// The secrets' values, by name, in the spec's order.
pub type Values = Vec<(String, SecretValue)>;

/// Every secret of the spec, read from its store; an error naming each one that can't be read.
pub fn read(p: &Prepared) -> Res<Values> {
    if p.spec.secrets.is_empty() {
        return Ok(Vec::new());
    }
    let var = |name: &str| spec::input(name);
    let at = Lookup { project: &p.dir, var: &var };
    secrets::resolve(&p.spec, &at).map_err(|errors| {
        let lines: Vec<String> = errors.iter().map(ToString::to_string).collect();
        Failure::new(
            ErrorCode::SecretMissing,
            format!("can't read the secrets the environment needs:\n  {}", lines.join("\n  ")),
        )
        .hint("set them in their stores (see `secrets:` in the spec), then run again")
        .into()
    })
}

/// Where each secret of `spec` comes from, and why it can't be read now (`None`: it can), for
/// `isoloom doctor`. Each store is read and the values dropped at once.
pub fn sources(spec: &isoloom_spec::Spec, dir: &std::path::Path) -> Vec<Source> {
    let var = |name: &str| spec::input(name);
    let at = Lookup { project: dir, var: &var };
    let errors = secrets::resolve(spec, &at).err().unwrap_or_default();
    spec.secrets
        .iter()
        .map(|(name, source)| Source {
            name: name.clone(),
            store: source.store(),
            reference: source.reference().to_string(),
            missing: errors.iter().find(|e| &e.name == name).map(|e| e.why.clone()),
        })
        .collect()
}

/// A secret as `isoloom doctor` reports it: never its value.
#[derive(serde::Serialize)]
pub struct Source {
    /// The secret's name.
    #[serde(rename = "secret")]
    pub name: String,
    /// Its store.
    pub store: &'static str,
    /// What the store is asked for.
    pub reference: String,
    /// Why it can't be read now, when it can't.
    pub missing: Option<String>,
}

/// Hands the secrets to the tool that brings the target up: Compose's environment, or a
/// Kubernetes Secret applied first. Nothing to do without secrets.
pub fn deliver(p: &Prepared, values: &Values, cmd: &mut Command) -> Res<()> {
    if values.is_empty() {
        return Ok(());
    }
    match p.target {
        Target::Docker | Target::Hosted => {
            cmd.envs(values.iter().map(|(name, v)| (secret_variable(name), v.expose())));
            Ok(())
        }
        Target::Kubernetes => kubernetes(p, values),
        // The VM targets: written on each machine over its own connection (`vms`).
        Target::Vagrant | Target::DockerVm | Target::Hybrid | Target::Proxmox | Target::Esxi | Target::CloudVm | Target::CloudDocker | Target::External => {
            Ok(())
        }
        // Their backends refuse a spec with secrets.
        Target::CloudServices => Err(Failure::new(ErrorCode::Unsupported, format!("`{}` doesn't deliver secrets", p.target.id())).into()),
    }
}

/// The namespace and the `isoloom-secrets` Secret, applied server-side from standard input.
fn kubernetes(p: &Prepared, values: &Values) -> Res<()> {
    let at = Scope::step(Some(p.target.id()), "secrets");
    output::begin(&at, None);
    let mut cmd = process::command("kubectl");
    cmd.args(["apply", "--server-side", "--field-manager=isoloom", "-f", "-"]);
    let doc = secret_list(&kubectl::namespace(&p.plan.name), values);
    let status = process::run_step_input(&mut cmd, Some(doc.to_string().as_bytes()), &at)?;
    if !status.success() {
        return Err(Failure::new(ErrorCode::ToolFailed, format!("kubectl couldn't create the secrets: exited with {status}")).into());
    }
    output::done(&at, None);
    Ok(())
}

/// The namespace (so the Secret has one before the manifests are applied) and the Secret.
fn secret_list(namespace: &str, values: &Values) -> serde_json::Value {
    let data: serde_json::Map<String, serde_json::Value> = values.iter().map(|(k, v)| (k.clone(), v.expose().into())).collect();
    serde_json::json!({
        "apiVersion": "v1",
        "kind": "List",
        "items": [
            { "apiVersion": "v1", "kind": "Namespace", "metadata": { "name": namespace } },
            {
                "apiVersion": "v1",
                "kind": "Secret",
                "metadata": { "name": isoloom_targets::KUBERNETES_SECRETS, "namespace": namespace },
                "type": "Opaque",
                "stringData": data,
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_secret_list_holds_the_namespace_then_the_secret() {
        let values = vec![("db-password".to_string(), test_value("v"))];
        let doc = super::secret_list("isoloom-x", &values);
        assert_eq!(doc["items"][0]["metadata"]["name"], "isoloom-x");
        assert_eq!(doc["items"][1]["metadata"]["name"], "isoloom-secrets");
        assert_eq!(doc["items"][1]["stringData"]["db-password"], "v");
    }

    fn test_value(v: &str) -> isoloom_runtime::secrets::SecretValue {
        let spec =
            isoloom_spec::parse("version: 1\nname: x\nsecrets: { s: { env: V } }\nmachines: { a: { docker: { image: alpine }, secrets: [s] } }\n").unwrap();
        let var = |_: &str| Some(v.to_string());
        let at = isoloom_runtime::secrets::Lookup {
            project: std::path::Path::new("."),
            var: &var,
        };
        isoloom_runtime::secrets::resolve(&spec, &at).unwrap().remove(0).1
    }
}
