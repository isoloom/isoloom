//! The files the engine hands out: every target's generated files, and the project's own (the
//! appliances' configurations and the starter scripts of its provisioning steps).

use isoloom_targets::GeneratedFile;
use isoloom_topology::ProjectFile;
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::plan::compiled;

/// A file: its path and what it holds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct File {
    /// Its path, relative to the project's folder.
    pub path: String,
    /// What it holds.
    pub contents: String,
}

impl From<GeneratedFile> for File {
    fn from(f: GeneratedFile) -> File {
        File {
            path: f.path,
            contents: f.contents,
        }
    }
}

impl From<ProjectFile> for File {
    fn from(f: ProjectFile) -> File {
        File {
            path: f.path,
            contents: f.contents,
        }
    }
}

/// The project's own files, next to isoloom.yml where its paths point: the appliances'
/// configurations and the starter scripts of the machines' provisioning steps. What a program
/// writes with the spec to run it.
///
/// # Errors
///
/// The topology is malformed or doesn't compile.
pub(crate) fn own_files(topology_json: &str) -> Result<Vec<File>> {
    let (_, compiled) = compiled(topology_json)?;
    Ok(compiled.into_parts().2.into_iter().map(File::from).collect())
}
