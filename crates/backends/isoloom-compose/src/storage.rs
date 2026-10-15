//! A container's storage in Compose: its volumes (all of one, or the folder of a shared one it
//! takes), its data disks (a named volume each: a container shares the host's disk), its tmpfs
//! mounts with a size or a mode (the long form), and the named volumes the file declares, kept
//! past `down` or existing already.

use indexmap::IndexMap;
use isoloom_plan::{Environment, Lifetime, MachinePlan, MachineShape};
use isoloom_spec::model::Machine;

use crate::docker::UTILITY_IMAGE;
use crate::file::{LongMount, Mount, Service, TmpfsOptions, Volume, VolumeOptions};

/// The label of a volume `isoloom down` keeps (`down --volumes` removes it too).
pub const PERSISTENT_LABEL: &str = "isoloom.persistent";

/// The one-shot service creating the folders machines take of shared volumes.
pub const FOLDERS_JOB: &str = "isoloom-volume-folders";

/// A machine's volumes and data disks, as its service mounts them.
pub fn volume_mounts(mp: &MachinePlan) -> Vec<Mount> {
    let volumes = mp.volumes.iter().map(|v| match &v.subpath {
        Some(sub) => Mount::Long(LongMount {
            kind: "volume",
            source: Some(v.key.clone()),
            target: v.path.clone(),
            volume: Some(VolumeOptions { subpath: sub.clone() }),
            tmpfs: None,
        }),
        None => Mount::Short(format!("{}:{}", v.key, v.path)),
    });
    let disks = mp.disks.iter().filter_map(|d| Some(Mount::Short(format!("{}:{}", d.key, d.mount.as_ref()?))));
    volumes.chain(disks).collect()
}

/// A machine's tmpfs mounts: the plain ones (Compose's `tmpfs:`), and the ones with a size or a
/// mode (long-form mounts).
pub fn tmpfs(m: &Machine) -> (Vec<String>, Vec<Mount>) {
    let mut plain = Vec::new();
    let mut long = Vec::new();
    for t in &m.tmpfs {
        if t.size().is_none() && t.mode().is_none() {
            plain.push(t.path().to_string());
            continue;
        }
        long.push(Mount::Long(LongMount {
            kind: "tmpfs",
            source: None,
            target: t.path().to_string(),
            volume: None,
            tmpfs: Some(TmpfsOptions {
                size: t.size().map(String::from),
                // Validated: three or four octal digits.
                mode: t.mode().and_then(|m| u32::from_str_radix(m, 8).ok()),
            }),
        }));
    }
    (plain, long)
}

/// The named volumes of the containers: each machine's (shared ones once), then its disks'. A
/// persistent one is labelled for `down` to keep; an existing one is only named.
pub fn declared(env: &Environment) -> IndexMap<String, Volume> {
    let mut out = IndexMap::new();
    for m in env.machines.values().filter(|m| m.shape == MachineShape::Container) {
        for v in &m.volumes {
            let vol = match &v.lifetime {
                Lifetime::Environment => Volume::default(),
                Lifetime::Persistent => Volume {
                    labels: IndexMap::from([(PERSISTENT_LABEL.to_string(), "true".to_string())]),
                    ..Volume::default()
                },
                Lifetime::External { name } => Volume {
                    external: true,
                    name: Some(name.clone()),
                    ..Volume::default()
                },
            };
            out.entry(v.key.clone()).or_insert(vol);
        }
        out.extend(m.disks.iter().filter(|d| d.mount.is_some()).map(|d| (d.key.clone(), Volume::default())));
    }
    out
}

/// The one-shot job creating, in each shared volume, the folders machines take of it: started
/// once the volume's owner is (Docker seeds an empty volume from the owner's image first), and
/// waited for by the machines taking a folder (Docker mounts only a folder that exists).
pub fn folders(env: &Environment, services: &mut IndexMap<String, Service>) {
    let mut cmds = Vec::new();
    let mut mounts = Vec::new();
    let mut owners = Vec::new();
    for (name, m) in env.machines.iter().filter(|(_, m)| m.shape == MachineShape::Container) {
        for v in &m.volumes {
            let (Some(sub), isoloom_plan::VolumeKind::Shared { owner, .. }) = (&v.subpath, &v.kind) else {
                continue;
            };
            let at = format!("/volumes/{}", v.key);
            if !mounts.iter().any(|x: &String| x.ends_with(&format!(":{at}"))) {
                mounts.push(format!("{}:{at}", v.key));
            }
            cmds.push(format!("mkdir -p {}", isoloom_spec::shell::quote(&format!("{at}/{sub}"))));
            owners.push(owner.clone());
            if let Some(svc) = services.get_mut(name) {
                svc.after(FOLDERS_JOB, "service_completed_successfully");
            }
        }
    }
    if cmds.is_empty() {
        return;
    }
    let mut job = Service {
        image: Some(env.images.pull(UTILITY_IMAGE)),
        entrypoint: vec!["sh".into(), "-c".into(), cmds.join(" && ")],
        volumes: mounts.into_iter().map(Mount::Short).collect(),
        network_mode: Some("none".into()),
        restart: Some("no".into()),
        ..Service::default()
    };
    for o in owners.into_iter().filter(|o| services.contains_key(o)) {
        job.after(o, "service_started");
    }
    services.insert(FOLDERS_JOB.to_string(), job);
}

/// What `isoloom down` removes of a Compose file's named volumes when some are kept: the Docker
/// names of the others (`<project>_<key>`, or their `name`), the existing ones left out. `None`
/// when no volume is kept: `down -v` then removes them all. `project` is the project the file
/// runs under, when not its own `name`.
pub fn removable_volumes(compose_yaml: &str, project: Option<&str>) -> Option<Vec<String>> {
    let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(compose_yaml).ok()?;
    let volumes = doc.get("volumes")?.as_mapping()?;
    let kept = |v: &serde_yaml_ng::Value| v.get("labels").and_then(|l| l.get(PERSISTENT_LABEL)).is_some();
    if !volumes.values().any(kept) {
        return None;
    }
    let project = project.or_else(|| doc.get("name")?.as_str())?;
    let external = |v: &serde_yaml_ng::Value| v.get("external").and_then(serde_yaml_ng::Value::as_bool) == Some(true);
    let names = volumes
        .iter()
        .filter(|(_, v)| !kept(v) && !external(v))
        .filter_map(|(k, v)| match v.get("name").and_then(serde_yaml_ng::Value::as_str) {
            Some(n) => Some(n.to_string()),
            None => Some(format!("{project}_{}", k.as_str()?)),
        })
        .collect();
    Some(names)
}

#[cfg(test)]
mod tests {
    use super::removable_volumes;

    #[test]
    fn down_removes_every_volume_but_the_kept_and_the_existing_ones() {
        let yaml =
            "name: lab\nvolumes:\n  db-data:\n    labels: { isoloom.persistent: 'true' }\n  web-cache: {}\n  shared:\n    external: true\n    name: theirs\n";
        assert_eq!(removable_volumes(yaml, None), Some(vec!["lab_web-cache".to_string()]));
        assert_eq!(removable_volumes(yaml, Some("mine")), Some(vec!["mine_web-cache".to_string()]));
        assert_eq!(removable_volumes("name: lab\nvolumes: { a: {} }\n", None), None);
    }
}
