//! Service keys from `image` to `working_dir`, long-form `ports` and `volumes` included.

use super::{
    CLOUD_SIZES, IMAGE, IMAGE_MOUNT, IPV6, LEGACY, LONG_VOLUMES, MODEL_RUNNER, NO_CONTAINER_ON_VMS, SHARED_NAMESPACE, SHORT_PORTS, SHORT_VOLUMES, SWARM,
    TOOLING,
};
use crate::Support::{self, Emitted, Equivalent, NotPortable, Tooling};

/// Service keys from `image` to `working_dir`.
pub(super) const SERVICES_I_Z: &[(&str, Support)] = &[
    (
        "services.*.image",
        Emitted {
            from: "machines.*.docker.image",
        },
    ),
    (
        "services.*.init",
        Tooling {
            note: "container mechanics: a VM always runs its own init",
        },
    ),
    ("services.*.ipc", SHARED_NAMESPACE),
    (
        "services.*.isolation",
        NotPortable {
            why: "Windows containers only",
        },
    ),
    ("services.*.label_file", TOOLING),
    (
        "services.*.labels",
        Emitted {
            from: "machines.*.services (isoloom.service.<name>, for tools reading the containers); isoloom.managed and isoloom.environment on every service, for `isoloom gc`",
        },
    ),
    ("services.*.links", LEGACY),
    (
        "services.*.logging",
        Tooling {
            note: "where the runner collects logs",
        },
    ),
    (
        "services.*.logging.driver",
        Tooling {
            note: "where the runner collects logs",
        },
    ),
    (
        "services.*.logging.options",
        Tooling {
            note: "where the runner collects logs",
        },
    ),
    (
        "services.*.mac_address",
        NotPortable {
            why: "cloud VMs get their MAC address from the provider",
        },
    ),
    (
        "services.*.mem_limit",
        Equivalent {
            via: "machines.*.resources.memory_mb, written as deploy.resources.limits",
        },
    ),
    ("services.*.mem_reservation", CLOUD_SIZES),
    ("services.*.mem_swappiness", CLOUD_SIZES),
    ("services.*.memswap_limit", NO_CONTAINER_ON_VMS),
    ("services.*.models", MODEL_RUNNER),
    ("services.*.models.*.endpoint_var", MODEL_RUNNER),
    ("services.*.models.*.model_var", MODEL_RUNNER),
    (
        "services.*.network_mode",
        Emitted {
            from: "(Isoloom's own sidecars and check runner; sharing another machine's network isn't possible between VMs)",
        },
    ),
    (
        "services.*.networks",
        Emitted {
            from: "machines.*.networks (fixed addresses)",
        },
    ),
    ("services.*.networks.*.aliases", Emitted { from: "machines.*.aliases" }),
    (
        "services.*.networks.*.interface_name",
        Emitted {
            from: "(network appliances: each network on the interface the image expects)",
        },
    ),
    (
        "services.*.networks.*.ipv4_address",
        Emitted {
            from: "machines.*.networks (fixed addresses)",
        },
    ),
    ("services.*.networks.*.ipv6_address", IPV6),
    (
        "services.*.networks.*.link_local_ips",
        NotPortable {
            why: "link-local addresses on a container's interface: cloud VMs get theirs from the provider",
        },
    ),
    (
        "services.*.networks.*.mac_address",
        Emitted {
            from: "(VLAN trunks: each end found by its fixed MAC address)",
        },
    ),
    (
        "services.*.networks.*.driver_opts",
        NotPortable {
            why: "Docker network driver options",
        },
    ),
    (
        "services.*.networks.*.priority",
        Emitted {
            from: "(network appliances: the order their interfaces come up in)",
        },
    ),
    (
        "services.*.networks.*.gw_priority",
        Emitted {
            from: "(the hybrid target: the default route through the network shared with the VMs)",
        },
    ),
    ("services.*.oom_kill_disable", NO_CONTAINER_ON_VMS),
    ("services.*.oom_score_adj", NO_CONTAINER_ON_VMS),
    ("services.*.pid", SHARED_NAMESPACE),
    ("services.*.pids_limit", NO_CONTAINER_ON_VMS),
    (
        "services.*.platform",
        Emitted {
            from: "machines.*.arch (pinned on every container, so the machine runs the same on an x86-64 or an ARM host)",
        },
    ),
    (
        "services.*.ports",
        Emitted {
            from: "machines.*.services[].publish (on the host's loopback; `/udp` with `protocol: udp`)",
        },
    ),
    (
        "services.*.ports[].name",
        Tooling {
            note: "names the port for tools",
        },
    ),
    ("services.*.ports[].mode", SWARM),
    ("services.*.ports[].host_ip", SHORT_PORTS),
    ("services.*.ports[].target", SHORT_PORTS),
    ("services.*.ports[].published", SHORT_PORTS),
    ("services.*.ports[].protocol", SHORT_PORTS),
    (
        "services.*.ports[].app_protocol",
        Tooling {
            note: "names the port's protocol for tools",
        },
    ),
    (
        "services.*.post_start",
        Equivalent {
            via: "machines.*.docker.init (runs once the machine answers)",
        },
    ),
    (
        "services.*.post_start[].command",
        Equivalent {
            via: "machines.*.docker.init (runs once the machine answers)",
        },
    ),
    (
        "services.*.post_start[].user",
        Equivalent {
            via: "machines.*.docker.init (runs once the machine answers)",
        },
    ),
    (
        "services.*.post_start[].privileged",
        Equivalent {
            via: "machines.*.docker.init (runs once the machine answers)",
        },
    ),
    (
        "services.*.post_start[].working_dir",
        Equivalent {
            via: "machines.*.docker.init (runs once the machine answers)",
        },
    ),
    (
        "services.*.post_start[].environment",
        Equivalent {
            via: "machines.*.docker.init (runs once the machine answers)",
        },
    ),
    (
        "services.*.pre_start",
        Equivalent {
            via: "machines.*.depends_on and docker.init",
        },
    ),
    ("services.*.pre_stop", TOOLING),
    ("services.*.pre_stop[].command", TOOLING),
    ("services.*.pre_stop[].user", TOOLING),
    ("services.*.pre_stop[].privileged", TOOLING),
    ("services.*.pre_stop[].working_dir", TOOLING),
    ("services.*.pre_stop[].environment", TOOLING),
    (
        "services.*.privileged",
        Emitted {
            from: "machines.*.privileged (the container runs privileged; a VM already has it, its workload is root)",
        },
    ),
    (
        "services.*.profiles",
        Emitted {
            from: "checks (a `check` profile)",
        },
    ),
    ("services.*.provider", TOOLING),
    ("services.*.provider.type", TOOLING),
    ("services.*.provider.options", TOOLING),
    ("services.*.pull_policy", TOOLING),
    ("services.*.pull_refresh_after", TOOLING),
    (
        "services.*.read_only",
        Emitted {
            from: "machines.*.read_only (a read-only root filesystem; `volumes` stay writable)",
        },
    ),
    (
        "services.*.restart",
        Emitted {
            from: "(always unless-stopped: machines stay up like VMs)",
        },
    ),
    ("services.*.runtime", NO_CONTAINER_ON_VMS),
    (
        "services.*.scale",
        Equivalent {
            via: "machines.*.count (identical machines, each with its own name and address)",
        },
    ),
    ("services.*.secrets", Emitted { from: "machines.*.secrets" }),
    (
        "services.*.secrets[].source",
        Emitted {
            from: "machines.*.secrets[].source",
        },
    ),
    (
        "services.*.secrets[].target",
        Emitted {
            from: "machines.*.secrets[].target (default /run/secrets/<name>)",
        },
    ),
    (
        "services.*.secrets[].uid",
        Emitted {
            from: "machines.*.secrets[].uid",
        },
    ),
    (
        "services.*.secrets[].gid",
        Emitted {
            from: "machines.*.secrets[].gid",
        },
    ),
    (
        "services.*.secrets[].mode",
        Emitted {
            from: "machines.*.secrets[].mode",
        },
    ),
    ("services.*.security_opt", NO_CONTAINER_ON_VMS),
    (
        "services.*.shm_size",
        Emitted {
            from: "machines.*.shm_size (and a Memory emptyDir on Kubernetes)",
        },
    ),
    ("services.*.stdin_open", TOOLING),
    ("services.*.stop_grace_period", TOOLING),
    ("services.*.stop_signal", IMAGE),
    ("services.*.storage_opt", NO_CONTAINER_ON_VMS),
    (
        "services.*.sysctls",
        Emitted {
            from: "machines.*.sysctls and networks.*.gateway",
        },
    ),
    (
        "services.*.tmpfs",
        Emitted {
            from: "machines.*.tmpfs (and Memory emptyDirs on Kubernetes)",
        },
    ),
    ("services.*.tty", TOOLING),
    ("services.*.ulimits", Emitted { from: "machines.*.ulimits" }),
    ("services.*.ulimits.*.hard", Emitted { from: "machines.*.ulimits" }),
    ("services.*.ulimits.*.soft", Emitted { from: "machines.*.ulimits" }),
    (
        "services.*.use_api_socket",
        NotPortable {
            why: "hands the host's Docker to a machine: VMs and the cloud have none",
        },
    ),
    (
        "services.*.user",
        Emitted {
            from: "(Isoloom's check runner, as root to drop its default route on offline networks; a machine's own user is its image's)",
        },
    ),
    ("services.*.userns_mode", SHARED_NAMESPACE),
    ("services.*.uts", SHARED_NAMESPACE),
    (
        "services.*.volumes",
        Emitted {
            from: "machines.*.volumes and volumes (shared between machines; seeded with the image's files at the path, Kubernetes included; and read-only mounts of init and check scripts)",
        },
    ),
    ("services.*.volumes[].type", LONG_VOLUMES),
    ("services.*.volumes[].source", LONG_VOLUMES),
    ("services.*.volumes[].target", LONG_VOLUMES),
    ("services.*.volumes[].read_only", SHORT_VOLUMES),
    (
        "services.*.volumes[].consistency",
        Tooling {
            note: "Docker Desktop's file sharing tuning",
        },
    ),
    ("services.*.volumes[].bind", NO_CONTAINER_ON_VMS),
    ("services.*.volumes[].bind.propagation", NO_CONTAINER_ON_VMS),
    (
        "services.*.volumes[].bind.create_host_path",
        Tooling {
            note: "how Docker treats a missing host folder",
        },
    ),
    ("services.*.volumes[].bind.recursive", NO_CONTAINER_ON_VMS),
    ("services.*.volumes[].bind.selinux", NO_CONTAINER_ON_VMS),
    ("services.*.volumes[].volume", LONG_VOLUMES),
    (
        "services.*.volumes[].volume.labels",
        Tooling {
            note: "labels on a volume Compose creates on the fly: Isoloom declares its volumes",
        },
    ),
    (
        "services.*.volumes[].volume.nocopy",
        Tooling {
            note: "skips seeding the volume with the image's files: Isoloom always seeds",
        },
    ),
    (
        "services.*.volumes[].volume.subpath",
        Emitted {
            from: "machines.*.volumes.*.subpath (created first by a one-shot job)",
        },
    ),
    (
        "services.*.volumes[].tmpfs",
        Emitted {
            from: "machines.*.tmpfs[] with a size or a mode",
        },
    ),
    (
        "services.*.volumes[].tmpfs.size",
        Emitted {
            from: "machines.*.tmpfs[].size",
        },
    ),
    (
        "services.*.volumes[].tmpfs.mode",
        Emitted {
            from: "machines.*.tmpfs[].mode",
        },
    ),
    ("services.*.volumes[].image", IMAGE_MOUNT),
    ("services.*.volumes[].image.subpath", IMAGE_MOUNT),
    (
        "services.*.volumes_from",
        Equivalent {
            via: "volumes (a volume shared between machines, each mounting it at its own path)",
        },
    ),
    (
        "services.*.working_dir",
        Emitted {
            from: "machines.*.working_dir",
        },
    ),
];
