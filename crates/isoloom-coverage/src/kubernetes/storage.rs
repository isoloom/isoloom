//! Volumes: what a pod mounts, and the claims Isoloom's volumes are.

use crate::Support::{self, Emitted, Equivalent, NotPortable, Planned, Tooling};

const PLUGIN: Support = NotPortable {
    why: "one storage system's plugin: no other target has it",
};
const CLUSTER_STORAGE: Support = Tooling {
    note: "the cluster's storage setup: Isoloom's claims take its default",
};
const MOUNT_DETAIL: Support = Tooling {
    note: "a mount detail of the node: Isoloom mounts whole volumes, read-write",
};
const TMPFS: Support = Emitted {
    from: "machines.*.tmpfs and machines.*.shm_size (in memory)",
};

/// What Isoloom does with a field of the volumes' definitions.
pub(super) fn classify(definition: &str, field: &str) -> Option<Support> {
    match definition {
        "Volume" => volume(field),
        "VolumeMount" => mount(field),
        "VolumeDevice" => match field {
            "name" | "devicePath" => Some(Emitted {
                from: "machines.*.disks with `fs: raw` (at /dev/isoloom-<disk>)",
            }),
            _ => None,
        },
        "PersistentVolumeClaimVolumeSource" | "ConfigMapVolumeSource" | "EmptyDirVolumeSource" => source(definition, field),
        "SecretVolumeSource" | "KeyToPath" => secret(definition, field),
        "PersistentVolumeClaimSpec" | "VolumeResourceRequirements" => claim(definition, field),
        _ => None,
    }
}

fn volume(field: &str) -> Option<Support> {
    Some(match field {
        "name" => Emitted {
            from: "volumes (and Isoloom's own: the scripts, the probe)",
        },
        "persistentVolumeClaim" => Emitted {
            from: "machines.*.volumes and machines.*.disks",
        },
        "configMap" => Emitted {
            from: "(the project's scripts the pods run: `init:` jobs, checks)",
        },
        "emptyDir" => Emitted {
            from: "machines.*.tmpfs and machines.*.shm_size (and Isoloom's probe)",
        },
        "secret" => Emitted {
            from: "machines.*.secrets (keys of the `isoloom-secrets` Secret, which `isoloom run` creates)",
        },
        "ephemeral" => Equivalent {
            via: "`emptyDir`, scratch space that goes with the pod",
        },
        "projected" | "downwardAPI" => Tooling {
            note: "the pod's own details and API tokens as files: machines don't read them",
        },
        "hostPath" => NotPortable {
            why: "a folder of the node: cloud VMs share no files with a host",
        },
        "image" => Tooling {
            note: "another image's files as a volume: the machine's image holds its files",
        },
        "gitRepo" => Tooling {
            note: "deprecated by Kubernetes",
        },
        "awsElasticBlockStore"
        | "azureDisk"
        | "azureFile"
        | "cephfs"
        | "cinder"
        | "csi"
        | "fc"
        | "flexVolume"
        | "flocker"
        | "gcePersistentDisk"
        | "glusterfs"
        | "iscsi"
        | "nfs"
        | "photonPersistentDisk"
        | "portworxVolume"
        | "quobyte"
        | "rbd"
        | "scaleIO"
        | "storageos"
        | "vsphereVolume" => PLUGIN,
        _ => return None,
    })
}

fn mount(field: &str) -> Option<Support> {
    Some(match field {
        "name" | "mountPath" => Emitted {
            from: "machines.*.volumes, machines.*.tmpfs and machines.*.shm_size",
        },
        "readOnly" => Emitted {
            from: "(Isoloom's probe, mounted read-only)",
        },
        "subPath" => Emitted {
            from: "machines.*.files and machines.*.secrets (each file mounted at its path), machines.*.volumes.*.subpath (a folder of a shared volume)",
        },
        "recursiveReadOnly" => Equivalent { via: "`readOnly`" },
        "subPathExpr" | "mountPropagation" | "bindMountOptions" => MOUNT_DETAIL,
        _ => return None,
    })
}

fn source(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        ("PersistentVolumeClaimVolumeSource", "claimName") => Emitted {
            from: "volumes (an existing claim by its name: volumes.*.external)",
        },
        ("PersistentVolumeClaimVolumeSource", "readOnly") => Equivalent { via: "the mount's `readOnly`" },
        ("ConfigMapVolumeSource", "name" | "defaultMode") => Emitted {
            from: "(the project's scripts, executable)",
        },
        ("ConfigMapVolumeSource", "items" | "optional" | "defaultUser") | ("EmptyDirVolumeSource", "mode") => MOUNT_DETAIL,
        ("EmptyDirVolumeSource", "medium" | "sizeLimit") => TMPFS,
        _ => return None,
    })
}

/// The secrets' volume: some keys of the Secret, each a file with its mode.
fn secret(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        ("SecretVolumeSource", "secretName") => Emitted {
            from: "secrets (the `isoloom-secrets` Secret)",
        },
        ("SecretVolumeSource", "items") | ("KeyToPath", "key" | "path") => Emitted {
            from: "machines.*.secrets (the machine's own secrets only)",
        },
        ("KeyToPath", "mode") => Emitted {
            from: "machines.*.secrets[].mode (default 0444)",
        },
        ("SecretVolumeSource", "defaultMode") => Equivalent { via: "each key's `mode`" },
        ("SecretVolumeSource", "defaultUser") | ("KeyToPath", "user") => Planned {
            note: "a secret's `uid` on Kubernetes, once the clusters Isoloom targets all have it",
        },
        ("SecretVolumeSource", "optional") => Tooling {
            note: "`isoloom run` creates the Secret before the pods start: a missing secret stops the run instead",
        },
        _ => return None,
    })
}

fn claim(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        ("PersistentVolumeClaimSpec", "accessModes") => Emitted {
            from: "volumes (ReadWriteOnce: a shared volume's machines run on one node)",
        },
        ("PersistentVolumeClaimSpec", "resources") | ("VolumeResourceRequirements", "requests") => Emitted {
            from: "volumes (1Gi each) and machines.*.disks.*.size_gb",
        },
        ("PersistentVolumeClaimSpec", "volumeMode") => Emitted {
            from: "machines.*.disks.*.fs (Block for a raw disk, Filesystem otherwise)",
        },
        ("PersistentVolumeClaimSpec", "dataSource" | "dataSourceRef") => Tooling {
            note: "a claim cloned from a snapshot: snapshots are `isoloom snapshot`'s",
        },
        ("PersistentVolumeClaimSpec", "selector" | "storageClassName" | "volumeAttributesClassName" | "volumeName")
        | ("VolumeResourceRequirements", "limits") => CLUSTER_STORAGE,
        _ => return None,
    })
}
