//! A container's storage in Compose: its volumes (all of one, or the folder of a shared one it
//! takes), its data disks (a named volume each: a container shares the host's disk), its tmpfs
//! mounts with a size or a mode (the long form), and the named volumes the file declares, kept
//! past `down` or existing already.

use indexmap::IndexMap;
use isoloom_plan::{Environment, Lifetime, MachinePlan, MachineShape, VolumeKind};
use isoloom_render::container::UTILITY_IMAGE;
use isoloom_spec::model::Machine;
use isoloom_spec::shell;

use crate::file::{Condition, LongMount, Mount, NetworkMode, Restart, Service, TmpfsOptions, Volume, VolumeOptions};
use crate::labels::PERSISTENT_LABEL;

/// The one-shot service creating the folders machines take of shared volumes.
pub(crate) const FOLDERS_JOB: &str = "isoloom-volume-folders";

/// A machine's volumes and data disks, as its service mounts them.
pub(crate) fn volume_mounts(plan: &MachinePlan) -> Vec<Mount> {
    let volumes = plan.volumes.iter().map(|v| match &v.subpath {
        Some(sub) => Mount::Long(LongMount::Volume {
            source: v.key.clone(),
            target: v.path.clone(),
            volume: VolumeOptions { subpath: sub.clone() },
        }),
        None => Mount::Short(format!("{}:{}", v.key, v.path)),
    });
    let disks = plan.disks.iter().filter_map(|d| Some(Mount::Short(format!("{}:{}", d.key, d.mount.as_ref()?))));
    volumes.chain(disks).collect()
}

/// A machine's tmpfs mounts: the plain ones (Compose's `tmpfs:`), and the ones with a size or a
/// mode (long-form mounts).
pub(crate) fn tmpfs(machine: &Machine) -> (Vec<String>, Vec<Mount>) {
    let mut plain = Vec::new();
    let mut long = Vec::new();
    for t in &machine.tmpfs {
        if t.size().is_none() && t.mode().is_none() {
            plain.push(t.path().to_owned());
            continue;
        }
        long.push(Mount::Long(LongMount::Tmpfs {
            target: t.path().to_owned(),
            tmpfs: TmpfsOptions {
                size: t.size().map(String::from),
                // Validated: three or four octal digits.
                mode: t.mode().and_then(|mode| u32::from_str_radix(mode, 8).ok()),
            },
        }));
    }
    (plain, long)
}

/// The named volumes of the containers: each machine's (shared ones once), then its disks'. A
/// persistent one is labelled for `down` to keep; an existing one is only named.
pub(crate) fn declared(env: &Environment) -> IndexMap<String, Volume> {
    let mut out = IndexMap::new();
    for m in env.machines.values().filter(|m| m.shape == MachineShape::Container) {
        for v in &m.volumes {
            let vol = match &v.lifetime {
                Lifetime::Environment => Volume::default(),
                Lifetime::Persistent => Volume {
                    labels: IndexMap::from([(PERSISTENT_LABEL.to_owned(), "true".to_owned())]),
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
pub(crate) fn folders(env: &Environment, services: &mut IndexMap<String, Service>) {
    let mut cmds = Vec::new();
    let mut mounts: Vec<String> = Vec::new();
    let mut owners = Vec::new();
    for (name, m) in env.machines.iter().filter(|(_, m)| m.shape == MachineShape::Container) {
        for v in &m.volumes {
            let (Some(sub), VolumeKind::Shared { owner, .. }) = (&v.subpath, &v.kind) else {
                continue;
            };
            let at = format!("/volumes/{}", v.key);
            if !mounts.iter().any(|x| x.ends_with(&format!(":{at}"))) {
                mounts.push(format!("{}:{at}", v.key));
            }
            cmds.push(format!("mkdir -p {}", shell::quote(&format!("{at}/{sub}"))));
            owners.push(owner.clone());
            if let Some(svc) = services.get_mut(name) {
                svc.after(FOLDERS_JOB, Condition::CompletedSuccessfully);
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
        network_mode: Some(NetworkMode::None),
        restart: Some(Restart::No),
        ..Service::default()
    };
    for o in owners.into_iter().filter(|o| services.contains_key(o)) {
        job.after(o, Condition::Started);
    }
    services.insert(FOLDERS_JOB.to_owned(), job);
}

#[cfg(test)]
mod tests {
    use crate::labels::PERSISTENT_LABEL;
    use crate::testing::environment;

    const SPEC: &str = "version: 1
name: store
networks: { lan: { cidr: 10.9.0.0/24 } }
volumes: { data: { persistent: true } }
machines:
  db: { networks: { lan: 10 }, docker: { image: postgres }, volumes: { data: /var/lib/postgresql/data } }
  web: { networks: { lan: 11 }, docker: { image: nginx }, tmpfs: [/run, { path: /cache, size: 64m, mode: '1777' }] }
";

    #[test]
    fn a_kept_volume_is_labelled_for_down_to_keep() {
        let volumes = super::declared(&environment(SPEC));
        assert_eq!(volumes.len(), 1);
        assert!(volumes.values().all(|v| v.labels.get(PERSISTENT_LABEL).is_some_and(|l| l == "true")));
    }

    #[test]
    fn a_tmpfs_with_a_size_or_a_mode_is_a_long_mount() {
        let env = environment(SPEC);
        let (plain, long) = super::tmpfs(&env.spec().machines["web"]);
        assert_eq!(plain, ["/run"]);
        let yaml = serde_yaml_ng::to_string(&long).expect("serializes");
        assert_eq!(yaml, "- type: tmpfs\n  target: /cache\n  tmpfs:\n    size: 64m\n    mode: 1023\n");
    }
}
