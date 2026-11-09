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
//! **Why the whole script, not cloud-init's own modules**: cloud-init's
//! `packages:`, `users:` and the rest run in cloud-init's fixed module order, not the recipe's,
//! and before `runcmd` (so before the routes, project, files, secrets and waits the set-up runs
//! there). A recipe step reordered that way would change what the next step sees, and a native
//! module that quietly makes a change can't set the restart mark a later `service` step reads, so
//! a service would not restart when the shell renderer would restart it. The library keeps one
//! implementation, guarded and ordered, that every carrier runs the same way.
//!
//! **Size.** A cloud that caps user data measures the whole document, as it receives it; gzip
//! keeps a recipe's script small. The `cloud-vm` modules (`isoloom-terraform`, which knows each
//! cloud's cap) refuse a machine whose user data is still over its cloud's, naming the machine.

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
pub(crate) fn gz_b64(script: &str) -> String {
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

#[cfg(test)]
mod tests;
