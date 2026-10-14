//! The runtime's errors: one enum per concern, each naming the file or tool involved.

use std::path::PathBuf;

/// Reading, locking or writing the registry (`~/.isoloom/status.yml`) failed.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// The file exists but can't be read.
    #[error("can't read {}: {source}", path.display())]
    Read {
        /// The registry file.
        path: PathBuf,
        /// What the system said.
        #[source]
        source: std::io::Error,
    },
    /// The file isn't a registry.
    #[error("{}: {source}", path.display())]
    Parse {
        /// The registry file.
        path: PathBuf,
        /// What the YAML parser said.
        #[source]
        source: serde_yaml_ng::Error,
    },
    /// The folder can't be created.
    #[error("can't create {}: {source}", path.display())]
    Create {
        /// The folder.
        path: PathBuf,
        /// What the system said.
        #[source]
        source: std::io::Error,
    },
    /// The file (or its temporary copy) can't be written.
    #[error("can't write {}: {source}", path.display())]
    Write {
        /// The file being written.
        path: PathBuf,
        /// What the system said.
        #[source]
        source: std::io::Error,
    },
    /// The lock file can't be taken.
    #[error("can't lock {}: {source}", path.display())]
    Lock {
        /// The lock file.
        path: PathBuf,
        /// What the system said.
        #[source]
        source: std::io::Error,
    },
    /// The registry can't be written as YAML.
    #[error("{0}")]
    Render(#[from] serde_yaml_ng::Error),
}

/// A report that can't be made.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    /// No report has this name.
    #[error("no report named `{name}`; one of: {known}")]
    Unknown {
        /// The name asked for.
        name: String,
        /// The report names, comma separated.
        known: String,
    },
}

/// A tool run on the host that gave no answer.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    /// The program isn't on the `PATH`.
    #[error("{0} not installed")]
    NotInstalled(String),
    /// The program ran past its time limit and was killed.
    #[error("{program} didn't answer within {secs}s")]
    Timeout {
        /// The program.
        program: String,
        /// The limit, in seconds.
        secs: u64,
    },
    /// Starting or waiting for the program failed.
    #[error("{0}")]
    Io(#[from] std::io::Error),
}
