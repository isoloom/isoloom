//! The top-level keys, then the networks, volumes, configs, secrets and models a file declares.

use super::{FILES, LEGACY_NAME, MODEL_RUNNER, TOOLING};
use crate::Support::{self, Emitted, Equivalent, NotPortable, Partial, Planned, Tooling};

/// The top-level keys, `include` items included.
pub(super) const TOP: &[(&str, Support)] = &[
    ("version", Tooling { note: "obsolete in Compose" }),
    ("name", Emitted { from: "name" }),
    ("include", TOOLING),
    ("include[].path", TOOLING),
    ("include[].env_file", TOOLING),
    ("include[].project_directory", TOOLING),
    ("services", Emitted { from: "machines" }),
    ("models", MODEL_RUNNER),
    ("networks", Emitted { from: "networks" }),
    ("volumes", Emitted { from: "machines.*.volumes" }),
    ("secrets", Emitted { from: "secrets" }),
    ("configs", FILES),
    (
        "jobs",
        Equivalent {
            via: "docker.init (one-shot jobs)",
        },
    ),
];

/// The keys of each declared network, volume, config, secret and model.
pub(super) const DECLARED: &[(&str, Support)] = &[
    (
        "networks.*.name",
        Emitted {
            from: "(the hybrid target: the Docker network its host VM created for each network; elsewhere Compose scopes names to the environment)",
        },
    ),
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
    ("networks.*.ipam", Emitted { from: "networks.*.cidr" }),
    (
        "networks.*.ipam.driver",
        NotPortable {
            why: "Docker IPAM drivers have no VM equivalent",
        },
    ),
    (
        "networks.*.ipam.config",
        Emitted {
            from: "networks.*.cidr and networks.*.ipv6.cidr",
        },
    ),
    (
        "networks.*.ipam.config[].subnet",
        Emitted {
            from: "networks.*.cidr and networks.*.ipv6.cidr",
        },
    ),
    (
        "networks.*.ipam.config[].ip_range",
        Emitted {
            from: "networks.*.ipv6 (where Docker picks IPv6 addresses, clear of the fixed ones)",
        },
    ),
    (
        "networks.*.ipam.config[].gateway",
        Emitted {
            from: "(the address Docker takes, when the first one is a machine's or the router's)",
        },
    ),
    (
        "networks.*.ipam.config[].aux_addresses",
        Tooling {
            note: "addresses Docker must not hand out: every machine has a fixed one",
        },
    ),
    (
        "networks.*.ipam.options",
        NotPortable {
            why: "Docker IPAM driver options",
        },
    ),
    (
        "networks.*.external",
        Partial {
            from: "(the hybrid target: its containers join the macvlan networks the Docker host VM creates, shared with the VMs)",
            gap: "joining a network outside the environment (a Docker network, a host bridge, a VPC)",
        },
    ),
    ("networks.*.external.name", LEGACY_NAME),
    (
        "networks.*.internal",
        Emitted {
            from: "networks.*.internet: false (when nothing routes)",
        },
    ),
    (
        "networks.*.enable_ipv4",
        Emitted {
            from: "networks.*.ipv6.only (off on an IPv6-only network)",
        },
    ),
    ("networks.*.enable_ipv6", Emitted { from: "networks.*.ipv6" }),
    ("networks.*.attachable", TOOLING),
    (
        "networks.*.labels",
        Emitted {
            from: "(the environment's labels: isoloom.managed and isoloom.environment, for `isoloom gc`)",
        },
    ),
    (
        "volumes.*.name",
        Tooling {
            note: "Compose scopes names to the environment",
        },
    ),
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
    ("volumes.*.external.name", LEGACY_NAME),
    (
        "volumes.*.labels",
        Emitted {
            from: "(the environment's labels: isoloom.managed and isoloom.environment, for `isoloom gc`)",
        },
    ),
    (
        "configs.*.name",
        Tooling {
            note: "Compose scopes names to the environment",
        },
    ),
    ("configs.*.content", FILES),
    ("configs.*.environment", FILES),
    ("configs.*.file", FILES),
    (
        "configs.*.external",
        Tooling {
            note: "a config created outside Compose",
        },
    ),
    ("configs.*.external.name", LEGACY_NAME),
    ("configs.*.labels", TOOLING),
    (
        "configs.*.template_driver",
        Tooling {
            note: "Swarm's templating of configs",
        },
    ),
    (
        "secrets.*.name",
        Tooling {
            note: "Compose scopes names to the environment",
        },
    ),
    (
        "secrets.*.environment",
        Emitted {
            from: "secrets (every store: `isoloom run` reads the value and sets `ISOLOOM_SECRET_<NAME>` for Compose alone)",
        },
    ),
    (
        "secrets.*.file",
        Equivalent {
            via: "secrets.*.file: `isoloom run` reads the file and hands its value to Compose as `environment`",
        },
    ),
    (
        "secrets.*.external",
        Equivalent {
            via: "secrets.*.vault and secrets.*.sops: stores outside the file, read by `isoloom run`",
        },
    ),
    ("secrets.*.external.name", LEGACY_NAME),
    ("secrets.*.labels", TOOLING),
    (
        "secrets.*.driver",
        NotPortable {
            why: "Docker secret drivers have no VM equivalent",
        },
    ),
    (
        "secrets.*.driver_opts",
        NotPortable {
            why: "Docker secret driver options",
        },
    ),
    (
        "secrets.*.template_driver",
        Tooling {
            note: "Swarm's templating of secrets",
        },
    ),
    ("models.*.name", MODEL_RUNNER),
    ("models.*.model", MODEL_RUNNER),
    ("models.*.context_size", MODEL_RUNNER),
    ("models.*.runtime_flags", MODEL_RUNNER),
];
