//! `isoloom import`: an isoloom.yml drafted from a Compose file, Terraform or a Vagrantfile.
//! Each source has its own module; `draft` writes the result and says what to check in it.

mod compose;
mod draft;
mod terraform;
mod vagrant;

use std::path::PathBuf;

use clap::Subcommand;

use super::{Console, Result, Verdict};

#[derive(Debug, Subcommand)]
pub(crate) enum Import {
    /// From Terraform: the `.tf` files of a folder, read statically (variables, locals,
    /// `for_each` and count evaluated); VMs and subnets become the draft, the rest is listed.
    Terraform {
        /// The folder with the .tf files (default: the current folder).
        dir: Option<PathBuf>,
        #[command(flatten)]
        dest: draft::Dest,
    },
    /// From a Vagrantfile: it runs against a stand-in `Vagrant` module (nothing is created), and
    /// the settings it makes become the draft; the rest is listed, with why.
    Vagrant {
        /// The Vagrantfile (default: ./Vagrantfile).
        file: Option<PathBuf>,
        #[command(flatten)]
        dest: draft::Dest,
    },
    /// From a Compose file: what it says that the format can express goes into the draft;
    /// the rest is listed, with why.
    Compose {
        /// The Compose file (default: compose.yaml, compose.yml, docker-compose.yaml or
        /// docker-compose.yml in the current folder).
        file: Option<PathBuf>,
        #[command(flatten)]
        dest: draft::Dest,
    },
}

/// Drafts the spec from the source asked for.
pub(crate) fn run(import: Import, console: &mut Console) -> Result<Verdict> {
    match import {
        Import::Compose { file, dest } => compose::run(file, &dest, console),
        Import::Terraform { dir, dest } => terraform::run(dir, &dest, console),
        Import::Vagrant { file, dest } => vagrant::run(file, &dest, console),
    }
}
