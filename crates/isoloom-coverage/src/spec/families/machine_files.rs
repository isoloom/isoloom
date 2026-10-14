//! What each rendering does with a machine's `files`, in one place: each family's `PARTS` lists
//! its table here.

use crate::spec::status::PROBE;
use crate::spec::status::Status::{self, Done, Planned};

/// The Compose file's (`docker`, `hosted`, `docker-vm`, `cloud-docker`, the containers of
/// `hybrid`).
pub const COMPOSE: &[(&str, Status)] = &[
    (
        "machines.*.files[].path",
        Done {
            how: "a config mounted at the path",
            proof: PROBE,
        },
    ),
    (
        "machines.*.files[].source",
        Done {
            how: "a `file` config: the project's file, mounted as it is",
            proof: PROBE,
        },
    ),
    (
        "machines.*.files[].content",
        Done {
            how: "a `content` config, copied into the container",
            proof: PROBE,
        },
    ),
    ("machines.*.files[].uid", COMPOSE_OWNER),
    ("machines.*.files[].gid", COMPOSE_OWNER),
    ("machines.*.files[].mode", COMPOSE_OWNER),
];

/// A file's owner and mode on Compose: given to a config Compose copies in, a project file's
/// read into Compose's environment as it starts.
const COMPOSE_OWNER: Status = Done {
    how: "the config's owner and mode: a file's `content`, or its `source` as an `environment` config `isoloom run` reads it into",
    proof: PROBE,
};

/// The Kubernetes manifests and the chart.
pub const KUBERNETES: &[(&str, Status)] = &[
    ("machines.*.files[].path", KUBERNETES_FILE),
    (
        "machines.*.files[].source",
        Done {
            how: "the project's ConfigMap (read from the project at apply time, copied into the chart)",
            proof: PROBE,
        },
    ),
    (
        "machines.*.files[].content",
        Done {
            how: "the machine's own ConfigMap (`<machine>-files`)",
            proof: PROBE,
        },
    ),
    ("machines.*.files[].uid", KUBERNETES_FILE),
    ("machines.*.files[].gid", KUBERNETES_FILE),
    ("machines.*.files[].mode", KUBERNETES_FILE),
];

/// A file on Kubernetes.
const KUBERNETES_FILE: Status = Done {
    how: "copied from a ConfigMap into an emptyDir with its owner and mode by an init container, mounted at the path (`subPath`)",
    proof: PROBE,
};

/// The VM outputs (Vagrant, Proxmox, the clouds): the `files` step of the machine's set-up.
pub const VM: &[(&str, Status)] = &[
    ("machines.*.files[].path", VM_FILE),
    (
        "machines.*.files[].source",
        Done {
            how: "copied from the project unpacked on the machine",
            proof: PROBE,
        },
    ),
    ("machines.*.files[].content", VM_FILE),
    ("machines.*.files[].uid", VM_OWNER),
    ("machines.*.files[].gid", VM_OWNER),
    ("machines.*.files[].mode", VM_OWNER),
];

/// A file on a VM.
const VM_FILE: Status = Done {
    how: "written by the machine's set-up (its `files` step)",
    proof: PROBE,
};

/// A file's owner and mode on a VM.
const VM_OWNER: Status = Done {
    how: "`chown` and `chmod` in the `files` step (Linux; a Windows machine's files have neither)",
    proof: PROBE,
};

/// The `external` output: the backend refuses a machine's files (Isoloom writes nothing on an
/// existing host).
pub const EXTERNAL: &[(&str, Status)] = &[
    ("machines.*.files[].path", EXTERNAL_FILE),
    ("machines.*.files[].source", EXTERNAL_FILE),
    ("machines.*.files[].content", EXTERNAL_FILE),
    ("machines.*.files[].uid", EXTERNAL_FILE),
    ("machines.*.files[].gid", EXTERNAL_FILE),
    ("machines.*.files[].mode", EXTERNAL_FILE),
];

/// A file on an existing host.
const EXTERNAL_FILE: Status = Planned {
    note: "written over SSH by a `provision:` playbook",
};
