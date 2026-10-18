//! The clouds: the [`Cloud`]s the modules run on, what every cloud module shares (its head: the
//! operator's variables, the provider, the environment's instance; the auto-stop), the targets
//! on them (`cloud-vm`, `cloud-docker`, `cloud-services`), and each cloud's own folder: its
//! pieces for both targets, and its driver of each (`<cloud>::vm::DRIVER`,
//! `<cloud>::docker::DRIVER`), which holds only what the cloud does its own way.

pub(crate) mod auto_stop;
pub(crate) mod aws;
pub(crate) mod azure;
pub(crate) mod digitalocean;
pub(crate) mod docker;
pub(crate) mod gcp;
pub(crate) mod instance;
pub(crate) mod linode;
pub(crate) mod manifest;
pub(crate) mod oci;
pub(crate) mod operator;
pub(crate) mod services;
pub(crate) mod vm;

use std::fmt;

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Expression, Module};

use crate::module::ProjectCopy;
use crate::module::variable::Variable;

/// A cloud the `cloud-vm` and `cloud-docker` modules run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Cloud {
    /// Amazon Web Services.
    Aws,
    /// Microsoft Azure.
    Azure,
    /// Google Cloud.
    Gcp,
    /// DigitalOcean.
    DigitalOcean,
    /// Linode (Akamai).
    Linode,
    /// Oracle Cloud Infrastructure.
    Oci,
}

impl Cloud {
    /// Every cloud, in the order the modules are written: AWS first.
    pub const ALL: [Cloud; 6] = [Cloud::Aws, Cloud::Azure, Cloud::Gcp, Cloud::DigitalOcean, Cloud::Linode, Cloud::Oci];

    /// Its id: its module's folder under the target's (`.isoloom/cloud-vm/aws/`), as a
    /// refusal names it.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Cloud::Aws => "aws",
            Cloud::Azure => "azure",
            Cloud::Gcp => "gcp",
            Cloud::DigitalOcean => "digitalocean",
            Cloud::Linode => "linode",
            Cloud::Oci => "oci",
        }
    }

    /// Its name, as a sentence says it (`Google Cloud`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Cloud::Aws => "AWS",
            Cloud::Azure => "Azure",
            Cloud::Gcp => "Google Cloud",
            Cloud::DigitalOcean => "DigitalOcean",
            Cloud::Linode => "Linode",
            Cloud::Oci => "Oracle Cloud",
        }
    }
}

/// A cloud's provider in its modules: writes the provider's configuration, and gives the locals
/// it adds to the environment's (`instance::id_and_locals`).
pub(crate) type ProviderConfig = fn(&Environment, &mut Module) -> Vec<(&'static str, Expression)>;

/// A cloud module's start after its `terraform` block, the same on every cloud and target: the
/// cloud's `variables`, the operator's, then its provider and the environment's id and locals.
pub(crate) fn head(env: &Environment, m: &mut Module, variables: Vec<Variable>, provider: ProviderConfig, copy: ProjectCopy) {
    m.blocks(variables.into_iter().chain(operator::host_reaching()).map(Variable::block));
    let locals = provider(env, m);
    m.blocks(instance::id_and_locals(env, locals, copy != ProjectCopy::Runner));
}

impl fmt::Display for Cloud {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cloud_has_a_folder_and_a_name() {
        let ids: Vec<&str> = Cloud::ALL.iter().map(|c| c.id()).collect();
        assert_eq!(ids, ["aws", "azure", "gcp", "digitalocean", "linode", "oci"]);
        assert_eq!(Cloud::Gcp.to_string(), "Google Cloud");
    }
}
