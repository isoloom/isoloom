//! Service keys from `deploy` to `hostname`, `deploy` and `healthcheck` included.

use super::{CLOUD_SIZES, DEVICES, GPUS, GPUS_KEY, IMAGE, LEGACY, NO_CONTAINER_ON_VMS, RESERVED, STAY_UP, SWARM, TOOLING};
use crate::Support::{self, Emitted, Equivalent, Tooling};

/// Service keys from `deploy` to `hostname`.
pub(super) const SERVICES_D_H: &[(&str, Support)] = &[
    (
        "services.*.deploy",
        Emitted {
            from: "machines.*.resources (replicas: see `scale`)",
        },
    ),
    ("services.*.deploy.mode", SWARM),
    ("services.*.deploy.endpoint_mode", SWARM),
    (
        "services.*.deploy.replicas",
        Equivalent {
            via: "machines.*.count (identical machines, each with its own name and address)",
        },
    ),
    ("services.*.deploy.labels", SWARM),
    ("services.*.deploy.rollback_config", SWARM),
    ("services.*.deploy.rollback_config.parallelism", SWARM),
    ("services.*.deploy.rollback_config.delay", SWARM),
    ("services.*.deploy.rollback_config.failure_action", SWARM),
    ("services.*.deploy.rollback_config.monitor", SWARM),
    ("services.*.deploy.rollback_config.max_failure_ratio", SWARM),
    ("services.*.deploy.rollback_config.order", SWARM),
    ("services.*.deploy.update_config", SWARM),
    ("services.*.deploy.update_config.parallelism", SWARM),
    ("services.*.deploy.update_config.delay", SWARM),
    ("services.*.deploy.update_config.failure_action", SWARM),
    ("services.*.deploy.update_config.monitor", SWARM),
    ("services.*.deploy.update_config.max_failure_ratio", SWARM),
    ("services.*.deploy.update_config.order", SWARM),
    ("services.*.deploy.resources", Emitted { from: "machines.*.resources" }),
    ("services.*.deploy.resources.limits", Emitted { from: "machines.*.resources" }),
    (
        "services.*.deploy.resources.limits.cpus",
        Emitted {
            from: "machines.*.resources.cpus",
        },
    ),
    (
        "services.*.deploy.resources.limits.memory",
        Emitted {
            from: "machines.*.resources.memory_mb",
        },
    ),
    ("services.*.deploy.resources.limits.pids", NO_CONTAINER_ON_VMS),
    ("services.*.deploy.resources.reservations", RESERVED),
    ("services.*.deploy.resources.reservations.cpus", CLOUD_SIZES),
    ("services.*.deploy.resources.reservations.memory", CLOUD_SIZES),
    ("services.*.deploy.resources.reservations.generic_resources", SWARM),
    ("services.*.deploy.resources.reservations.generic_resources[].discrete_resource_spec", SWARM),
    (
        "services.*.deploy.resources.reservations.generic_resources[].discrete_resource_spec.kind",
        SWARM,
    ),
    (
        "services.*.deploy.resources.reservations.generic_resources[].discrete_resource_spec.value",
        SWARM,
    ),
    ("services.*.deploy.resources.reservations.devices", GPUS),
    ("services.*.deploy.resources.reservations.devices[].capabilities", GPUS),
    ("services.*.deploy.resources.reservations.devices[].count", GPUS),
    ("services.*.deploy.resources.reservations.devices[].device_ids", GPUS),
    ("services.*.deploy.resources.reservations.devices[].driver", GPUS),
    ("services.*.deploy.resources.reservations.devices[].options", GPUS),
    ("services.*.deploy.restart_policy", STAY_UP),
    ("services.*.deploy.restart_policy.condition", STAY_UP),
    ("services.*.deploy.restart_policy.delay", STAY_UP),
    ("services.*.deploy.restart_policy.max_attempts", STAY_UP),
    ("services.*.deploy.restart_policy.window", STAY_UP),
    ("services.*.deploy.placement", SWARM),
    ("services.*.deploy.placement.constraints", SWARM),
    ("services.*.deploy.placement.preferences", SWARM),
    ("services.*.deploy.placement.preferences[].spread", SWARM),
    ("services.*.deploy.placement.max_replicas_per_node", SWARM),
    ("services.*.develop", TOOLING),
    ("services.*.develop.watch", TOOLING),
    ("services.*.develop.watch[].ignore", TOOLING),
    ("services.*.develop.watch[].include", TOOLING),
    ("services.*.develop.watch[].path", TOOLING),
    ("services.*.develop.watch[].action", TOOLING),
    ("services.*.develop.watch[].target", TOOLING),
    ("services.*.develop.watch[].exec", TOOLING),
    ("services.*.develop.watch[].exec.command", TOOLING),
    ("services.*.develop.watch[].exec.user", TOOLING),
    ("services.*.develop.watch[].exec.privileged", TOOLING),
    ("services.*.develop.watch[].exec.working_dir", TOOLING),
    ("services.*.develop.watch[].exec.environment", TOOLING),
    ("services.*.develop.watch[].initial_sync", TOOLING),
    ("services.*.device_cgroup_rules", NO_CONTAINER_ON_VMS),
    ("services.*.devices", DEVICES),
    ("services.*.devices[].source", DEVICES),
    ("services.*.devices[].target", DEVICES),
    ("services.*.devices[].permissions", DEVICES),
    (
        "services.*.dns",
        Emitted {
            from: "machines.*.dns.servers (and the Kubernetes dnsConfig)",
        },
    ),
    (
        "services.*.dns_opt",
        Emitted {
            from: "machines.*.dns.options (and the Kubernetes dnsConfig)",
        },
    ),
    (
        "services.*.dns_search",
        Emitted {
            from: "machines.*.dns.search (and the Kubernetes dnsConfig)",
        },
    ),
    ("services.*.domainname", Emitted { from: "machines.*.dns.domain" }),
    (
        "services.*.entrypoint",
        Emitted {
            from: "machines.*.before (a shell running the steps, then the command; Isoloom's own containers and init jobs)",
        },
    ),
    ("services.*.env_file", Equivalent { via: "inputs" }),
    ("services.*.env_file[].path", Equivalent { via: "inputs" }),
    ("services.*.env_file[].format", Equivalent { via: "inputs" }),
    ("services.*.env_file[].required", Equivalent { via: "inputs" }),
    (
        "services.*.environment",
        Emitted {
            from: "machines.*.environment and machines.*.inputs",
        },
    ),
    (
        "services.*.expose",
        Equivalent {
            via: "machines.*.ports (every port is reachable on the machine's networks)",
        },
    ),
    ("services.*.extends", TOOLING),
    ("services.*.extends.service", TOOLING),
    ("services.*.extends.file", TOOLING),
    ("services.*.external_links", LEGACY),
    (
        "services.*.extra_hosts",
        Emitted {
            from: "machines.*.networks (names of machines on other networks)",
        },
    ),
    ("services.*.gpus", GPUS_KEY),
    ("services.*.gpus[].capabilities", GPUS_KEY),
    ("services.*.gpus[].count", GPUS_KEY),
    ("services.*.gpus[].device_ids", GPUS_KEY),
    ("services.*.gpus[].driver", GPUS_KEY),
    ("services.*.gpus[].options", GPUS_KEY),
    ("services.*.group_add", IMAGE),
    (
        "services.*.healthcheck",
        Emitted {
            from: "machines.*.ports and machines.*.healthcheck (a probe of every port, and the machine's own check beside it)",
        },
    ),
    (
        "services.*.healthcheck.disable",
        Tooling {
            note: "turns off the image's own check: Isoloom's readiness is its own probe",
        },
    ),
    (
        "services.*.healthcheck.interval",
        Emitted {
            from: "machines.*.healthcheck.interval (else Isoloom's probe pace)",
        },
    ),
    (
        "services.*.healthcheck.retries",
        Emitted {
            from: "machines.*.healthcheck.retries (else Isoloom's probe pace)",
        },
    ),
    (
        "services.*.healthcheck.test",
        Emitted {
            from: "machines.*.healthcheck.test and machines.*.healthcheck.http (after the probe of every port)",
        },
    ),
    (
        "services.*.healthcheck.timeout",
        Emitted {
            from: "machines.*.healthcheck.timeout (else Isoloom's probe pace)",
        },
    ),
    (
        "services.*.healthcheck.start_period",
        Emitted {
            from: "machines.*.healthcheck.start_period (else a grace period for slow starters: appliances and built images)",
        },
    ),
    (
        "services.*.healthcheck.start_interval",
        Equivalent {
            via: "interval (Isoloom probes at one pace)",
        },
    ),
    ("services.*.hostname", Emitted { from: "the machine's name" }),
];
