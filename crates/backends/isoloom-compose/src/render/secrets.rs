//! Secrets as Compose writes them: each declared once at the top, read from a variable of
//! Compose's own environment (`ISOLOOM_SECRET_<NAME>`, set by `isoloom run` from the secret's
//! store), and mounted as a file in the containers that get it. The file never holds a value.

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachinePlan, MachineShape};
use isoloom_spec::model::{SecretSource, secret_variable};

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

/// The file's secrets: every one a container gets, in the order the spec declares them.
pub(super) fn of_file(env: &Environment) -> IndexMap<String, Secret> {
    let containers: Vec<&MachinePlan> = env.machines.values().filter(|m| m.shape == MachineShape::Container).collect();
    env.spec()
        .secrets
        .iter()
        .filter(|(name, _)| containers.iter().any(|m| m.secrets.iter().any(|s| &s.name == *name)))
        .map(|(name, source)| {
            // An input's file reads the input's own variable; a secret, the one `isoloom run`
            // fills from its store.
            let environment = match source {
                SecretSource::Input { input } => input.clone(),
                _ => secret_variable(name),
            };
            (name.clone(), Secret { environment })
        })
        .collect()
}

/// The usage line saying where the secrets' values come from, when the file has any: a
/// secret's from its store, an input's given at each start.
pub(super) fn usage(file: &ComposeFile) -> String {
    if file.secrets.is_empty() {
        return String::new();
    }
    let vars: Vec<&str> = file.secrets.values().map(|s| s.environment.as_str()).collect();
    let inputs = file.secrets.values().any(|s| !s.environment.starts_with("ISOLOOM_SECRET_"));
    if inputs {
        format!(
            "# Secrets and inputs, delivered as files: set {} for `up` (each value, never in the file).\n",
            vars.join(", ")
        )
    } else {
        format!(
            "# Secrets: `isoloom run docker` reads them from their stores; by hand, set {} for `up`.\n",
            vars.join(", ")
        )
    }
}
