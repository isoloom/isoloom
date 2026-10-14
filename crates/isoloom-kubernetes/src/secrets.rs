//! Secrets on Kubernetes: one Secret, `isoloom-secrets`, created in the namespace by
//! `isoloom run` from the secrets' stores (never written in the manifests), and each machine's
//! secrets mounted from it as files, one key at its path.

use std::fmt::Write as _;

use isoloom_plan::Environment;
use isoloom_plan::secrets::DEFAULT_MODE;

use crate::machine::Machine;
use crate::objects::{KeyToPath, Volume, VolumeMount, VolumeSource};

/// The Secret holding the secrets' values, one key per secret.
pub const SECRETS: &str = "isoloom-secrets";
/// The pod volume a machine's secrets are mounted from.
const VOLUME: &str = "isoloom-secrets";

/// The pod's volume of the machine's secrets, when it gets any: their keys of the Secret, each
/// with its mode.
pub(crate) fn volume(m: &Machine) -> Option<Volume> {
    if m.plan.secrets.is_empty() {
        return None;
    }
    let items = m
        .plan
        .secrets
        .iter()
        .map(|s| KeyToPath {
            key: s.name.clone(),
            path: s.name.clone(),
            mode: Some(s.mode.unwrap_or(DEFAULT_MODE)),
        })
        .collect();
    Some(Volume {
        name: VOLUME.into(),
        source: VolumeSource::Secret {
            secret_name: SECRETS.into(),
            items,
        },
    })
}

/// Where the machine sees each secret: its key alone, at the file's path, read-only.
pub(crate) fn mounts(m: &Machine) -> Vec<VolumeMount> {
    m.plan
        .secrets
        .iter()
        .map(|s| VolumeMount {
            sub_path: Some(s.name.clone()),
            ..VolumeMount::read_only(VOLUME, &s.path)
        })
        .collect()
}

/// The usage line saying how the Secret comes to be, when a machine gets secrets.
pub(crate) fn usage(env: &Environment, namespace: &str) -> Option<String> {
    let names: Vec<&String> = env.spec().secrets.keys().collect();
    if env.machines.values().all(|m| m.secrets.is_empty()) || names.is_empty() {
        return None;
    }
    let mut out = format!(
        "# Secrets: `isoloom run kubernetes` creates {SECRETS} from their stores; by hand, once the namespace exists (the pods wait for it):\n#   kubectl -n {namespace} create secret generic {SECRETS}"
    );
    for n in names {
        let _ = write!(out, " --from-file={n}=<file>");
    }
    out.push('\n');
    Some(out)
}
