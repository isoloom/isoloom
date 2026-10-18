//! The configuration renderers whose output an infrastructure file embeds or builds from (design
//! `plan/phase4/design.md` §2, §3, §6.1): each renders a machine's resolved recipe
//! ([`Recipe`], on the plan) and nothing else.
//!
//! - [`shell`]: the POSIX library and a recipe's calls, for a Linux VM's set-up and every
//!   generated Dockerfile (cloud-init carries the VM's script).
//! - [`cloud_init`]: the same shell script, delivered in a cloud-config `write_files` entry
//!   (gzip then base64) and run from `runcmd`, for the targets that hand a VM its user data.
//! - [`powershell`]: the PowerShell library and a recipe's calls, for a Windows VM's set-up.
//! - [`dockerfile`]: the image a container builds from its recipe, its process Isoloom's
//!   [`runner`] when it runs services.
//! - [`files`]: the files they add to a target's: the Dockerfiles, the libraries, the runners and
//!   the templates filled in for a container's build.
//!
//! Every step is rendered in recipe order, behind its guard, so a second apply changes nothing.

use isoloom_plan::Environment;
use isoloom_plan::configure::{Context, Recipe, Resource, fill};
use isoloom_spec::syntax::ProjectPath;

use crate::GeneratedFile;

pub mod cloud_init;
pub mod dockerfile;
mod password;
pub mod powershell;
pub mod runner;
pub mod shell;
mod units;

pub use units::{linux_unit, systemd_word};

/// Where the configuration renderers' own files go, from the project folder.
pub const DIR: &str = ".isoloom/configure";

/// One machine's recipe as its renderers read it: the environment the machine is planned in
/// (its addresses on the target, which its templates name) and its name.
#[derive(Debug, Clone, Copy)]
pub struct Configured<'a> {
    env: &'a Environment,
    machine: &'a str,
    recipe: &'a Recipe,
}

impl<'a> Configured<'a> {
    /// Machine `machine` of `env`, when it has a recipe.
    #[must_use]
    pub fn of(env: &'a Environment, machine: &'a str) -> Option<Configured<'a>> {
        let recipe = env.machines.get(machine)?.recipe.as_ref()?;
        Some(Configured { env, machine, recipe })
    }

    /// The machine's name.
    #[must_use]
    pub fn machine(&self) -> &'a str {
        self.machine
    }

    /// Its recipe.
    #[must_use]
    pub fn recipe(&self) -> &'a Recipe {
        self.recipe
    }

    /// Template `from` filled in for the machine on its target.
    ///
    /// # Errors
    ///
    /// Why it can't be: its text wasn't read when the files were generated, or a placeholder
    /// names nothing the lab has (`isoloom validate` reports that first), as the line a recipe
    /// fails with.
    pub fn template(&self, from: &ProjectPath) -> Result<String, String> {
        let text = self
            .recipe
            .files
            .get(from)
            .ok_or_else(|| format!("the template {from} wasn't read when the files were generated: run `isoloom generate` in the project folder"))?;
        fill(text, &Context::new(self.env, self.machine)).map_err(|e| format!("the template {from}: {e}"))
    }

    /// User `user`'s literal `password`, hashed as Linux keeps it (the same for the same lab).
    #[must_use]
    pub fn password(&self, user: &str, password: &str) -> String {
        password::hash(&self.env.name, self.machine, user, password)
    }
}

/// A library as a VM's set-up step embeds it: without its blank lines and comment lines (its
/// directives to shellcheck kept), so the step stays small enough for a VM's user data. The
/// generated library file keeps them, for people to read.
#[must_use]
pub(crate) fn compact(library: &str) -> String {
    let mut out = String::with_capacity(library.len());
    for line in library.lines() {
        let t = line.trim_start();
        if !t.is_empty() && (!t.starts_with('#') || t.starts_with("# shellcheck")) {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Where template `from` of machine `machine`, filled in, is generated for its image's build.
#[must_use]
pub fn rendered_path(machine: &str, from: &ProjectPath) -> String {
    format!("{DIR}/{machine}/rendered/{from}")
}

/// The files the configuration renderers add to `env`'s: each container's Dockerfile, with its
/// runner and its filled templates, and the libraries, once, when a recipe uses them (a VM's
/// set-up embeds its library; the file is the same text, which the validators check).
#[must_use]
pub fn files(env: &Environment) -> Vec<GeneratedFile> {
    let mut out = dockerfile::files(env);
    let configured = || env.machines.keys().filter_map(|m| Configured::of(env, m));
    let windows = |c: &Configured<'_>| c.recipe.family == isoloom_plan::configure::Family::Windows;
    if configured().any(|c| !windows(&c) && uses_library(c.recipe)) && !out.iter().any(|f| f.path == shell::LIBRARY_PATH) {
        out.push(shell::library_file());
    }
    if configured().any(|c| windows(&c) && uses_library(c.recipe)) {
        out.push(powershell::library_file());
    }
    out
}

/// Whether a recipe has a step its library runs (not only scripts, reboots and playbooks).
#[must_use]
pub fn uses_library(recipe: &Recipe) -> bool {
    recipe
        .resources()
        .any(|r| !matches!(r, Resource::Run(_) | Resource::Reboot | Resource::Playbook(_)))
}

#[cfg(test)]
mod tests;
