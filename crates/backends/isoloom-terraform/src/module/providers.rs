//! The providers the modules require, each at the version it was written against, and the
//! `terraform` block requiring them. `~> X.Y.Z` takes that version's patch releases only, so a
//! module never runs on a provider whose schema it wasn't written against.

use isoloom_render::hcl_builder::{Block, object, string};

/// A provider a module requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Provider {
    /// Its local name in the module (`aws`).
    pub name: &'static str,
    /// Its registry source (`hashicorp/aws`).
    pub source: &'static str,
    /// The versions taken.
    pub version: &'static str,
}

/// AWS.
pub(crate) const AWS: Provider = Provider {
    name: "aws",
    source: "hashicorp/aws",
    version: "~> 6.67.0",
};
/// Azure.
pub(crate) const AZURERM: Provider = Provider {
    name: "azurerm",
    source: "hashicorp/azurerm",
    version: "~> 5.8.0",
};
/// Google Cloud.
pub(crate) const GOOGLE: Provider = Provider {
    name: "google",
    source: "hashicorp/google",
    version: "~> 8.5.0",
};
/// DigitalOcean.
pub(crate) const DIGITALOCEAN: Provider = Provider {
    name: "digitalocean",
    source: "digitalocean/digitalocean",
    version: "~> 2.103.0",
};
/// Linode.
pub(crate) const LINODE: Provider = Provider {
    name: "linode",
    source: "linode/linode",
    version: "~> 4.7.0",
};
/// Oracle Cloud.
pub(crate) const OCI: Provider = Provider {
    name: "oci",
    source: "oracle/oci",
    version: "~> 9.8.0",
};
/// Proxmox.
pub(crate) const PROXMOX: Provider = Provider {
    name: "proxmox",
    source: "bpg/proxmox",
    version: "~> 0.115.0",
};
/// Standalone ESXi hosts.
pub(crate) const ESXI: Provider = Provider {
    name: "esxi",
    source: "josenk/esxi",
    version: "~> 1.10.3",
};
/// Keys: the controller's.
pub(crate) const TLS: Provider = Provider {
    name: "tls",
    source: "hashicorp/tls",
    version: "~> 4.4.1",
};
/// Passwords: the Windows machines' and Linode's root ones.
pub(crate) const RANDOM: Provider = Provider {
    name: "random",
    source: "hashicorp/random",
    version: "~> 3.9.1",
};
/// Files: the project's exclusion list for the archive.
pub(crate) const LOCAL: Provider = Provider {
    name: "local",
    source: "hashicorp/local",
    version: "~> 2.9.1",
};
/// Waits: a Windows machine's restart after its renaming.
pub(crate) const TIME: Provider = Provider {
    name: "time",
    source: "hashicorp/time",
    version: "~> 0.14.2",
};

/// The `terraform` block: Terraform 1.6 or later, a local state, the providers.
pub(crate) fn terraform(providers: &[Provider]) -> Block {
    let required = providers.iter().fold(Block::builder("required_providers"), |b, p| {
        b.add_attribute((p.name, object([("source", string(p.source)), ("version", string(p.version))])))
    });
    Block::builder("terraform")
        .add_attribute(("required_version", string(">= 1.6")))
        .add_block(Block::builder("backend").add_label("local").build())
        .add_block(required.build())
        .build()
}
