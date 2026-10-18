//! A machine's storage on Kubernetes: the claims of the volumes it holds (an existing one is
//! only named, a persistent one labelled for `down` to keep and annotated for Helm to keep), a
//! claim per data disk (`volumeMode: Filesystem` mounted at its path, `Block` a device), how its
//! containers mount them (a folder of a shared volume through `subPath`), and its sized tmpfs.

use isoloom_plan::{DataDisk, Lifetime, Volume as PlanVolume};

use crate::machine::Machine;
use crate::objects::{ClaimResources, ClaimSpec, EmptyDir, Labels, Manifest, Volume, VolumeDevice, VolumeMount, VolumeSource};

/// The label of the objects `isoloom down` keeps (`down --volumes` removes them too): the claims
/// of persistent volumes, and the namespace holding them.
pub(crate) const PERSISTENT_LABEL: &str = "isoloom.persistent";

/// What keeps a claim when its Helm release is uninstalled.
const HELM_KEEP: (&str, &str) = ("helm.sh/resource-policy", "keep");

/// The size a volume's claim asks for.
const VOLUME_SIZE: &str = "1Gi";

/// The claims of the volumes `m` holds (an existing one has its own), then of its disks.
pub(crate) fn claims(m: &Machine) -> Vec<Manifest> {
    let volumes = m.plan.volumes.iter().filter(|v| m.holds(&v.kind)).filter_map(|v| {
        let mut c = match v.lifetime {
            Lifetime::External { .. } => return None,
            _ => Manifest::claim(&v.key, spec(VOLUME_SIZE.into(), None)),
        };
        if v.lifetime == Lifetime::Persistent {
            c.metadata.labels.insert(PERSISTENT_LABEL.into(), "true".into());
            c.metadata.annotations.insert(HELM_KEEP.0.into(), HELM_KEEP.1.into());
        }
        Some(c)
    });
    let disks = m.plan.disks.iter().map(|d| {
        let mode = if d.is_raw() { "Block" } else { "Filesystem" };
        Manifest::claim(&d.key, spec(format!("{}Gi", d.size_gb), Some(mode)))
    });
    volumes.chain(disks).collect()
}

fn spec(size: String, volume_mode: Option<&'static str>) -> ClaimSpec {
    ClaimSpec {
        access_modes: vec!["ReadWriteOnce"],
        volume_mode,
        resources: ClaimResources {
            requests: [("storage", size)].into_iter().collect(),
        },
    }
}

/// The labels of the environment's namespace: kept by `down` when it holds a persistent claim.
pub(crate) fn namespace_labels(machines: &[Machine]) -> Labels {
    let persistent = machines.iter().flat_map(|m| &m.plan.volumes).any(|v| v.lifetime == Lifetime::Persistent);
    if persistent {
        Labels::from([(PERSISTENT_LABEL.to_owned(), "true".to_owned())])
    } else {
        Labels::new()
    }
}

/// The pod's name for a volume.
fn volume_name(v: &PlanVolume) -> String {
    format!("vol-{}", v.name)
}

/// The pod's name for a disk.
fn disk_name(d: &DataDisk) -> String {
    format!("disk-{}", d.name)
}

/// The pod's claims: its volumes' (an existing one by its own name), then its disks'.
pub(crate) fn pod_volumes(m: &Machine) -> Vec<Volume> {
    let volumes = m.plan.volumes.iter().map(|v| {
        let claim_name = match &v.lifetime {
            Lifetime::External { name } => name.clone(),
            _ => v.key.clone(),
        };
        Volume {
            name: volume_name(v),
            source: VolumeSource::PersistentVolumeClaim { claim_name },
        }
    });
    let disks = m.plan.disks.iter().map(|d| Volume {
        name: disk_name(d),
        source: VolumeSource::PersistentVolumeClaim { claim_name: d.key.clone() },
    });
    volumes.chain(disks).collect()
}

/// Where the machine's containers see its volumes (the folder it takes of a shared one) and its
/// formatted disks.
pub(crate) fn mounts(m: &Machine) -> Vec<VolumeMount> {
    let volumes = m.plan.volumes.iter().map(|v| VolumeMount {
        sub_path: v.subpath.clone(),
        ..VolumeMount::at(volume_name(v), &v.path)
    });
    let disks = m.plan.disks.iter().filter_map(|d| Some(VolumeMount::at(disk_name(d), d.mount.as_deref()?)));
    volumes.chain(disks).collect()
}

/// Where the machine's container sees its raw disks: `/dev/isoloom-<disk>`.
pub(crate) fn devices(m: &Machine) -> Vec<VolumeDevice> {
    m.plan
        .disks
        .iter()
        .filter(|d| d.is_raw())
        .map(|d| VolumeDevice {
            name: disk_name(d),
            device_path: format!("/dev/isoloom-{}", d.name),
        })
        .collect()
}

/// The machine's tmpfs mounts, as memory-backed emptyDirs of the size each asks for.
pub(crate) fn tmpfs(m: &Machine) -> (Vec<VolumeMount>, Vec<Volume>) {
    m.spec
        .tmpfs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let name = format!("tmpfs-{i}");
            let dir = EmptyDir {
                medium: Some("Memory"),
                size_limit: t.size().map(quantity),
            };
            (
                VolumeMount::at(&name, t.path()),
                Volume {
                    name,
                    source: VolumeSource::EmptyDir(dir),
                },
            )
        })
        .unzip()
}

/// A Docker size (`64m`, `1g`, bytes) as a Kubernetes quantity (`64Mi`, `1Gi`, bytes): in
/// Kubernetes `m` is a thousandth.
pub(crate) fn quantity(size: &str) -> String {
    let mut digits = size.chars();
    let unit = match digits.next_back().map(|c| c.to_ascii_lowercase()) {
        Some('k') => "Ki",
        Some('m') => "Mi",
        Some('g') => "Gi",
        _ => return size.to_owned(),
    };
    format!("{}{unit}", digits.as_str())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::quantity;

    #[test]
    fn docker_sizes_become_binary_quantities() {
        assert_eq!(quantity("64m"), "64Mi");
        assert_eq!(quantity("2G"), "2Gi");
        assert_eq!(quantity("512k"), "512Ki");
        assert_eq!(quantity("1048576"), "1048576");
    }

    proptest! {
        #[test]
        fn a_size_keeps_its_number_and_takes_the_binary_unit(n in 0u64..1_000_000, unit in "[kKmMgG]?") {
            let docker = format!("{n}{unit}");
            let binary = match unit.to_ascii_lowercase().as_str() {
                "k" => "Ki",
                "m" => "Mi",
                "g" => "Gi",
                _ => "",
            };
            prop_assert_eq!(quantity(&docker), format!("{n}{binary}"));
        }
    }
}
