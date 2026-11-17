//! Secrets as Compose writes them: each declared once at the top, read from a variable of
//! Compose's own environment (`ISOLOOM_SECRET_<NAME>`, an input's file from the input's own
//! variable), and mounted as a file in the containers that get it. The file never holds a value:
//! whoever runs Compose sets the variables, from the secrets' stores or by hand.
//!
//! An input's file whose input has a default is a config instead: a secret's variable must be
//! set, or Compose refuses to create the container, and a secret can't name a fallback. The
//! config's `content` is `${NAME:-default}`, which Compose fills from the same variable, the
//! default when it is unset or empty, and copies in at the same path with the same owner and
//! mode.

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachinePlan, MachineShape, SecretFile, ValueSource};
use isoloom_spec::model::SecretSource;

use crate::file::{ComposeFile, Config, ConfigMount, Secret, ServiceSecret};
use crate::interpolation;

/// The default of `f`'s input, when `f` is an input's file and the input declares one.
fn default_of<'a>(env: &'a Environment, f: &SecretFile) -> Option<&'a str> {
    match &f.source {
        ValueSource::Input(input) => env.input_default(input),
        ValueSource::Secret(_) => None,
    }
}

/// Whether secret `name` is an input's file whose input has a default (a config, not a secret).
fn defaulted(env: &Environment, name: &str) -> bool {
    matches!(env.secrets.get(name), Some(SecretSource::Input { input }) if env.input_default(input).is_some())
}

/// Where a container has secret file `s`: at its path, with its owner and mode when the spec
/// sets them.
fn mounted(s: &SecretFile) -> ServiceSecret {
    ServiceSecret {
        source: s.name.clone(),
        target: s.path.clone(),
        uid: s.uid.map(|u| u.to_string()),
        gid: s.gid.map(|g| g.to_string()),
        mode: s.mode.map(|m| format!("{m:04o}")),
    }
}

/// The secrets a container gets ([`mounted`]): all but the inputs' files with a default
/// ([`as_configs`]).
pub(super) fn of_machine(env: &Environment, mp: &MachinePlan) -> Vec<ServiceSecret> {
    mp.secrets.iter().filter(|s| default_of(env, s).is_none()).map(mounted).collect()
}

/// Where a container has the inputs' files whose input has a default: each the config of its
/// secret's name ([`configs`]), mounted as the secret would be.
pub(super) fn as_configs(env: &Environment, mp: &MachinePlan) -> Vec<ConfigMount> {
    let config = |m: ServiceSecret| ConfigMount {
        source: m.source,
        target: m.target,
        uid: m.uid,
        gid: m.gid,
        mode: m.mode,
    };
    mp.secrets.iter().filter(|s| default_of(env, s).is_some()).map(mounted).map(config).collect()
}

/// The secrets a container gets, by name, in the order the plan holds them (the spec's).
fn of_containers(env: &Environment) -> impl Iterator<Item = &String> {
    let containers: Vec<&MachinePlan> = env.machines.values().filter(|m| m.shape == MachineShape::Container).collect();
    env.secrets
        .keys()
        .filter(move |name| containers.iter().any(|m| m.secrets.iter().any(|s| &s.name == *name)))
}

/// The file's secrets: every one a container gets but an input's file with a default.
pub(super) fn of_file(env: &Environment) -> IndexMap<String, Secret> {
    of_containers(env)
        .filter(|name| !defaulted(env, name))
        .map(|name| {
            let environment = isoloom_render::secrets::compose_variable(env, name);
            (name.clone(), Secret { environment })
        })
        .collect()
}

/// The file's configs of the inputs' files with a default: each `${NAME:-default}`, filled from
/// the input's variable when it is set and not empty.
pub(super) fn configs(env: &Environment) -> IndexMap<String, Config> {
    of_containers(env)
        .filter_map(|name| match env.secrets.get(name) {
            Some(SecretSource::Input { input }) => env.input_default(input).map(|d| (name, input, d)),
            _ => None,
        })
        .map(|(name, input, default)| {
            let config = Config {
                content: Some(interpolation::input(input, Some(default))),
                ..Config::default()
            };
            (name.clone(), config)
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

/// The usage line naming the inputs delivered as files that fall back to their default, when
/// the file has any.
pub(super) fn defaults_usage(env: &Environment) -> String {
    let mut inputs: Vec<&str> = Vec::new();
    for name in of_containers(env) {
        if let Some(SecretSource::Input { input }) = env.secrets.get(name)
            && env.input_default(input).is_some()
            && !inputs.contains(&input.as_str())
        {
            inputs.push(input);
        }
    }
    if inputs.is_empty() {
        return String::new();
    }
    format!(
        "# Inputs delivered as files, each its spec's default when unset or empty: set {} in Compose's environment for `up` and `start` to give another value.\n",
        inputs.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use crate::backend::ComposeTarget;
    use crate::render::compose_file;
    use crate::testing::environment;

    /// `TOKEN` (sensitive, a file) and `MODE` (in the environment) have a default; `KEY` (a file)
    /// has none.
    const SPEC: &str = "
version: 1
name: defaults
inputs:
  TOKEN: { default: dev-$token }
  KEY: {}
  MODE: { sensitive: false, default: demo }
networks:
  lan: { cidr: 10.20.0.0/24 }
machines:
  web:
    networks: { lan: 10 }
    app: { image: nginx }
    environment: { MODE: { input: MODE } }
    files:
      - { path: /run/lab/token, input: TOKEN, mode: '0400' }
      - { path: /run/lab/key, input: KEY }
";

    #[test]
    fn an_input_with_a_default_falls_back_to_it_and_one_without_stays_a_secret() {
        let file = compose_file(ComposeTarget::Docker, &environment(SPEC)).expect("renders").file;
        let yaml: serde_yaml_ng::Value = serde_yaml_ng::to_value(&file).expect("YAML");
        // The token's file: a config filled from its variable, else its default (`$` literal).
        assert_eq!(yaml["configs"]["input-token"]["content"].as_str(), Some("${TOKEN:-dev-$$token}"));
        assert!(yaml["secrets"].get("input-token").is_none(), "not a secret: Compose would refuse it unset");
        let web = &yaml["services"]["web"];
        let mount = web["configs"]
            .as_sequence()
            .and_then(|c| c.iter().find(|m| m["source"] == "input-token"))
            .expect("the token's file is mounted");
        assert_eq!((mount["target"].as_str(), mount["mode"].as_str()), (Some("/run/lab/token"), Some("0400")));
        // The key has no default: a secret, as before, whose variable Compose needs.
        assert_eq!(yaml["secrets"]["input-key"]["environment"].as_str(), Some("KEY"));
        assert_eq!(web["secrets"][0]["source"].as_str(), Some("input-key"));
        // An input in the environment falls back to its default.
        assert_eq!(web["environment"]["MODE"].as_str(), Some("${MODE:-demo}"));
    }
}
