//! Why a spec couldn't be loaded. Every variant says where (the folder, the file, the stage or
//! the field) and why, in a message a user can act on without reading Isoloom's code. The
//! problems of a spec that loads but is wrong are [`crate::validate::Problem`]s, one per field.

use std::path::PathBuf;

/// The step of loading that turned the YAML into the spec the rest of Isoloom reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// `-s key=value` overrides and a profile's `spec:`, merged into the document.
    Overrides,
    /// `count:` clones of a machine.
    Clones,
    /// `common:` and `groups:` folded into the machines.
    Groups,
    /// A machine's several addresses on one network (`lan: [10, 11]`), split from its first.
    Addresses,
    /// VLANs written under their LAN, made networks of their own.
    Vlans,
    /// Each machine's `os` and `vm.os`, resolved to a known OS.
    Os,
}

/// Why a spec couldn't be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The folder has no spec file.
    #[error("no isoloom.yml in {}", .0.display())]
    NotFound(PathBuf),
    /// Both `isoloom.yml` and `isoloom.yaml` exist: which one is meant is ambiguous.
    #[error("both isoloom.yml and isoloom.yaml exist in {}; keep one", .0.display())]
    Ambiguous(PathBuf),
    /// The spec file exists but couldn't be read.
    #[error("can't read {}: {source}", path.display())]
    Read {
        /// The spec file.
        path: PathBuf,
        /// The system's reason.
        #[source]
        source: std::io::Error,
    },
    /// Not YAML, or a key or a value the spec doesn't have: the message carries the field and
    /// its line and column.
    #[error("{0}")]
    Yaml(#[from] serde_yaml_ng::Error),
    /// A normalization step refused the document: the reason names the field.
    #[error("{reason}")]
    Expand {
        /// The step that refused it.
        stage: Stage,
        /// What is wrong, and where.
        reason: String,
    },
    /// The profile's `spec:` doesn't fit the spec: one line per problem, each naming the field.
    #[error("{}", .problems.join("\n"))]
    Profile {
        /// The problems, one per field.
        problems: Vec<String>,
    },
}

impl LoadError {
    /// A closure turning a step's reason into this error, for `map_err`.
    pub(crate) fn at(stage: Stage) -> impl Fn(String) -> LoadError {
        move |reason| LoadError::Expand { stage, reason }
    }
}
