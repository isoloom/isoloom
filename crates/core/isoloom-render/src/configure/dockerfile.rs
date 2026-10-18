//! The Dockerfile Isoloom generates from a container's recipe (design §2.2–§2.3): `FROM` its
//! OS's official image (pinned by digest), the project folder (`/opt/isoloom`, the same path a
//! VM has) as its working folder, then each step in order. Scripts and copies are the
//! Dockerfile's own `COPY` and `RUN`, a template a `COPY` of the file Isoloom filled in, and every
//! other step one `RUN` of the shell library's call. A service doesn't run at build: Isoloom's
//! runner ([`super::runner`]) is the image's entrypoint, and starts it. The project folder is the
//! build context.

use isoloom_plan::configure::{ContainerProcess, Resource, RunPlan, TemplatePlan};
use isoloom_plan::{Build, Environment, Image, MachineShape};
use isoloom_spec::syntax::Mode;

use super::{Configured, rendered_path, runner, shell};
use crate::steps::PROJECT_DIR;
use crate::{GeneratedFile, header};

/// Where the shell library is in the image.
const LIBRARY: &str = "/usr/local/lib/isoloom/configure.sh";

/// Where the runner is in the image.
const RUNNER: &str = "/usr/local/bin/isoloom-run";

/// The generated Dockerfiles of `env`'s containers, each at its path (its base image pulled
/// through the registry mirrors), with what each builds from: the shell library, the runner
/// and the templates filled in.
#[must_use]
pub fn files(env: &Environment) -> Vec<GeneratedFile> {
    let mut out = Vec::new();
    for (name, mp) in &env.machines {
        let (
            Some(Image::Container {
                build: Some(Build::Generated { dockerfile, base, process }),
                ..
            }),
            Some(c),
            MachineShape::Container,
        ) = (&mp.image, Configured::of(env, name), mp.shape)
        else {
            continue;
        };
        out.push(GeneratedFile {
            path: dockerfile.clone(),
            contents: render(&c, &env.images.pull(base), process),
        });
        if super::uses_library(c.recipe()) && !out.iter().any(|f| f.path == shell::LIBRARY_PATH) {
            out.push(shell::library_file());
        }
        out.extend(runner::file(&c, process));
        out.extend(templates(&c));
    }
    out
}

/// The Dockerfile of machine `c`'s image, from `base`, running `process`.
#[must_use]
pub fn render(c: &Configured<'_>, base: &str, process: &ContainerProcess) -> String {
    let library = if super::uses_library(c.recipe()) {
        format!("COPY {} {LIBRARY}\n", shell::LIBRARY_PATH)
    } else {
        String::new()
    };
    // `start` is the runner's, at each start; the build installs and configures.
    let recipe = c.recipe();
    let steps: String = recipe.install.iter().chain(&recipe.configure).filter_map(|r| instructions(c, r)).collect();
    let entrypoint = if process.is_runner() {
        // The image's own command stays its arguments, which the runner hands its entrypoint.
        let command = match process {
            ContainerProcess::Started { command, .. } => serde_json::to_string(command).unwrap_or_else(|_| "[]".to_owned()),
            _ => "[]".to_owned(),
        };
        format!(
            "{}COPY --chmod=0755 {} {RUNNER}\nENTRYPOINT [\"{RUNNER}\"]\nCMD {command}\n",
            start_files(c),
            runner::path(c.machine())
        )
    } else {
        String::new()
    };
    // The project folder is the working folder only when the build or the runner uses it; a bare
    // passthrough of an official image (no steps, the image's own entrypoint) keeps the image's
    // own working folder, which an app may rely on to find its files.
    let workdir = if process.is_runner() || !steps.is_empty() {
        format!("WORKDIR {PROJECT_DIR}\n")
    } else {
        String::new()
    };
    format!(
        "{}# The container form of machine `{}`: its OS's image and its steps.\nFROM {base}\n{workdir}{library}{steps}{entrypoint}",
        header("#"),
        c.machine()
    )
}

/// The project files the recipe's `start` steps read when the runner runs them at each start
/// (their scripts, their copies' sources), copied into the image's project folder.
fn start_files(c: &Configured<'_>) -> String {
    c.recipe()
        .start
        .iter()
        .filter_map(|r| match r {
            Resource::Run(run) => Some(run.script.as_str()),
            Resource::Copy(cp) => Some(cp.from.as_str()),
            _ => None,
        })
        .map(|path| format!("COPY {path} {path}\n"))
        .collect::<Vec<_>>()
        .concat()
}

/// One step's instructions; `None` for a step a container doesn't run at build (a service, which
/// the runner starts; a firewall, which a container doesn't have).
fn instructions(c: &Configured<'_>, step: &Resource) -> Option<String> {
    let lines = match step {
        Resource::Run(run) => script(run),
        Resource::Copy(cp) => {
            let owner = cp.owner.as_ref().map(|o| format!("--chown={o} ")).unwrap_or_default();
            let mode = cp.mode.map(|m| format!("--chmod={m} ")).unwrap_or_default();
            format!("COPY {owner}{mode}{} {}\n", cp.from, cp.to)
        }
        Resource::Template(t) => template(c, t),
        Resource::Service(_) | Resource::Firewall(_) | Resource::Reboot | Resource::Playbook(_) => return None,
        other => {
            let call = shell::call(c, other)?;
            let ignore = if shell::literal_dollar(&call) { "# hadolint ignore=SC2016\n" } else { "" };
            format!("{ignore}RUN . {LIBRARY} && {call}")
        }
    };
    Some(lines)
}

/// A project script, copied and run (behind its `creates`).
fn script(run: &RunPlan) -> String {
    let script = &run.script;
    let line = match &run.creates {
        Some(path) => format!("[ -e {path} ] || sh {script}"),
        None => format!("sh {script}"),
    };
    format!("COPY {script} {script}\nRUN {line}\n")
}

/// A template: the file Isoloom filled in, copied with its owner, group and mode (`0644` when it
/// gives none); a template that can't be filled in fails the build, saying why.
fn template(c: &Configured<'_>, t: &TemplatePlan) -> String {
    if let Err(why) = c.template(&t.from) {
        return format!("RUN . {LIBRARY} && {}\n", shell::words("isoloom_fail", [why]));
    }
    let chown = match (&t.owner, &t.group) {
        (Some(o), Some(g)) => format!("--chown={o}:{g} "),
        (Some(o), None) => format!("--chown={o} "),
        (None, Some(g)) => format!("--chown=root:{g} "),
        (None, None) => String::new(),
    };
    let mode = t.mode.unwrap_or(Mode::CONFIG);
    format!("COPY {chown}--chmod={mode} {} {}\n", rendered_path(c.machine(), &t.from), t.to)
}

/// The templates machine `c` builds with, filled in: one file each, at
/// [`rendered_path`] (those that can't be filled in fail the build instead).
fn templates(c: &Configured<'_>) -> Vec<GeneratedFile> {
    c.recipe()
        .resources()
        .filter_map(|r| match r {
            Resource::Template(t) => Some(GeneratedFile {
                path: rendered_path(c.machine(), &t.from),
                contents: c.template(&t.from).ok()?,
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests;
