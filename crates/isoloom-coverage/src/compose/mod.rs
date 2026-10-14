//! Every key path of the Compose file format (compose-spec, vendored in
//! `coverage/compose-spec.json` and refreshed by `scripts/refresh-compose-schema.sh`) and whether
//! an Isoloom spec produces it. Nested keys have their own rows (`services.*.deploy.resources`,
//! `services.*.networks.*.ipv4_address`, `networks.*.ipam.config[].subnet`): `*` stands for a
//! name the file chooses, `[]` for a list item. The rule: a machine feature belongs in the format
//! when the VM targets can produce it; containers produce what they can (see [`super`] and
//! docs/adr/0006-coverage-classification.md).
//!
//! The table is data, split by section so each file stays readable.

mod services_a_c;
mod services_d_h;
mod services_i_z;
mod top;

use super::{
    Format, Support,
    Support::{Emitted, Equivalent, InImage, NotPortable, Partial, Tooling},
};

/// Keys listed as one row, without the keys below them: a job and a `pre_start` hook are whole
/// service definitions, so their keys are the service rows again.
pub const ONE_ROW: &[&str] = &["jobs", "services.*.pre_start"];

const IMAGE: Support = InImage {
    note: "set it in the image (`docker.build`) or the VM's provisioning",
};
const TOOLING: Support = Tooling {
    note: "Compose tooling, not the environment's behavior",
};
const LEGACY: Support = Tooling {
    note: "legacy; replaced by networks",
};
const LEGACY_NAME: Support = Tooling {
    note: "legacy spelling of `name`",
};
const CLOUD_SIZES: Support = NotPortable {
    why: "cloud VMs only choose a size: no CPU or memory scheduling knobs",
};
const NO_CONTAINER_ON_VMS: Support = Tooling {
    note: "container mechanics: a VM has none; Isoloom sets them itself when a machine needs them",
};
const SHARED_NAMESPACE: Support = NotPortable {
    why: "shares a kernel namespace with the host or another machine: separate VMs can't",
};
const WINDOWS_CONTAINERS: Support = NotPortable {
    why: "Windows containers only",
};
const SWARM: Support = Tooling {
    note: "Swarm's scheduling across hosts: an environment's containers share one",
};
const BUILD: Support = Tooling {
    note: "how the image is built, not the machine it produces",
};
const FILES: Support = Emitted { from: "machines.*.files" };
/// A file's owner and mode: given to a text, refused for a project file Compose mounts as it is.
const FILE_OWNER: Support = Partial {
    from: "machines.*.files[].uid, gid and mode (a file's `content`)",
    gap: "a project file (`source`) is mounted as it is: an owner or mode for it is refused",
};
/// A container's GPUs, as the device request Isoloom writes (refused where the containers run
/// on a VM Isoloom creates, which has no GPU).
const GPUS: Support = Emitted { from: "machines.*.gpus" };
const GPUS_KEY: Support = Equivalent {
    via: "machines.*.gpus, written as `deploy.resources.reservations.devices` (the same request, which every Compose version reads)",
};
const RESERVED: Support = Partial {
    from: "machines.*.gpus (its devices)",
    gap: "cpus and memory: a machine's size is its limit",
};
const IPV6: Support = Support::Emitted {
    from: "networks.*.ipv6 (each machine's address from its octet; none with `addresses: auto`)",
};
/// The host's devices, in the long form (refused where the containers run on a VM Isoloom
/// creates, which has none of the host's).
const DEVICES: Support = Emitted { from: "machines.*.devices" };
const MODEL_RUNNER: Support = NotPortable {
    why: "served by Docker's model runner: VMs and the cloud have none",
};
const SHORT_PORTS: Support = Equivalent {
    via: "the short form Isoloom writes (`host_ip:published:target/protocol`)",
};
const SHORT_VOLUMES: Support = Equivalent {
    via: "the short form Isoloom writes (`source:target[:ro]`)",
};
const STAY_UP: Support = Equivalent {
    via: "restart (always unless-stopped: machines stay up like VMs)",
};

/// The Compose table.
pub fn format() -> Format {
    Format {
        name: "Docker Compose",
        file: ".isoloom/docker/compose.yml",
        source: "the compose-spec schema, commit 914ec15d1fa4 (crates/isoloom-coverage/coverage/compose-spec.json)",
        rows: rows(),
        collapse_not_portable: false,
    }
}

/// Every key path of the format and what Isoloom does with it: the top level, the services
/// (each nested key right after its parent), then what the file declares.
fn rows() -> Vec<(String, Support)> {
    [
        top::TOP,
        services_a_c::SERVICES_A_C,
        services_d_h::SERVICES_D_H,
        services_i_z::SERVICES_I_Z,
        top::DECLARED,
    ]
    .iter()
    .flat_map(|section| section.iter())
    .map(|(k, s)| ((*k).to_string(), *s))
    .collect()
}
