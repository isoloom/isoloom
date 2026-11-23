//! Secrets on Kubernetes: one Secret, `isoloom-secrets`, created in the namespace from the
//! secrets' stores (`kubectl create secret`, never written in the manifests), and each machine's
//! secrets mounted from it as files, one key at its path.
//!
//! A Secret volume's files belong to root: the fields that would give them an owner
//! (`defaultUser`, `KeyToPath.user`) are alpha, behind a feature gate managed clusters don't
//! open. So a secret with a `uid` or `gid` is copied by the pod's `secrets` init container
//! (Isoloom's busybox, as root) into a memory-backed `emptyDir` with its owner and mode, and
//! mounted from there: like the Secret's own files, it never touches the node's disk.

use isoloom_plan::Environment;
use isoloom_plan::secrets::{DEFAULT_MODE, SecretFile};
use isoloom_render::container::PROBE_IMAGE;

use crate::machine::Machine;
use crate::objects::{Container, EmptyDir, Image, KeyToPath, Volume, VolumeMount, VolumeSource};

/// The Secret holding the secrets' values, one key per secret.
pub(crate) const SECRETS: &str = "isoloom-secrets";
/// The pod volume a machine's secrets are mounted from.
const VOLUME: &str = "isoloom-secrets";
/// The memory-backed `emptyDir` holding the secrets with an owner.
const OWNED_VOLUME: &str = "isoloom-secrets-owned";
/// The init container filling [`OWNED_VOLUME`].
const OWNED_CONTAINER: &str = "isoloom-secrets-owned";
/// Where the init container sees the Secret's files and the `emptyDir`.
const DIR: &str = "/isoloom-secrets";

/// Whether secret `s` has an owner of its own, so it is copied out of the Secret volume.
fn owned(s: &SecretFile) -> bool {
    s.uid.is_some() || s.gid.is_some()
}

/// The pod's volumes of the machine's secrets, when it gets any: their keys of the Secret, each
/// with its mode, and the `emptyDir` of those with an owner.
pub(crate) fn volumes(m: &Machine) -> Vec<Volume> {
    let mut out: Vec<Volume> = volume(m).into_iter().collect();
    if m.plan.secrets.iter().any(owned) {
        out.push(Volume {
            name: OWNED_VOLUME.into(),
            source: VolumeSource::EmptyDir(EmptyDir {
                medium: Some("Memory"),
                size_limit: None,
            }),
        });
    }
    out
}

/// The init container copying each secret with an owner into the `emptyDir`, with its owner and
/// mode; `None` when none has one.
pub(crate) fn init_container(m: &Machine) -> Option<Container> {
    let script: Vec<String> = m
        .plan
        .secrets
        .iter()
        .filter(|s| owned(s))
        .map(|s| {
            let mode = format!("{:04o}", s.mode.unwrap_or(DEFAULT_MODE));
            crate::files::owned_copy(&format!("{DIR}/in/{}", s.name), &format!("{DIR}/out/{}", s.name), s.uid, s.gid, mode)
        })
        .collect();
    if script.is_empty() {
        return None;
    }
    let mut c = Container::new(OWNED_CONTAINER, Image::Tool(m.env.images.pull(PROBE_IMAGE)));
    c.command = vec!["sh".into(), "-c".into(), script.join(" && ")];
    // As the image's own user: the secrets' files are written for the machine that reads them.
    c.security_context = Some(crate::security::image_user());
    c.volume_mounts = vec![
        VolumeMount::read_only(VOLUME, format!("{DIR}/in")),
        VolumeMount::at(OWNED_VOLUME, format!("{DIR}/out")),
    ];
    Some(c)
}

/// The pod's volume of the machine's secrets, when it gets any.
fn volume(m: &Machine) -> Option<Volume> {
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

/// Where the machine sees each secret: its key alone (its copy, for one with an owner), at the
/// file's path, read-only.
pub(crate) fn mounts(m: &Machine) -> Vec<VolumeMount> {
    m.plan
        .secrets
        .iter()
        .map(|s| VolumeMount {
            sub_path: Some(s.name.clone()),
            ..VolumeMount::read_only(if owned(s) { OWNED_VOLUME } else { VOLUME }, &s.path)
        })
        .collect()
}

/// The usage line saying how the Secret comes to be, when a machine gets secrets.
pub(crate) fn usage(env: &Environment, namespace: &str) -> Option<String> {
    let names: Vec<&String> = env.secrets.keys().collect();
    if env.machines.values().all(|m| m.secrets.is_empty()) || names.is_empty() {
        return None;
    }
    let files: Vec<String> = names.iter().map(|n| format!(" --from-file={n}=<file>")).collect();
    Some(format!(
        "# Secrets: create {SECRETS} from their stores once the namespace exists (the pods wait for it):\n#   kubectl -n {namespace} create secret generic {SECRETS}{}\n",
        files.concat()
    ))
}
