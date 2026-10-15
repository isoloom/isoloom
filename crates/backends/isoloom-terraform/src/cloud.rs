//! The clouds: the [`Cloud`]s the modules run on, what every cloud module shares (the
//! operator's variables, the environment's instance, the auto-stop), the targets on them
//! (`cloud-vm`, `cloud-docker`, `cloud-services`), and each cloud's own folder, its pieces for
//! both `cloud-vm` and `cloud-docker`.

pub(crate) mod auto_stop;
pub(crate) mod aws;
pub(crate) mod azure;
pub(crate) mod digitalocean;
pub(crate) mod docker;
pub(crate) mod gcp;
pub(crate) mod instance;
pub(crate) mod linode;
pub(crate) mod oci;
pub(crate) mod operator;
pub(crate) mod services;
pub(crate) mod vm;

use std::fmt;

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
