//! What the environment needs from where it runs: constraints, a proxy and mirrors. A new key
//! is a minor change, so each is built from [`Default`], then its fields set:
//!
//! ```
//! let mut c = isoloom_spec::model::Constraints::default();
//! c.ntp = vec!["time.corp".to_owned()];
//! ```
//!
//! ```compile_fail
//! let c = isoloom_spec::model::Constraints { ntp: Vec::new(), ..Default::default() };
//! ```
//!
//! ```compile_fail
//! let p = isoloom_spec::model::Proxy { http: None, ..Default::default() };
//! ```
//!
//! ```compile_fail
//! let m = isoloom_spec::model::Mirrors { npm: None, ..Default::default() };
//! ```

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What the network an environment runs in imposes, applied on every target: images pulled
/// through the customer's registry, packages through their mirrors, outbound HTTP through their
/// proxy, time and names from their servers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
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
    /// The resolvers (IP addresses) Isoloom's own machines and a VM naming none of its own ask
    /// names of, where the place forbids a public one: the Proxmox and ESXi VMs, Isoloom's
    /// controller, and the router's resolver forwarding the environment's queries. Without it, the
    /// public `1.1.1.1`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dns: Vec<String>,
    /// Certificate authorities every machine trusts beside the public ones (PEM files in the
    /// project, one certificate each): a TLS-inspecting proxy's, an internal registry's. Each
    /// Linux VM adds them to its trust store (`update-ca-certificates`, `update-ca-trust`) once
    /// the project is in, Isoloom's own too (the containers' Docker host before it installs
    /// Docker, the controller and the router before their packages), each Windows VM to its root
    /// store, each image Isoloom builds at build time, and each container has them at
    /// `/usr/local/share/ca-certificates/isoloom-<n>.crt`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ca_certificates: Vec<String>,
}

/// The resolver a machine asks when neither the spec nor its constraints name one: public, so it
/// still installs software.
const PUBLIC_RESOLVER: &str = "1.1.1.1";

/// The resolvers of `constraints` ([`Constraints::dns`]), else the public `1.1.1.1`.
#[must_use]
pub fn resolvers(constraints: Option<&Constraints>) -> Vec<&str> {
    match constraints.filter(|c| !c.dns.is_empty()) {
        Some(c) => c.dns.iter().map(String::as_str).collect(),
        None => vec![PUBLIC_RESOLVER],
    }
}

impl Constraints {
    /// Where a container has the `n`th of [`Constraints::ca_certificates`]: the trust store's
    /// folder Debian, Ubuntu and Alpine read.
    #[must_use]
    pub fn ca_certificate_path(n: usize) -> String {
        format!("/usr/local/share/ca-certificates/isoloom-{n}.crt")
    }
}

/// An HTTP proxy (see [`Constraints::proxy`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
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
#[non_exhaustive]
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
