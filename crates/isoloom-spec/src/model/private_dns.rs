//! The environment's own DNS zone (`dns:` at the top level): the machines' names and `aliases`
//! served under a private domain by the platform's DNS, where it has one.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The environment's names in a private DNS zone (see [`super::Spec::dns`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentDns {
    /// Serve every machine's name and `aliases` under `domain` (`web.<domain>`) from the
    /// platform's own private DNS: a Route 53 private hosted zone on AWS, an Azure private DNS
    /// zone, a Cloud DNS private zone on Google Cloud, a private view on Oracle Cloud, Docker's
    /// DNS on the Docker targets. Where the platform has none (Vagrant, Proxmox, DigitalOcean,
    /// Linode), the hosts file every VM gets carries the same names. Off by default: a cloud
    /// zone is billed, and the short names already resolve through the hosts file everywhere.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub private: bool,
    /// The private zone's domain (`lab.internal`). By default `<name>.internal`, after the
    /// environment's name. Needs `private: true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
}

impl EnvironmentDns {
    /// The private zone's domain for environment `name`, when private DNS is on.
    pub fn private_domain(&self, name: &str) -> Option<String> {
        self.private.then(|| self.domain.clone().unwrap_or_else(|| format!("{name}.internal")))
    }
}
