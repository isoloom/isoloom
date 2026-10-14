//! Service keys from `annotations` to `cpuset`, `build` included.

use super::{BUILD, CLOUD_SIZES, FILE_OWNER, FILES, NO_CONTAINER_ON_VMS, TOOLING, WINDOWS_CONTAINERS};
use crate::Support::{self, Emitted, Equivalent, NotPortable, Tooling};

/// Service keys from `annotations` to `cpuset`.
pub(super) const SERVICES_A_C: &[(&str, Support)] = &[
    ("services.*.annotations", TOOLING),
    ("services.*.attach", TOOLING),
    ("services.*.blkio_config", CLOUD_SIZES),
    ("services.*.blkio_config.device_read_bps", CLOUD_SIZES),
    ("services.*.blkio_config.device_read_bps[].path", CLOUD_SIZES),
    ("services.*.blkio_config.device_read_bps[].rate", CLOUD_SIZES),
    ("services.*.blkio_config.device_read_iops", CLOUD_SIZES),
    ("services.*.blkio_config.device_read_iops[].path", CLOUD_SIZES),
    ("services.*.blkio_config.device_read_iops[].rate", CLOUD_SIZES),
    ("services.*.blkio_config.device_write_bps", CLOUD_SIZES),
    ("services.*.blkio_config.device_write_bps[].path", CLOUD_SIZES),
    ("services.*.blkio_config.device_write_bps[].rate", CLOUD_SIZES),
    ("services.*.blkio_config.device_write_iops", CLOUD_SIZES),
    ("services.*.blkio_config.device_write_iops[].path", CLOUD_SIZES),
    ("services.*.blkio_config.device_write_iops[].rate", CLOUD_SIZES),
    ("services.*.blkio_config.weight", CLOUD_SIZES),
    ("services.*.blkio_config.weight_device", CLOUD_SIZES),
    ("services.*.blkio_config.weight_device[].path", CLOUD_SIZES),
    ("services.*.blkio_config.weight_device[].weight", CLOUD_SIZES),
    (
        "services.*.build",
        Emitted {
            from: "machines.*.docker.build",
        },
    ),
    (
        "services.*.build.context",
        Emitted {
            from: "machines.*.docker.build",
        },
    ),
    (
        "services.*.build.dockerfile",
        Emitted {
            from: "machines.*.docker.dockerfile",
        },
    ),
    (
        "services.*.build.dockerfile_inline",
        Equivalent {
            via: "machines.*.docker.dockerfile (a file beside the spec)",
        },
    ),
    ("services.*.build.entitlements", BUILD),
    (
        "services.*.build.args",
        Emitted {
            from: "machines.*.docker.args",
        },
    ),
    ("services.*.build.ssh", BUILD),
    (
        "services.*.build.labels",
        Emitted {
            from: "(the environment's labels: isoloom.managed and isoloom.environment, for `isoloom gc`)",
        },
    ),
    ("services.*.build.cache_from", BUILD),
    ("services.*.build.cache_to", BUILD),
    ("services.*.build.no_cache", BUILD),
    ("services.*.build.no_cache_filter", BUILD),
    ("services.*.build.additional_contexts", BUILD),
    ("services.*.build.network", BUILD),
    ("services.*.build.provenance", BUILD),
    ("services.*.build.sbom", BUILD),
    ("services.*.build.pull", BUILD),
    (
        "services.*.build.target",
        Emitted {
            from: "machines.*.docker.target (a stage of the Dockerfile)",
        },
    ),
    ("services.*.build.shm_size", BUILD),
    ("services.*.build.extra_hosts", BUILD),
    ("services.*.build.isolation", BUILD),
    ("services.*.build.privileged", BUILD),
    ("services.*.build.secrets", BUILD),
    ("services.*.build.secrets[].source", BUILD),
    ("services.*.build.secrets[].target", BUILD),
    ("services.*.build.secrets[].uid", BUILD),
    ("services.*.build.secrets[].gid", BUILD),
    ("services.*.build.secrets[].mode", BUILD),
    ("services.*.build.tags", BUILD),
    ("services.*.build.ulimits", BUILD),
    ("services.*.build.ulimits.*.hard", BUILD),
    ("services.*.build.ulimits.*.soft", BUILD),
    (
        "services.*.build.platforms",
        Equivalent {
            via: "platform (from machines.*.arch)",
        },
    ),
    (
        "services.*.cap_add",
        Emitted {
            from: "networks.*.gateway (capabilities only exist for containers: root on a VM has them all)",
        },
    ),
    ("services.*.cap_drop", NO_CONTAINER_ON_VMS),
    ("services.*.cgroup", NO_CONTAINER_ON_VMS),
    ("services.*.cgroup_parent", NO_CONTAINER_ON_VMS),
    (
        "services.*.command",
        Emitted {
            from: "machines.*.command (and Isoloom's tools: tools.*.command)",
        },
    ),
    ("services.*.configs", FILES),
    ("services.*.configs[].source", FILES),
    (
        "services.*.configs[].target",
        Emitted {
            from: "machines.*.files[].path",
        },
    ),
    ("services.*.configs[].uid", FILE_OWNER),
    ("services.*.configs[].gid", FILE_OWNER),
    ("services.*.configs[].mode", FILE_OWNER),
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
    ("services.*.credential_spec.config", WINDOWS_CONTAINERS),
    ("services.*.credential_spec.file", WINDOWS_CONTAINERS),
    ("services.*.credential_spec.registry", WINDOWS_CONTAINERS),
    ("services.*.depends_on", Emitted { from: "machines.*.depends_on" }),
    (
        "services.*.depends_on.*.restart",
        Tooling {
            note: "Compose's restart of dependents on `docker compose restart`",
        },
    ),
    (
        "services.*.depends_on.*.required",
        Tooling {
            note: "lets Compose start without a dependency: Isoloom's are always required",
        },
    ),
    (
        "services.*.depends_on.*.condition",
        Emitted {
            from: "machines.*.depends_on (healthy, or an init job completed)",
        },
    ),
];
