//! Why a target's Terraform isn't rendered: the target refuses the environment, or a module
//! can't be written. A backend reports either as the rendering layer's [`RenderError`].

use isoloom_render::Error as RenderError;
use isoloom_render::hcl_builder;
use isoloom_spec::model::Target;

/// Why a target's Terraform isn't rendered.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The target can't run the environment: a feature it lacks, with why.
    #[error("{}: {reason}", target.id())]
    Refused {
        /// The target that refuses.
        target: Target,
        /// Why, as the refusal reads (`machine `web`: ...`).
        reason: String,
    },
    /// The HCL builder couldn't write a module (its formatting is documented as infallible; the
    /// error is kept typed rather than assumed away).
    #[error("{}: the {folder} module couldn't be written", target.id())]
    Module {
        /// The target whose module it is.
        target: Target,
        /// The module's folder under `.isoloom/` (`cloud-vm/aws`).
        folder: String,
        /// What the builder reported.
        #[source]
        source: hcl_builder::Error,
    },
    /// The run manifest breaks its contract (a bug: the plan checked what it is made of).
    #[error("{}: the run manifest: {source}", target.id())]
    Manifest {
        /// The target whose manifest it is.
        target: Target,
        /// What the manifest's types refused.
        #[source]
        source: isoloom_manifest::Error,
    },
}

/// A result whose error is this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<Error> for RenderError {
    /// The error as a backend reports it: an unsupported environment for the target, its reason
    /// in full (the builder's own message after the module's).
    fn from(e: Error) -> RenderError {
        let (target, what) = match e {
            Error::Refused { target, reason } => (target, reason),
            Error::Module { target, folder, source } => (target, format!("the {folder} module couldn't be written: {source}")),
            Error::Manifest { target, source } => (target, format!("the run manifest: {source}")),
        };
        RenderError::Unsupported { target, what }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_reads_as_the_backend_reports_it() {
        let e = Error::Refused {
            target: Target::Proxmox,
            reason: "machine `w`: no image".to_owned(),
        };
        assert_eq!(e.to_string(), "proxmox: machine `w`: no image");
        assert_eq!(RenderError::from(e).to_string(), "proxmox: machine `w`: no image");
    }
}
