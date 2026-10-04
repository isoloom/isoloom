//! Every key of the Compose file format (compose-spec, vendored in `coverage/compose-spec.json`)
//! and whether an Isoloom spec can produce it.

use super::{Format, Support, Support::*};

const HOST_TUNING: Support = ByDesign {
    why: "host-level tuning: no meaning for the same machine as a VM",
};
const RUNTIME_KNOB: Support = Open {
    note: "a container-runtime knob with no VM equivalent; would need a field under `docker:`",
};
const IMAGE: Support = InImage {
    note: "set it in the image (`docker.build`)",
};
const TOOLING: Support = Tooling {
    note: "Compose tooling, not the environment's behavior",
};
const LEGACY: Support = Tooling {
    note: "legacy; replaced by networks",
};

pub fn format() -> Format {
    Format {
        name: "Docker Compose",
        file: ".isoloom/docker/compose.yml",
        source: "compose-spec schema, commit 914ec15d1fa4 (crates/isoloom-core/coverage/compose-spec.json)",
        rows: rows(),
    }
}

fn rows() -> Vec<(&'static str, Support)> {
    vec![
        // Top level.
        ("version", ByDesign { why: "obsolete in Compose" }),
        ("name", Emitted { from: "name" }),
        ("include", TOOLING),
        ("services", Emitted { from: "machines" }),
        ("networks", Emitted { from: "networks" }),
        (
            "volumes",
            Planned {
                note: "persistent or shared data: no concept in the format yet",
            },
        ),
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
        ("services.*.blkio_config", HOST_TUNING),
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
                gap: "only NET_ADMIN, for gateways and Isoloom's own router",
            },
        ),
        ("services.*.cap_drop", RUNTIME_KNOB),
        ("services.*.cgroup", HOST_TUNING),
        ("services.*.cgroup_parent", HOST_TUNING),
        ("services.*.command", IMAGE),
        ("services.*.configs", IMAGE),
        (
            "services.*.container_name",
            ByDesign {
                why: "names come from the environment; the hostname is the machine's name",
            },
        ),
        ("services.*.cpu_count", HOST_TUNING),
        ("services.*.cpu_percent", HOST_TUNING),
        ("services.*.cpu_period", HOST_TUNING),
        ("services.*.cpu_quota", HOST_TUNING),
        ("services.*.cpu_rt_period", HOST_TUNING),
        ("services.*.cpu_rt_runtime", HOST_TUNING),
        ("services.*.cpu_shares", HOST_TUNING),
        (
            "services.*.cpus",
            Equivalent {
                via: "machines.*.resources.cpus, written as deploy.resources.limits",
            },
        ),
        ("services.*.cpuset", HOST_TUNING),
        (
            "services.*.credential_spec",
            ByDesign {
                why: "Windows containers only",
            },
        ),
        ("services.*.depends_on", Emitted { from: "machines.*.depends_on" }),
        (
            "services.*.deploy",
            Partial {
                from: "machines.*.resources",
                gap: "only CPU and memory limits",
            },
        ),
        ("services.*.develop", TOOLING),
        ("services.*.device_cgroup_rules", HOST_TUNING),
        ("services.*.devices", RUNTIME_KNOB),
        (
            "services.*.dns",
            Planned {
                note: "a DNS concept for networks (servers, search domains)",
            },
        ),
        ("services.*.dns_opt", Planned { note: "with DNS" }),
        ("services.*.dns_search", Planned { note: "with DNS" }),
        ("services.*.domainname", Planned { note: "with DNS" }),
        (
            "services.*.entrypoint",
            Partial {
                from: "(written for Isoloom's own containers and init jobs)",
                gap: "a machine's own: set it in its image",
            },
        ),
        ("services.*.env_file", Equivalent { via: "inputs" }),
        (
            "services.*.environment",
            Partial {
                from: "machines.*.inputs",
                gap: "only values given at launch; fixed values belong in the image",
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
        ("services.*.gpus", RUNTIME_KNOB),
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
        ("services.*.init", HOST_TUNING),
        ("services.*.ipc", RUNTIME_KNOB),
        (
            "services.*.isolation",
            ByDesign {
                why: "Windows containers only",
            },
        ),
        ("services.*.label_file", TOOLING),
        ("services.*.labels", TOOLING),
        ("services.*.links", LEGACY),
        ("services.*.logging", HOST_TUNING),
        (
            "services.*.mac_address",
            ByDesign {
                why: "addresses are fixed at the IP level, the same on every target",
            },
        ),
        (
            "services.*.mem_limit",
            Equivalent {
                via: "machines.*.resources.memory_mb, written as deploy.resources.limits",
            },
        ),
        ("services.*.mem_reservation", HOST_TUNING),
        ("services.*.mem_swappiness", HOST_TUNING),
        ("services.*.memswap_limit", HOST_TUNING),
        ("services.*.models", TOOLING),
        (
            "services.*.network_mode",
            Partial {
                from: "(internal)",
                gap: "used by Isoloom's sidecars and check runner; a machine can't share another's network",
            },
        ),
        (
            "services.*.networks",
            Emitted {
                from: "machines.*.networks (fixed addresses)",
            },
        ),
        ("services.*.oom_kill_disable", HOST_TUNING),
        ("services.*.oom_score_adj", HOST_TUNING),
        ("services.*.pid", RUNTIME_KNOB),
        ("services.*.pids_limit", HOST_TUNING),
        (
            "services.*.platform",
            Planned {
                note: "a CPU architecture field (amd64, arm64), for VMs too",
            },
        ),
        (
            "services.*.ports",
            ByDesign {
                why: "nothing is published on the host: the environment is reached from its own networks",
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
        ("services.*.privileged", RUNTIME_KNOB),
        (
            "services.*.profiles",
            Emitted {
                from: "checks (a `check` profile)",
            },
        ),
        ("services.*.provider", TOOLING),
        ("services.*.pull_policy", TOOLING),
        ("services.*.pull_refresh_after", TOOLING),
        ("services.*.read_only", RUNTIME_KNOB),
        (
            "services.*.restart",
            Emitted {
                from: "(always unless-stopped: machines stay up like VMs)",
            },
        ),
        ("services.*.runtime", RUNTIME_KNOB),
        (
            "services.*.scale",
            ByDesign {
                why: "machines are individuals with their own addresses",
            },
        ),
        ("services.*.secrets", Equivalent { via: "machines.*.inputs" }),
        ("services.*.security_opt", RUNTIME_KNOB),
        ("services.*.shm_size", RUNTIME_KNOB),
        ("services.*.stdin_open", TOOLING),
        ("services.*.stop_grace_period", TOOLING),
        ("services.*.stop_signal", IMAGE),
        ("services.*.storage_opt", HOST_TUNING),
        (
            "services.*.sysctls",
            Partial {
                from: "networks.*.gateway",
                gap: "only forwarding, for gateways and Isoloom's own router",
            },
        ),
        ("services.*.tmpfs", RUNTIME_KNOB),
        ("services.*.tty", TOOLING),
        ("services.*.ulimits", RUNTIME_KNOB),
        (
            "services.*.use_api_socket",
            ByDesign {
                why: "would hand the host's Docker to a machine",
            },
        ),
        ("services.*.user", IMAGE),
        ("services.*.userns_mode", RUNTIME_KNOB),
        ("services.*.uts", RUNTIME_KNOB),
        (
            "services.*.volumes",
            Partial {
                from: "(read-only mounts of init and check scripts)",
                gap: "persistent or shared data: no concept in the format yet",
            },
        ),
        ("services.*.volumes_from", Planned { note: "with volumes" }),
        ("services.*.working_dir", IMAGE),
        // Networks.
        ("networks.*.attachable", TOOLING),
        (
            "networks.*.driver",
            ByDesign {
                why: "always a bridge: the same layer-2 network as on VM targets",
            },
        ),
        ("networks.*.driver_opts", HOST_TUNING),
        (
            "networks.*.enable_ipv4",
            ByDesign {
                why: "networks are IPv4 (the default)",
            },
        ),
        ("networks.*.enable_ipv6", Planned { note: "IPv6 networks" }),
        (
            "networks.*.external",
            ByDesign {
                why: "an environment is self-contained",
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
            ByDesign {
                why: "names are scoped to the environment by Compose",
            },
        ),
        // Volumes.
        ("volumes.*.driver", Planned { note: "with volumes" }),
        ("volumes.*.driver_opts", Planned { note: "with volumes" }),
        (
            "volumes.*.external",
            ByDesign {
                why: "an environment is self-contained",
            },
        ),
        ("volumes.*.labels", TOOLING),
        (
            "volumes.*.name",
            ByDesign {
                why: "names are scoped to the environment by Compose",
            },
        ),
    ]
}
