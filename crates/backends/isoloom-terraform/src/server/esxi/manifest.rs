//! The `esxi` target's run manifest (`.isoloom/esxi/run.json`), the servers' own
//! (`crate::server::manifest`) with what is ESXi's:
//!
//! - **The runner's private key** (`${run.ssh_key}`), which Terraform sets the machines up with
//!   over SSH through the router.
//! - **Tools:** Terraform, VMware's `ovftool` (the provider deploys each OVA with it, from the
//!   machine that runs Terraform: so Terraform runs from the host, never from an image), and `ssh`.
//! - **Networks** named after their port group on the host.

use isoloom_manifest::{Error as ManifestError, Manifest, Tool};
use isoloom_plan::Environment;
use isoloom_spec::model::Target;

use crate::server::manifest::{self as shared, Server};

/// The run manifest of `env` on ESXi; its message is left to the caller.
pub(super) fn manifest(env: &Environment) -> crate::Result<Manifest> {
    let native = |net: &str| shared::slotted("iso", super::networks::suffix(net));
    let server = Server {
        target: Target::Esxi,
        names: isoloom_manifest::Target::Esxi,
        variables: super::base::variables(env),
        private_key: "ssh_private_key_file",
        tools,
        native: &native,
        applied: "the VMs exist, and Terraform set each Linux one up over SSH through the router",
        // Terraform's `remote-exec` runs the set-up: its output is the apply's own, in no file on
        // the machine (cloud-init there runs the router's set-up alone).
        setup_log: None,
        // None named: no josenk/esxi delete is known to fail on a resource already gone.
        gone: || Ok(Vec::new()),
    };
    shared::manifest(env, &server)
}

/// Terraform, `ovftool` and `ssh`, from the host.
fn tools() -> Result<Vec<Tool>, ManifestError> {
    Ok(vec![
        shared::terraform()?,
        isoloom_render::run_manifest::tool("ovftool", isoloom_render::run_manifest::argv("ovftool", &["--version"])?)?,
        shared::ssh()?,
    ])
}
