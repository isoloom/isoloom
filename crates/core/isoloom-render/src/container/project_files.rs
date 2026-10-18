//! The project files Compose reads from its own environment: a container's file from the project
//! with an owner or a mode (outside Swarm, Compose ignores both for a `file` config), copied in
//! from a variable whoever starts Compose sets first (the runner, a Docker host's start
//! script).

use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::model::MachineFile;
use isoloom_spec::shell::quote;

/// The variable of Compose's own environment holding machine `machine`'s `n`th file, when it
/// is a project file with an owner or a mode (`ISOLOOM_FILE_WEB_1`). Machine names are
/// kebab-case, so two files never share one.
#[must_use]
pub fn variable(machine: &str, n: usize) -> String {
    format!("ISOLOOM_FILE_{}_{n}", machine.to_ascii_uppercase().replace('-', "_"))
}

/// Whether file `f` is copied in from Compose's environment: a project file the spec gives an
/// owner or a mode.
#[must_use]
pub fn copied_from_project(f: &MachineFile) -> bool {
    f.source.is_some() && f.sets_owner_or_mode()
}

/// The project files Compose reads from its own environment, as (variable, path in the
/// project).
#[must_use]
pub fn project_variables(env: &Environment) -> Vec<(String, String)> {
    env.machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Container)
        .flat_map(|(machine, mp)| {
            mp.files
                .iter()
                .enumerate()
                .filter(|(_, f)| copied_from_project(f))
                .filter_map(move |(n, f)| Some((variable(machine, n), f.source.clone()?)))
        })
        .collect()
}

/// On a Docker host, in the project's folder before `docker compose up`: each project file with
/// an owner or a mode read into its variable, its last line breaks kept, as commands joined by
/// `&&` (ending with one). Nothing when there are none.
#[must_use]
pub fn host_environment(env: &Environment) -> String {
    project_variables(env)
        .into_iter()
        .map(|(var, path)| format!("{var}=\"$(cat {}; printf x)\" && {var}=\"${{{var}%x}}\" && export {var} && ", quote(&path)))
        .collect::<Vec<_>>()
        .concat()
}
