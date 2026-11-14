//! Secrets as Compose writes them: each declared once at the top, read from a variable of
//! Compose's own environment (`ISOLOOM_SECRET_<NAME>`, an input's file from the input's own
//! variable), and mounted as a file in the containers that get it. The file never holds a value:
//! whoever runs Compose sets the variables, from the secrets' stores or by hand.

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachinePlan, MachineShape};
use isoloom_spec::model::SecretSource;

use crate::file::{ComposeFile, Secret, ServiceSecret};

/// The secrets a container gets, at their paths, with their owner and mode when the spec sets
/// them.
pub(super) fn of_machine(mp: &MachinePlan) -> Vec<ServiceSecret> {
    mp.secrets
        .iter()
        .map(|s| ServiceSecret {
            source: s.name.clone(),
            target: s.path.clone(),
            uid: s.uid.map(|u| u.to_string()),
            gid: s.gid.map(|g| g.to_string()),
            mode: s.mode.map(|m| format!("{m:04o}")),
        })
        .collect()
}

/// The file's secrets: every one a container gets, in the order the plan holds them (the
/// spec's).
pub(super) fn of_file(env: &Environment) -> IndexMap<String, Secret> {
    let containers: Vec<&MachinePlan> = env.machines.values().filter(|m| m.shape == MachineShape::Container).collect();
    env.secrets
        .keys()
        .filter(|name| containers.iter().any(|m| m.secrets.iter().any(|s| &s.name == *name)))
        .map(|name| {
            let environment = isoloom_render::secrets::compose_variable(env, name);
            (name.clone(), Secret { environment })
        })
        .collect()
}

/// The usage line saying where the secrets' values come from, when the file has any: the
/// variables of Compose's environment that hold them, set for `up` and `start`.
pub(super) fn usage(env: &Environment, file: &ComposeFile) -> String {
    if file.secrets.is_empty() {
        return String::new();
    }
    let vars: Vec<&str> = file.secrets.values().map(|s| s.environment.as_str()).collect();
    let inputs = file.secrets.keys().any(|n| matches!(env.secrets.get(n), Some(SecretSource::Input { .. })));
    let what = if inputs { "Secrets and inputs, delivered as files" } else { "Secrets" };
    format!(
        "# {what}: set {} in Compose's environment for `up` and `start` (each value, never in this file).\n",
        vars.join(", ")
    )
}
