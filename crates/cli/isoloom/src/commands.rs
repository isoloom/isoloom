//! One module per subcommand: its arguments (`Args`) and what it does (`run`), writing to the
//! console and giving its verdict, or the error that stopped it.

pub(crate) mod check;
pub(crate) mod defaults;
pub(crate) mod docs;
pub(crate) mod generate;
pub(crate) mod graph;
pub(crate) mod import;
pub(crate) mod inspect;
pub(crate) mod matrix;
pub(crate) mod message;
pub(crate) mod profiles;
pub(crate) mod report;
pub(crate) mod resources;
pub(crate) mod schema;
pub(crate) mod targets;
pub(crate) mod validate;

use std::path::PathBuf;

use isoloom_targets::GenerateError;

use crate::cli::Command;
use crate::console::Console;
use crate::error::{CliError, Result};
use crate::exit::Verdict;
use crate::spec::{self, Profile};

// The project folder a command works in (holding isoloom.yml), the current one unless named;
// flattened into each command's arguments (no doc comment: clap would print it in `--help`).
#[derive(Debug, clap::Args)]
pub(crate) struct Project {
    /// The project folder (holding isoloom.yml).
    #[arg(default_value = ".")]
    pub(crate) dir: PathBuf,
}

// The arguments of the commands that read a project and answer as text or `--json`
// (`profiles`, `resources`, `targets`), with what the lab is read with besides the spec.
#[derive(Debug, clap::Args)]
pub(crate) struct Listing {
    #[command(flatten)]
    pub(crate) project: Project,
    /// JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
    #[command(flatten)]
    pub(crate) settings: spec::Settings,
}

/// The targets `generate_all` left out, each a refusal of the lab; those left out for any other
/// reason (a backend failed) as the error, every one of them, so
/// neither `generate` nor `check` passes over a target whose files it couldn't make.
fn refusals(skipped: Vec<GenerateError>) -> Result<Vec<GenerateError>> {
    let (refused, failed): (Vec<_>, Vec<_>) = skipped.into_iter().partition(GenerateError::is_refusal);
    let mut failed: Vec<isoloom_targets::Error> = failed.into_iter().map(isoloom_targets::Error::Generate).collect();
    match failed.len() {
        0 => Ok(refused),
        1 => Err(failed.remove(0).into()),
        _ => Err(CliError::Backends(failed)),
    }
}

/// Runs the subcommand asked for, with the profile `main` worked out.
pub(crate) fn run(command: Command, profile: &Profile, console: &mut Console) -> Result<Verdict> {
    match command {
        Command::Validate(a) => validate::run(&a, profile, console),
        Command::Targets(a) => targets::run(&a, profile, console),
        Command::Resources(a) => resources::run(&a, profile, console),
        Command::Generate(a) => generate::run(&a, profile, console),
        Command::Check(a) => check::run(&a, profile, console),
        Command::Defaults(a) => defaults::run(&a, profile, console),
        Command::Graph(a) => graph::run(&a, profile, console),
        Command::Report(a) => report::run(&a, profile, console),
        Command::Message(a) => message::run(&a, profile, console),
        Command::Inspect(a) => inspect::run(a, profile, console),
        Command::Profiles(a) => profiles::run(&a, console),
        Command::Schema(a) => schema::run(&a, console),
        Command::Import(i) => import::run(i, console),
        Command::Docs(a) => docs::run(&a, console),
        Command::Matrix(a) => matrix::run(&a, console),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use isoloom_spec::model::Target;
    use isoloom_targets::GenerateError;

    use super::refusals;
    use crate::error::CliError;

    #[test]
    fn a_refusal_is_left_out_and_a_failure_is_an_error() {
        let refused = GenerateError::Unsupported {
            target: Target::Proxmox,
            what: "machine `w`: no image".to_owned(),
        };
        let kept = refusals(vec![refused.clone(), GenerateError::NotPossible(Target::Kubernetes)]);
        assert!(matches!(
            kept.as_deref(),
            Ok([GenerateError::Unsupported { .. }, GenerateError::NotPossible(_)])
        ));
        let failed = GenerateError::Internal {
            target: Target::Vagrant,
            what: "the run manifest refused a value of this backend".to_owned(),
            source: Arc::new(std::io::Error::other("bad name")),
        };
        let stopped = refusals(vec![refused, failed]);
        assert!(
            matches!(
                &stopped,
                Err(CliError::Targets(isoloom_targets::Error::Generate(GenerateError::Internal {
                    target: Target::Vagrant,
                    ..
                })))
            ),
            "{stopped:?}"
        );
        if let Err(e) = &stopped {
            let rendered = crate::diagnostic::render(e);
            assert!(
                rendered.contains("error: vagrant: the run manifest refused a value of this backend\n  caused by: bad name"),
                "{rendered}"
            );
        }
    }

    #[test]
    fn every_failed_backend_is_reported() {
        let failed = |target| GenerateError::Internal {
            target,
            what: "the run manifest refused a value of this backend".to_owned(),
            source: Arc::new(std::io::Error::other("bad name")),
        };
        let stopped = refusals(vec![failed(Target::Vagrant), failed(Target::DockerVm)]);
        let Err(e) = &stopped else { panic!("{stopped:?}") };
        let rendered = crate::diagnostic::render(e);
        for target in ["vagrant", "docker-vm"] {
            assert!(
                rendered.contains(&format!(
                    "error: {target}: the run manifest refused a value of this backend\n  caused by: bad name"
                )),
                "{rendered}"
            );
        }
    }
}
