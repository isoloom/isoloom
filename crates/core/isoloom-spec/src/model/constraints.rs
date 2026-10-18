//! What the environment needs from where it runs: constraints, a proxy and mirrors.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What the network an environment runs in imposes, applied on every target: images pulled
/// through the customer's registry, packages through their mirrors, outbound HTTP through their
/// proxy, time from their servers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Constraints {
    /// The HTTP proxy outbound traffic goes through: set for the machines' containers (and their
    /// builds) and on every Linux VM (shell, apt, dnf, Docker), never for Isoloom's own checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<Proxy>,
    /// Registry mirrors, by the registry they stand for: `docker.io: registry.corp/dockerhub`
    /// pulls `redis:7` as `registry.corp/dockerhub/library/redis:7`. Every image of the
    /// environment is rewritten, Isoloom's own included; images built from the project aren't
    /// pulled, so they stay as they are (their `FROM` lines are the Dockerfile's business).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub registries: IndexMap<String, String>,
    /// Package mirrors the machines install from instead of the public archives: apt's for
    /// Debian and Ubuntu VMs, and a Python package index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirrors: Option<Mirrors>,
    /// The time servers (names or addresses) the Linux VMs synchronise their clocks with,
    /// where the place forbids the public pools.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ntp: Vec<String>,
}

/// An HTTP proxy (see [`Constraints::proxy`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Proxy {
    /// The proxy for `http://` URLs, e.g. `http://proxy.corp:3128`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<String>,
    /// The proxy for `https://` URLs (default: `http`'s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub https: Option<String>,
    /// Names, domains (`.corp`) and blocks reached directly. The environment's own machine
    /// names and network blocks, and localhost, are always added.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_proxy: Vec<String>,
}

/// Package mirrors (see [`Constraints::mirrors`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mirrors {
    /// Replaces `deb.debian.org/debian` in the Debian VMs' apt sources (`debian-security`, a
    /// separate archive, is left as it is).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debian: Option<String>,
    /// Replaces `archive.ubuntu.com/ubuntu` (and its country and security hosts) in the Ubuntu
    /// VMs' apt sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ubuntu: Option<String>,
    /// A Python package index: `PIP_INDEX_URL` for the machines, `/etc/pip.conf` on the VMs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pypi: Option<String>,
    /// An npm registry: `NPM_CONFIG_REGISTRY` for the machines, `/etc/npmrc` on the VMs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npm: Option<String>,
}
