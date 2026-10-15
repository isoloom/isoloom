//! `isoloom import`: an isoloom.yml drafted from a Compose file, Terraform or a Vagrantfile.
//! Each source has its own module; `draft` writes the result and its notes.

mod compose;
mod draft;
mod terraform;
mod vagrant;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;

use crate::Res;

#[derive(Subcommand)]
pub enum Import {
    /// From a Compose file: what it says that the format can express goes into the draft;
    /// the rest is listed, with why.
    /// From Terraform: the `.tf` files of a folder, read statically (variables, locals,
    /// for_each and count evaluated); VMs and subnets become the draft, the rest is listed.
    Terraform {
        /// The folder with the .tf files (default: the current folder).
        dir: Option<PathBuf>,
        /// Where to write isoloom.yml (default: that folder).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Replace an existing isoloom.yml.
        #[arg(long)]
        force: bool,
        /// Print the draft instead of writing it.
        #[arg(long)]
        stdout: bool,
    },
    /// From a Vagrantfile: it runs against a stand-in `Vagrant` module (nothing is created), and
    /// the settings it makes become the draft; the rest is listed, with why.
    Vagrant {
        /// The Vagrantfile (default: ./Vagrantfile).
        file: Option<PathBuf>,
        /// Where to write isoloom.yml (default: next to the Vagrantfile).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Replace an existing isoloom.yml.
        #[arg(long)]
        force: bool,
        /// Print the draft instead of writing it.
        #[arg(long)]
        stdout: bool,
    },
    Compose {
        /// The Compose file (default: compose.yaml, compose.yml, docker-compose.yaml or
        /// docker-compose.yml in the current folder).
        file: Option<PathBuf>,
        /// Where to write isoloom.yml (default: next to the Compose file).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Replace an existing isoloom.yml.
        #[arg(long)]
        force: bool,
        /// Print the draft instead of writing it.
        #[arg(long)]
        stdout: bool,
    },
}

pub fn run(import: Import) -> Res<ExitCode> {
    match import {
        Import::Compose { file, out, force, stdout } => compose::run(file, &draft::Dest { out, force, stdout }),
        Import::Terraform { dir, out, force, stdout } => terraform::run(dir, &draft::Dest { out, force, stdout }),
        Import::Vagrant { file, out, force, stdout } => vagrant::run(file, &draft::Dest { out, force, stdout }),
    }
}
