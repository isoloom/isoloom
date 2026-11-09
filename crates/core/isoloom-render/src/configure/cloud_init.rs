//! The cloud-init carrier for a machine's recipe: the shell library and a recipe's calls, as one
//! `sh` script, delivered in a `write_files`
//! entry (gzip then base64, `encoding: gz+b64`) and run from `runcmd` at the step's place in the
//! machine's set-up.
//!
//! **Parity by construction.** The script is exactly [`shell::script`]'s output, the same text a
//! Linux VM's shell set-up runs. cloud-init only delivers it compressed and runs it, so it leaves
//! exactly the state the shell renderer leaves. The resource conformance harness proves it by
//! running real cloud-init (`cargo xtask conformance --renderer cloud-init`): cloud-init writes
//! and runs the script, then the shell library applied on top changes nothing.
//!
//! **Why the whole script, not cloud-init's own modules** (design decision 5): cloud-init's
//! `packages:`, `users:` and the rest run in cloud-init's fixed module order, not the recipe's,
//! and before `runcmd` (so before the routes, project, files, secrets and waits the set-up runs
//! there). A recipe step reordered that way would change what the next step sees, and a native
//! module that quietly makes a change can't set the restart mark a later `service` step reads, so
//! a service would not restart when the shell renderer would restart it. The library keeps one
//! implementation, guarded and ordered, that every carrier runs the same way.
//!
//! **Size.** A cloud that caps user data ([`Limit`]) measures the whole document, as it receives
//! it; gzip keeps a recipe's script small. The `cloud-vm` modules refuse a machine whose user data
//! is still over its cloud's cap ([`Limit::fits`]), naming the machine.

use std::io::Write as _;

use flate2::Compression;
use flate2::write::GzEncoder;
use isoloom_plan::configure::Resource;
use isoloom_spec::shell::base64;

use super::{Configured, shell};

/// Where a machine's configure scripts are delivered on it.
pub const DIR: &str = "/var/lib/isoloom/configure";

/// What the ready file calls a configure run (the same label every carrier gives it).
pub const LABEL: &str = "its configuration";

/// A file cloud-init writes before it runs anything (`write_files`): a recipe's script, gzip
/// then base64, which cloud-init decodes with `encoding: gz+b64`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteFile {
    /// Where it is written on the machine.
    pub path: String,
    /// Its mode, as `permissions` wants it (octal text).
    pub permissions: &'static str,
    /// Its content, gzip then base64: a `write_files` entry's `content` with `encoding: gz+b64`.
    pub content: String,
}

/// A machine's `Step::Configure` as cloud-init carries it: the script delivered in a
/// [`WriteFile`], and the `runcmd` that runs it under [`LABEL`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configure {
    /// The script, in `write_files`.
    pub file: WriteFile,
    /// The `runcmd` command that runs it (`sh <path>`).
    pub command: String,
}

/// The `index`-th configure run of machine `c`, its `resources` and `fresh` flag as the shell
/// renderer takes them (see [`shell::script`]): the script compressed into a `write_files` entry
/// and the `runcmd` that runs it. A machine's runs are numbered from 0, in set-up order.
#[must_use]
pub fn configure(c: &Configured<'_>, resources: &[Resource], fresh: bool, index: usize) -> Configure {
    let path = format!("{DIR}/configure-{index}.sh");
    let command = format!("sh {path}");
    let file = WriteFile {
        path,
        permissions: "0700",
        content: gz_b64(&shell::script(c, resources, fresh)),
    };
    Configure { file, command }
}

/// `script` gzip-compressed then base64-encoded, for a `write_files` entry's `content` with
/// `encoding: gz+b64`. Deterministic: the gzip header carries no modification time or name, so
/// the same script gives the same bytes on every host.
#[must_use]
pub fn gz_b64(script: &str) -> String {
    base64(&gzip(script))
}

/// `script` gzip-compressed, deterministically (no modification time or name in the header).
#[must_use]
pub fn gzip(script: &str) -> Vec<u8> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::best());
    // Writing to and flushing an in-memory `Vec` can't fail; the empty fallback is unreachable.
    let written = gz.write_all(script.as_bytes());
    written.and(gz.finish()).unwrap_or_default()
}

/// How large a cloud lets a machine's user data be, in bytes of the document it receives once
/// decoded from the base64 it travels in (a `docker-vm`, Proxmox or ESXi snippet has no such cap).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Limit {
    /// AWS: 16 KiB of user data, counted before its base64 (so, for gzipped user data, the
    /// gzip's bytes).
    /// <https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/user-data.html>
    Aws,
    /// Azure: 65535 bytes of custom data, once decoded from its base64.
    /// <https://learn.microsoft.com/rest/api/compute/virtual-machines/create-or-update#osprofile>
    Azure,
    /// Google Cloud: 256 KiB for one metadata value (the startup script).
    /// <https://cloud.google.com/compute/docs/metadata/setting-custom-metadata#limitations>
    Gcp,
}

impl Limit {
    /// The cap in bytes.
    #[must_use]
    pub fn bytes(self) -> usize {
        match self {
            Limit::Aws => 16 * 1024,
            Limit::Azure => 65_535,
            Limit::Gcp => 256 * 1024,
        }
    }

    /// The provider's name, for a refusal.
    #[must_use]
    pub fn provider(self) -> &'static str {
        match self {
            Limit::Aws => "AWS",
            Limit::Azure => "Azure",
            Limit::Gcp => "Google Cloud",
        }
    }

    /// `Ok` when `document` (the user data as the cloud counts it: gzipped when it is sent
    /// gzipped, never base64) fits; otherwise [`TooBig`], which a caller turns into a refusal
    /// naming the machine.
    ///
    /// # Errors
    ///
    /// [`TooBig`] when the document is over the cap, with its size and the cap.
    pub fn fits(self, document: &[u8]) -> Result<(), TooBig> {
        let size = document.len();
        let cap = self.bytes();
        if size <= cap {
            Ok(())
        } else {
            Err(TooBig {
                provider: self.provider(),
                size,
                cap,
            })
        }
    }
}

/// A machine's user data doesn't fit its cloud's cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("its user data is {size} bytes as {provider} counts it, over its {cap}-byte limit: fewer or smaller steps, files or recipes")]
#[non_exhaustive]
pub struct TooBig {
    /// The provider whose cap it broke.
    pub provider: &'static str,
    /// The document's size, in bytes.
    pub size: usize,
    /// The cap, in bytes.
    pub cap: usize,
}

#[cfg(test)]
mod tests;
