//! Secrets as Compose writes them: each declared once at the top, read from a variable of
//! Compose's own environment (`ISOLOOM_SECRET_<NAME>`, an input's file from the input's own
//! variable), and mounted as a file in the containers that get it. The file never holds a value:
//! whoever runs Compose sets the variables, from the secrets' stores or by hand.
//!
//! An input's file whose input has a default can't be a secret: a secret's variable must be set,
//! or Compose refuses to create the container, and a secret can't name a fallback. Nor can it be
//! a config mounted as is: Compose fills a config's `content` (`${NAME:-default}`) from the
//! variable, but an empty one (a default of `""`, an empty value) gives no file at all, and
//! Compose mounts the Compose file's own folder at the path instead. So each machine with such
//! files has a one-shot job of its own ([`inputs_service`]) that writes them, exactly their
//! values, into a volume of its own, and the machine (and its `setup` jobs) mount each file from
//! there, read-only, once the job is done:
//!
//! - the job reads each value from a config whose `content` is never empty, a `-` then
//!   `${NAME:-default}`, and writes it without that first byte (`tail -c +2`): an empty value is
//!   an empty file, a value its exact bytes, as a VM's file holds them;
//! - the value stays where a config's does: in no generated file (the default only), in no image
//!   layer, in no container's environment; Compose hands it to the job as a file;
//! - the job runs at every `up`, so a value given at the next start replaces the file in place.

use indexmap::IndexMap;
use isoloom_plan::secrets::DEFAULT_MODE;
use isoloom_plan::{Environment, MachinePlan, MachineShape, SecretFile, ValueSource};
use isoloom_render::container::PROBE_IMAGE;
use isoloom_spec::model::SecretSource;

use crate::file::{ComposeFile, Condition, Config, ConfigMount, LongMount, Mount, NetworkMode, Restart, Secret, Service, ServiceSecret, Volume, VolumeOptions};
use crate::interpolation;
use crate::names::inputs_service;

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
/// ([`mount_defaulted`]).
pub(super) fn of_machine(env: &Environment, mp: &MachinePlan) -> Vec<ServiceSecret> {
    mp.secrets.iter().filter(|s| default_of(env, s).is_none()).map(mounted).collect()
}

/// The inputs' files of `mp` whose input has a default, each with its index in the job's volume.
fn defaulted_files<'a>(env: &'a Environment, mp: &'a MachinePlan) -> impl Iterator<Item = (usize, &'a SecretFile)> {
    mp.secrets.iter().filter(|s| default_of(env, s).is_some()).enumerate()
}

/// Machine `machine`'s inputs' files with a default added to `svc` (the machine's own service, or
/// one of its jobs): each mounted read-only from the inputs job's volume at its path, once the
/// job wrote them. Nothing when it has none.
pub(super) fn mount_defaulted(env: &Environment, machine: &str, mp: &MachinePlan, svc: &mut Service) {
    let job = inputs_service(machine);
    let mut any = false;
    for (i, s) in defaulted_files(env, mp) {
        any = true;
        svc.volumes.push(Mount::Long(LongMount::Volume {
            source: job.clone(),
            target: s.path.clone(),
            read_only: true,
            volume: VolumeOptions { subpath: i.to_string() },
        }));
    }
    if any {
        svc.after(job, Condition::CompletedSuccessfully);
    }
}

/// Machine `machine`'s inputs job ([`inputs_service`]), when it has inputs' files with a default:
/// each value read from its config at `/in/<i>`, written without the config's first byte to
/// `/out/<i>` (truncated in place, so a running container's mount sees the new value), owned and
/// moded as the spec says (root, [`DEFAULT_MODE`] by default, as a config would be).
pub(super) fn inputs_job(env: &Environment, machine: &str, mp: &MachinePlan) -> Option<Service> {
    let files: Vec<(usize, &SecretFile)> = defaulted_files(env, mp).collect();
    if files.is_empty() {
        return None;
    }
    let writes: Vec<String> = files
        .iter()
        .map(|(i, s)| {
            let owner = format!("{}:{}", s.uid.unwrap_or(0), s.gid.unwrap_or(0));
            let mode = s.mode.unwrap_or(DEFAULT_MODE);
            format!("tail -c +2 /in/{i} > /out/{i}\nchown {owner} /out/{i}\nchmod {mode:04o} /out/{i}\n")
        })
        .collect();
    let script = format!("set -e\n{}", writes.concat());
    let configs = files
        .iter()
        .map(|(i, s)| ConfigMount {
            source: s.name.clone(),
            target: format!("/in/{i}"),
            uid: None,
            gid: None,
            mode: None,
        })
        .collect();
    Some(Service {
        image: Some(env.images.pull(PROBE_IMAGE)),
        entrypoint: vec!["/bin/sh".into(), "-c".into(), interpolation::literal(&script)],
        configs,
        volumes: vec![format!("{}:/out", inputs_service(machine)).into()],
        network_mode: Some(NetworkMode::None),
        restart: Some(Restart::No),
        ..Service::default()
    })
}

/// The inputs jobs' volumes, one per machine that has one ([`inputs_job`]).
pub(super) fn inputs_volumes(env: &Environment) -> IndexMap<String, Volume> {
    env.machines
        .iter()
        .filter(|(_, m)| m.shape == MachineShape::Container && defaulted_files(env, m).next().is_some())
        .map(|(name, _)| (inputs_service(name), Volume::default()))
        .collect()
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

/// The file's configs of the inputs' files with a default: each `-${NAME:-default}`, filled from
/// the input's variable when it is set and not empty, and never empty itself (the inputs job
/// drops the `-`).
pub(super) fn configs(env: &Environment) -> IndexMap<String, Config> {
    of_containers(env)
        .filter_map(|name| match env.secrets.get(name) {
            Some(SecretSource::Input { input }) => env.input_default(input).map(|d| (name, input, d)),
            _ => None,
        })
        .map(|(name, input, default)| {
            let config = Config {
                content: Some(format!("-{}", interpolation::input(input, Some(default)))),
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
        // The token's value: a config filled from its variable, else its default (`$` literal),
        // behind a first byte that keeps it from ever being empty.
        assert_eq!(yaml["configs"]["input-token"]["content"].as_str(), Some("-${TOKEN:-dev-$$token}"));
        assert!(yaml["secrets"].get("input-token").is_none(), "not a secret: Compose would refuse it unset");
        let web = &yaml["services"]["web"];
        // The key has no default: a secret, as before, whose variable Compose needs.
        assert_eq!(yaml["secrets"]["input-key"]["environment"].as_str(), Some("KEY"));
        assert_eq!(web["secrets"][0]["source"].as_str(), Some("input-key"));
        // An input in the environment falls back to its default.
        assert_eq!(web["environment"]["MODE"].as_str(), Some("${MODE:-demo}"));
    }

    #[test]
    fn an_input_file_with_a_default_is_written_by_a_job_and_mounted_read_only() {
        let file = compose_file(ComposeTarget::Docker, &environment(SPEC)).expect("renders").file;
        let yaml: serde_yaml_ng::Value = serde_yaml_ng::to_value(&file).expect("YAML");
        let web = &yaml["services"]["web"];
        let configs = web["configs"].as_sequence();
        assert!(
            configs.is_none_or(|c| c.iter().all(|m| m["source"] != "input-token")),
            "the machine never mounts the config"
        );
        // The machine mounts the file the inputs job wrote, read-only, once the job is done.
        let mount = web["volumes"]
            .as_sequence()
            .and_then(|v| v.iter().find(|m| m["target"] == "/run/lab/token"))
            .expect("the token's file is mounted");
        assert_eq!(
            (mount["source"].as_str(), mount["volume"]["subpath"].as_str(), mount["read_only"].as_bool()),
            (Some("isoloom-inputs-web"), Some("0"), Some(true))
        );
        let after = web["depends_on"]["isoloom-inputs-web"]["condition"].as_str();
        assert_eq!(after, Some("service_completed_successfully"));
        assert!(yaml["volumes"].get("isoloom-inputs-web").is_some(), "the job's volume is declared");
        // The job: the value without the config's first byte, the spec's mode, no network.
        let job = &yaml["services"]["isoloom-inputs-web"];
        let carrier = &job["configs"][0];
        assert_eq!((carrier["source"].as_str(), carrier["target"].as_str()), (Some("input-token"), Some("/in/0")));
        assert_eq!((job["network_mode"].as_str(), job["restart"].as_str()), (Some("none"), Some("no")));
        let script = job["entrypoint"][2].as_str().expect("a script");
        let writes = ["tail -c +2 /in/0 > /out/0", "chown 0:0 /out/0", "chmod 0400 /out/0"];
        assert!(writes.iter().all(|w| script.contains(w)), "{script}");
    }
}
