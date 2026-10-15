//! The kustomizations: the environment's (its manifests, and the project's scripts as a
//! `ConfigMap` read from the project at apply time) and the checks' (their runners). Typed, so
//! the command line reads back which project files the chart copies ([`project_scripts`]).

use serde::{Deserialize, Serialize};

use crate::layout::ROOT;

/// A `kustomization.yaml`, the fields Isoloom writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Kustomization {
    /// `kustomize.config.k8s.io/v1beta1`.
    pub(crate) api_version: String,
    /// `Kustomization`.
    pub(crate) kind: String,
    /// The namespace every object goes in.
    pub(crate) namespace: String,
    /// The manifests, next to it.
    pub(crate) resources: Vec<String>,
    /// How generated `ConfigMap`s are named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) generator_options: Option<GeneratorOptions>,
    /// `ConfigMap`s made from files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) config_map_generator: Vec<ConfigMapGenerator>,
}

/// How generated `ConfigMap`s are named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorOptions {
    /// Their names as written (the pods name them), without kustomize's hash.
    pub(crate) disable_name_suffix_hash: bool,
}

/// A `ConfigMap` made from files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ConfigMapGenerator {
    /// The `ConfigMap`.
    pub(crate) name: String,
    /// Its files: `<key>=<path>`, or a path whose file name is the key.
    pub(crate) files: Vec<String>,
}

/// The `ConfigMap` of the project's scripts the pods run (init jobs, checks).
pub(crate) const SCRIPTS: &str = "isoloom-scripts";

/// The `ConfigMap` of the check runners.
pub(crate) const CHECK_SCRIPTS: &str = "isoloom-check-runners";

impl Kustomization {
    /// The objects in `resources`, in `namespace`, plus a `ConfigMap` `name` of `files` when
    /// there are any.
    pub(crate) fn new(namespace: &str, resource: &str, name: &str, files: Vec<String>) -> Kustomization {
        let generates = !files.is_empty();
        Kustomization {
            api_version: "kustomize.config.k8s.io/v1beta1".into(),
            kind: "Kustomization".into(),
            namespace: namespace.into(),
            resources: vec![resource.into()],
            generator_options: generates.then_some(GeneratorOptions {
                disable_name_suffix_hash: true,
            }),
            config_map_generator: if generates {
                vec![ConfigMapGenerator { name: name.into(), files }]
            } else {
                Vec::new()
            },
        }
    }
}

/// A `ConfigMap` key for a project file (keys allow letters, digits, `-`, `_` and `.`).
pub(crate) fn key(path: &str) -> String {
    path.replace('/', "__")
}

/// The environment's kustomization: its manifests, and the project's `scripts` (paths from the
/// project folder) as the [`SCRIPTS`] `ConfigMap`.
pub(crate) fn environment(namespace: &str, scripts: &[String]) -> Kustomization {
    let files = scripts.iter().map(|f| format!("{}={ROOT}/{f}", key(f))).collect();
    Kustomization::new(namespace, "environment.yaml", SCRIPTS, files)
}

/// The project's scripts an environment's kustomization reads, as (`ConfigMap` key, path from the
/// project folder).
pub(crate) fn project_scripts(kustomization: &str) -> Result<Vec<(String, String)>, serde_yaml_ng::Error> {
    let k: Kustomization = serde_yaml_ng::from_str(kustomization)?;
    let files = k.config_map_generator.into_iter().filter(|g| g.name == SCRIPTS).flat_map(|g| g.files);
    Ok(files
        .filter_map(|f| {
            let (key, path) = f.split_once('=')?;
            let path = path.strip_prefix(ROOT)?.strip_prefix('/')?;
            Some((key.to_owned(), path.to_owned()))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn the_scripts_read_back_are_those_written() {
        let scripts = vec!["build/cache/seed.sh".to_owned(), "checks/up.sh".to_owned()];
        let text = serde_yaml_ng::to_string(&environment("isoloom-x", &scripts)).unwrap();
        assert_eq!(
            project_scripts(&text).unwrap(),
            [
                ("build__cache__seed.sh".to_owned(), "build/cache/seed.sh".to_owned()),
                ("checks__up.sh".to_owned(), "checks/up.sh".to_owned())
            ]
        );
    }

    #[test]
    fn no_scripts_no_config_map() {
        let k = environment("isoloom-x", &[]);
        assert!(k.generator_options.is_none() && k.config_map_generator.is_empty());
        let text = serde_yaml_ng::to_string(&k).unwrap();
        assert_eq!(project_scripts(&text).unwrap(), Vec::<(String, String)>::new());
    }

    proptest! {
        #[test]
        fn any_scripts_read_back_are_those_written(scripts in prop::collection::vec("[a-z0-9_.-]{1,8}(/[a-z0-9_.-]{1,8}){0,2}", 0..4)) {
            let text = serde_yaml_ng::to_string(&environment("isoloom-x", &scripts)).expect("writes");
            let expected: Vec<(String, String)> = scripts.iter().map(|s| (key(s), s.clone())).collect();
            prop_assert_eq!(project_scripts(&text).expect("reads"), expected);
        }
    }
}
