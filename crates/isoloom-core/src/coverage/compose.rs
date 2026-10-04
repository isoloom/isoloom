//! Every key of the Compose file format (compose-spec, vendored in `coverage/compose-spec.json`)
//! and whether an Isoloom spec produces it. The rule: a key's behavior belongs in the format
//! when every kind of target can produce it (see [`super`]).

use super::{Format, Support, Support::*};

const IMAGE: Support = InImage {
    note: "set it in the image (`docker.build`) or the VM's provisioning",
};
const TOOLING: Support = Tooling {
    note: "Compose tooling, not the environment's behavior",
};
const LEGACY: Support = Tooling {
    note: "legacy; replaced by networks",
};
const CLOUD_SIZES: Support = NotPortable {
    why: "cloud VMs only choose a size: no CPU or memory scheduling knobs",
};
const NO_CONTAINER_ON_VMS: Support = NotPortable {
    why: "a container-runtime setting: a VM has no container around its processes to configure",
};
const SHARED_NAMESPACE: Support = NotPortable {
    why: "shares a kernel namespace with the host or another machine: separate VMs can't",
};
const VOLUMES: Support = Planned {
    note: "persistent or shared data: Docker volumes, VM disks or shares",
};

pub fn format() -> Format {
    Format {
        name: "Docker Compose",
        file: ".isoloom/docker/compose.yml",
        source: "the compose-spec schema, commit 914ec15d1fa4 (crates/isoloom-core/coverage/compose-spec.json)",
        rows: rows(),
    }
}

fn rows() -> Vec<(String, Support)> {
    let rows: Vec<(&str, Support)> = vec![
        // Top level.
        ("version", Tooling { note: "obsolete in Compose" }),
        ("name", Emitted { from: "name" }),
        ("include", TOOLING),
        ("services", Emitted { from: "machines" }),
        ("networks", Emitted { from: "networks" }),
        ("volumes", VOLUMES),
        (
            "secrets",
            Equivalent {
                via: "inputs (values given at launch, never baked into images)",
            },
        ),
        ("configs", IMAGE),
        ("models", TOOLING),
        (
            "jobs",
            Equivalent {
                via: "docker.init (one-shot jobs)",
            },
        ),
        // Services.
        ("services.*.annotations", TOOLING),
        ("services.*.attach", TOOLING),
        ("services.*.blkio_config", CLOUD_SIZES),
        (
            "services.*.build",
            Emitted {
                from: "machines.*.docker.build",
            },
        ),
        (
            "services.*.cap_add",
            Partial {
                from: "networks.*.gateway",
                gap: "nothing more: capabilities only exist for containers (root on a VM has them all)",
            },
        ),
        ("services.*.cap_drop", NO_CONTAINER_ON_VMS),
        ("services.*.cgroup", NO_CONTAINER_ON_VMS),
        ("services.*.cgroup_parent", NO_CONTAINER_ON_VMS),
        ("services.*.command", IMAGE),
        ("services.*.configs", IMAGE),
        ("services.*.container_name", Equivalent { via: "the machine's name" }),
        ("services.*.cpu_count", CLOUD_SIZES),
        ("services.*.cpu_percent", CLOUD_SIZES),
        ("services.*.cpu_period", CLOUD_SIZES),
        ("services.*.cpu_quota", CLOUD_SIZES),
        ("services.*.cpu_rt_period", CLOUD_SIZES),
        ("services.*.cpu_rt_runtime", CLOUD_SIZES),
        ("services.*.cpu_shares", CLOUD_SIZES),
        (
            "services.*.cpus",
            Equivalent {
                via: "machines.*.resources.cpus, written as deploy.resources.limits",
            },
        ),
        ("services.*.cpuset", CLOUD_SIZES),
        (
            "services.*.credential_spec",
            NotPortable {
                why: "Windows containers only",
            },
        ),
        ("services.*.depends_on", Emitted { from: "machines.*.depends_on" }),
        (
            "services.*.deploy",
            Partial {
                from: "machines.*.resources",
                gap: "replicas (see `scale`)",
            },
        ),
        ("services.*.develop", TOOLING),
        ("services.*.device_cgroup_rules", NO_CONTAINER_ON_VMS),
        (
            "services.*.devices",
            NotPortable {
                why: "host devices: hosting services and cloud VMs have none to pass",
            },
        ),
        (
            "services.*.dns",
            Planned {
                note: "DNS servers for a network or a machine",
            },
        ),
        ("services.*.dns_opt", Planned { note: "with DNS" }),
        (
            "services.*.dns_search",
            Planned {
                note: "with DNS (search domains)",
            },
        ),
        (
            "services.*.domainname",
            Planned {
                note: "with DNS (a domain for the environment)",
            },
        ),
        (
            "services.*.entrypoint",
            Partial {
                from: "(Isoloom's own containers and init jobs)",
                gap: "a machine's own: set it in its image",
            },
        ),
        ("services.*.env_file", Equivalent { via: "inputs" }),
        (
            "services.*.environment",
            Partial {
                from: "machines.*.inputs",
                gap: "fixed values: set them in the image or the provisioning",
            },
        ),
        (
            "services.*.expose",
            Equivalent {
                via: "machines.*.services (every port is reachable on the machine's networks)",
            },
        ),
        ("services.*.extends", TOOLING),
        ("services.*.external_links", LEGACY),
        (
            "services.*.extra_hosts",
            Emitted {
                from: "machines.*.networks (names of machines on other networks)",
            },
        ),
        (
            "services.*.gpus",
            NotPortable {
                why: "local VMs can't use the host's GPU",
            },
        ),
        ("services.*.group_add", IMAGE),
        (
            "services.*.healthcheck",
            Emitted {
                from: "machines.*.services (a probe of every port)",
            },
        ),
        ("services.*.hostname", Emitted { from: "the machine's name" }),
        (
            "services.*.image",
            Emitted {
                from: "machines.*.docker.image",
            },
        ),
        (
            "services.*.init",
            NotPortable {
                why: "container-only: a VM always runs its own init",
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
        ("services.*.labels", TOOLING),
        ("services.*.links", LEGACY),
        (
            "services.*.logging",
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
        ("services.*.models", TOOLING),
        (
            "services.*.network_mode",
            Partial {
                from: "(Isoloom's own sidecars and check runner)",
                gap: "nothing more: sharing another machine's network isn't possible between VMs",
            },
        ),
        (
            "services.*.networks",
            Emitted {
                from: "machines.*.networks (fixed addresses)",
            },
        ),
        ("services.*.oom_kill_disable", NO_CONTAINER_ON_VMS),
        ("services.*.oom_score_adj", NO_CONTAINER_ON_VMS),
        ("services.*.pid", SHARED_NAMESPACE),
        ("services.*.pids_limit", NO_CONTAINER_ON_VMS),
        (
            "services.*.platform",
            Planned {
                note: "a CPU architecture (amd64, arm64): images, boxes and instance types all have one",
            },
        ),
        (
            "services.*.ports",
            Planned {
                note: "publishing a service outside the environment: Docker ports, Vagrant forwarded ports, a port forward on the Proxmox router, a public IP in the cloud",
            },
        ),
        (
            "services.*.post_start",
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
        ("services.*.privileged", NO_CONTAINER_ON_VMS),
        (
            "services.*.profiles",
            Emitted {
                from: "checks (a `check` profile)",
            },
        ),
        ("services.*.provider", TOOLING),
        ("services.*.pull_policy", TOOLING),
        ("services.*.pull_refresh_after", TOOLING),
        (
            "services.*.read_only",
            Planned {
                note: "a read-only root filesystem (VMs can mount it read-only too)",
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
            Planned {
                note: "several identical machines (replicas), each with its own address",
            },
        ),
        ("services.*.secrets", Equivalent { via: "machines.*.inputs" }),
        ("services.*.security_opt", NO_CONTAINER_ON_VMS),
        (
            "services.*.shm_size",
            Planned {
                note: "the size of /dev/shm (a mount option on VMs)",
            },
        ),
        ("services.*.stdin_open", TOOLING),
        ("services.*.stop_grace_period", TOOLING),
        ("services.*.stop_signal", IMAGE),
        ("services.*.storage_opt", NO_CONTAINER_ON_VMS),
        (
            "services.*.sysctls",
            Partial {
                from: "networks.*.gateway",
                gap: "a machine's own network sysctls (net.*): containers only allow those, VMs allow them too",
            },
        ),
        (
            "services.*.tmpfs",
            Planned {
                note: "memory-backed mounts (tmpfs works on VMs too)",
            },
        ),
        ("services.*.tty", TOOLING),
        (
            "services.*.ulimits",
            Planned {
                note: "process limits (limits.conf or systemd on VMs)",
            },
        ),
        (
            "services.*.use_api_socket",
            NotPortable {
                why: "hands the host's Docker to a machine: VMs and the cloud have none",
            },
        ),
        ("services.*.user", IMAGE),
        ("services.*.userns_mode", SHARED_NAMESPACE),
        ("services.*.uts", SHARED_NAMESPACE),
        (
            "services.*.volumes",
            Partial {
                from: "(read-only mounts of init and check scripts)",
                gap: "persistent or shared data",
            },
        ),
        (
            "services.*.volumes_from",
            Planned {
                note: "with volumes (data shared between machines)",
            },
        ),
        ("services.*.working_dir", IMAGE),
        // Networks.
        ("networks.*.attachable", TOOLING),
        (
            "networks.*.driver",
            NotPortable {
                why: "Docker network drivers (overlay, macvlan) have no VM equivalent",
            },
        ),
        (
            "networks.*.driver_opts",
            NotPortable {
                why: "Docker network driver options",
            },
        ),
        (
            "networks.*.enable_ipv4",
            Tooling {
                note: "networks are IPv4 (the default)",
            },
        ),
        ("networks.*.enable_ipv6", Planned { note: "IPv6 networks" }),
        (
            "networks.*.external",
            Planned {
                note: "joining a network outside the environment (a Docker network, a host bridge, a VPC)",
            },
        ),
        (
            "networks.*.internal",
            Emitted {
                from: "networks.*.internet: false (when nothing routes)",
            },
        ),
        ("networks.*.ipam", Emitted { from: "networks.*.cidr" }),
        ("networks.*.labels", TOOLING),
        (
            "networks.*.name",
            Tooling {
                note: "Compose scopes names to the environment",
            },
        ),
        // Volumes.
        (
            "volumes.*.driver",
            NotPortable {
                why: "Docker volume drivers have no VM equivalent",
            },
        ),
        (
            "volumes.*.driver_opts",
            NotPortable {
                why: "Docker volume driver options",
            },
        ),
        (
            "volumes.*.external",
            Planned {
                note: "with volumes (data that outlives the environment)",
            },
        ),
        ("volumes.*.labels", TOOLING),
        (
            "volumes.*.name",
            Tooling {
                note: "Compose scopes names to the environment",
            },
        ),
    ];
    rows.into_iter().map(|(k, s)| (k.to_string(), s)).collect()
}
