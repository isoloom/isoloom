//! The provider versions every generated module requires: the versions the coverage tables were
//! measured against (scripts/refresh-terraform.sh pins the same ones). `~> X.Y.Z` takes that
//! version's patch releases only, so a module never runs on a provider whose schema the tables
//! haven't seen. Bump a version here and in the script together, then refresh the tables.

use super::module::Provider;

/// AWS.
pub const AWS: Provider = ("aws", "hashicorp/aws", "~> 6.67.0");
/// Azure.
pub const AZURERM: Provider = ("azurerm", "hashicorp/azurerm", "~> 5.8.0");
/// Google Cloud.
pub const GOOGLE: Provider = ("google", "hashicorp/google", "~> 8.5.0");
/// DigitalOcean.
pub const DIGITALOCEAN: Provider = ("digitalocean", "digitalocean/digitalocean", "~> 2.103.0");
/// Linode.
pub const LINODE: Provider = ("linode", "linode/linode", "~> 4.7.0");
/// Oracle Cloud.
pub const OCI: Provider = ("oci", "oracle/oci", "~> 9.8.0");
/// Proxmox.
pub const PROXMOX: Provider = ("proxmox", "bpg/proxmox", "~> 0.115.0");
/// Keys: the controller's.
pub const TLS: Provider = ("tls", "hashicorp/tls", "~> 4.4.1");
/// Passwords: the Windows machines' and Linode's root ones.
pub const RANDOM: Provider = ("random", "hashicorp/random", "~> 3.9.1");
/// Files: the project's exclusion list for the cloud modules' archive.
pub const LOCAL: Provider = ("local", "hashicorp/local", "~> 2.9.1");
/// Waits: a Windows machine's restart after its renaming.
pub const TIME: Provider = ("time", "hashicorp/time", "~> 0.14.2");
